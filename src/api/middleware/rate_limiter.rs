use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use std::net::SocketAddr;

use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::errors::{error_codes, ErrorBody, ErrorResponse};

// Lua script for an atomic Token Bucket rate limiter.
// Returns the remaining token count (float), or -1.0 if denied.
// KEYS[1]  = the rate limit key
// ARGV[1]  = the window duration in seconds
// ARGV[2]  = the max allowed tokens (bucket capacity)
// ARGV[3]  = current timestamp in milliseconds
const RATE_LIMIT_LUA: &str = r#"
local key = KEYS[1]
local window_secs = tonumber(ARGV[1])
local max_tokens = tonumber(ARGV[2])
local now_ms = tonumber(ARGV[3])

local refill_rate_per_sec = max_tokens / window_secs

local bucket = redis.call('HMGET', key, 'tokens', 'last_refill_ts')
local tokens = tonumber(bucket[1])
local last_refill_ts = tonumber(bucket[2])

if tokens == nil then
    tokens = max_tokens
    last_refill_ts = now_ms
else
    local elapsed_ms = math.max(0, now_ms - last_refill_ts)
    local refill = (elapsed_ms / 1000) * refill_rate_per_sec
    tokens = math.min(max_tokens, tokens + refill)
    last_refill_ts = now_ms
end

if tokens < 1 then
    return -1.0
end

tokens = tokens - 1
redis.call('HMSET', key, 'tokens', tostring(tokens), 'last_refill_ts', tostring(last_refill_ts))
redis.call('EXPIRE', key, math.ceil(window_secs))

return tokens
"#;

/// Extracts the true client IP, respecting Render's load balancer headers.
/// Falls back to the raw socket address if no proxy header is present.
fn extract_ip(req: &Request, socket_addr: &SocketAddr) -> String {
    // X-Forwarded-For Header Check (Primary Proxy Method)
    // Reverse proxies and load balancers (such as Render, Cloudflare, Nginx, or AWS ALB) append client IPs to this header.
    // So we check, if the HTTP request contains the X-Forwarded-For header.
    // If present and valid UTF-8, we split the value by commas (val.split(',')).
    // Since the value can be a chain of IPs representing each hop (e.g., client_ip, proxy_1_ip, proxy_2_ip), the leftmost (first) IP is always the original client.
    // So we extract the first IP, trim any whitespace, and return it
    if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
        // forwared_for is the header value
        if let Ok(val) = forwarded_for.to_str() {
            if let Some(first_ip) = val.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    // X-Real-IP Header Check (Secondary Proxy Method)
    // Some simpler reverse proxies or configurations only set X-Real-IP instead of a full chain in X-Forwarded-For
    // If X-Forwarded-For is not present, it checks for the X-Real-IP header.
    // If present and valid UTF-8, it trims any leading/trailing whitespace and returns it.
    if let Some(real_ip) = req.headers().get("X-Real-IP") {
        if let Ok(val) = real_ip.to_str() {
            return val.trim().to_string();
        }
    }

    // Fallback: use the raw TCP connection's remote address.
    // For local development or cases where traffic is hitting the server directly without a reverse proxy, no headers will be set.
    // It falls back to the physical SocketAddr (socket_addr) of the incoming TCP connection.
    // It extracts just the IP portion, discarding the port number to avoid treating the same IP on different source ports as distinct clients.
    socket_addr.ip().to_string()
}

/// Attempts to extract the user_id from the Bearer token in the Authorization header.
/// Returns `Some(user_id)` if the token is present and cryptographically valid.
/// Returns `None` if the token is absent, malformed, or expired — with no error response.
///
/// This is intentionally a "best-effort" read: we never reject a request here.
/// The real auth enforcement is done by the `JwtClaims` extractor on protected handlers.
fn try_extract_user_id(req: &Request, secret: &[u8]) -> Option<i32> {
    let auth_header = req.headers().get("Authorization")?.to_str().ok()?;
    let token = auth_header.strip_prefix("Bearer ")?;
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret),
        &Validation::default(),
    )
    .ok()?;
    Some(token_data.claims.sub)
}

/// Runs the atomic Lua rate-limit script against Redis.
/// Returns `Ok(remaining_tokens)` on success, or `Err` if Redis is unavailable.
/// A negative return value indicates the request is rate-limited.
async fn check_rate_limit(
    conn: &mut deadpool_redis::Connection,
    key: &str,
    window_secs: u64,
    max_requests: u64,
) -> Result<f64, redis::RedisError> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let remaining: f64 = redis::Script::new(RATE_LIMIT_LUA)
        .key(key)
        .arg(window_secs)
        .arg(max_requests)
        .arg(now_ms)
        .invoke_async(conn)
        .await?;
    Ok(remaining)
}

/// **Auth Rate Limiter** — Strict tier for authentication routes.
/// Limits: 3 requests per 10-second window per IP.
/// Auth routes (/login, /register, etc.) are always rate-limited by IP because
/// the user is unauthenticated by definition at this point in the request.
/// **Fail-Open**: If Redis is unreachable, the request is allowed through and
/// the error is logged. This keeps the API online even if Redis has a blip.
pub async fn auth_rate_limiter(
    State(state): State<SharedState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>, // Extract user's physical IP address and port, provided by the load balancer.
    req: Request,                                      // The actual HTTP data (headers, body, URL)
    next: Next, // A representation of the next layer in the pipeline. If we allow the request to pass, we call next.run(req) to send it to the next layer.
) -> Response {
    const WINDOW_SECS: u64 = 10; // duration of the window in seconds
    const MAX_REQUESTS: u64 = 3; // maximum number of requests allowed in the window

    let ip = extract_ip(&req, &socket_addr);
    let key = format!("rl:auth:ip:{}", ip); // rl:auth:ip: is the prefix for the rate limit key in Redis,ip is the user's IP address

    let mut conn = match state.redis_pool.get().await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(
                error = %err,
                ip = %ip,
                "Failed to acquire Redis connection from pool for auth rate limiter — failing open"
            );
            return next.run(req).await;
        }
    };

    match check_rate_limit(&mut conn, &key, WINDOW_SECS, MAX_REQUESTS).await {
        Ok(count) if count < 0.0 => {
            // used match guard
            tracing::warn!(
                ip = %ip,
                max = MAX_REQUESTS,
                "Auth rate limit exceeded"
            );
            let body = ErrorResponse {
                error: ErrorBody {
                    code: error_codes::RATE_LIMIT_EXCEEDED.to_string(),
                    message: "Too many requests. Please wait before trying again.".to_string(),
                },
            };
            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("X-RateLimit-Limit", MAX_REQUESTS.to_string()),
                    ("X-RateLimit-Remaining", "0".to_string()),
                    ("Retry-After", WINDOW_SECS.to_string()),
                ],
                Json(body),
            )
                .into_response()
        }
        Ok(count) => {
            let remaining = count.floor() as u64;
            let mut response = next.run(req).await; // pass the HTTP request downstream. If there are no other middlewares in the way, it goes straight to the target handler
            response.headers_mut().insert(
                "X-RateLimit-Limit",
                MAX_REQUESTS.to_string().parse().unwrap(),
            );
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            response
        }
        Err(err) => {
            // Fail-Open: log the Redis error but allow the request through. This ensures the API stays available even during Redis downtime.
            tracing::error!(
                error = %err,
                ip = %ip,
                "Redis unavailable for auth rate limiter — failing open"
            );
            next.run(req).await
        }
    }
}

/// **API Rate Limiter** — Relaxed tier for general API routes.
///
/// Limits: 100 requests per 10-second window per identity.
///
/// **Key Extraction Strategy (User-ID-Based):**
/// - Authenticated requests → keyed by `rl:api:user:<user_id>`
///   Prevents users sharing a corporate VPN or NAT gateway from being
///   grouped under the same bucket and penalising each other.
/// - Unauthenticated requests → falls back to `rl:api:ip:<ip>`
///
/// **Fail-Open**: If Redis is unreachable, the request is allowed through.
pub async fn api_rate_limiter(
    State(state): State<SharedState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    const WINDOW_SECS: u64 = 60;
    const MAX_REQUESTS: u64 = 100;

    // Attempt to identify the requester by their JWT user_id first.
    // This is a best-effort decode: no error is returned if the token is absent or invalid.
    let key = match try_extract_user_id(&req, &state.jwt_secret) {
        Some(user_id) => {
            tracing::debug!(user_id = user_id, "API rate limit keyed by user ID");
            format!("rl:api:user:{}", user_id)
        }
        None => {
            // No valid token — fall back to IP-based limiting for public or unauthenticated routes.
            let ip = extract_ip(&req, &socket_addr);
            tracing::debug!(ip = %ip, "API rate limit keyed by IP (no valid token)");
            format!("rl:api:ip:{}", ip)
        }
    };

    let mut conn = match state.redis_pool.get().await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(
                error = %err,
                key = %key,
                "Failed to acquire Redis connection from pool for API rate limiter — failing open"
            );
            return next.run(req).await;
        }
    };

    match check_rate_limit(&mut conn, &key, WINDOW_SECS, MAX_REQUESTS).await {
        Ok(count) if count < 0.0 => {
            tracing::warn!(
                key = %key,
                max = MAX_REQUESTS,
                "API rate limit exceeded"
            );
            let body = ErrorResponse {
                error: ErrorBody {
                    code: error_codes::RATE_LIMIT_EXCEEDED.to_string(),
                    message: "Too many requests. Please slow down.".to_string(),
                },
            };
            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("X-RateLimit-Limit", MAX_REQUESTS.to_string()),
                    ("X-RateLimit-Remaining", "0".to_string()),
                    ("Retry-After", WINDOW_SECS.to_string()),
                ],
                Json(body),
            )
                .into_response()
        }
        Ok(count) => {
            let remaining = count.floor() as u64;
            let mut response = next.run(req).await;
            response.headers_mut().insert(
                "X-RateLimit-Limit",
                MAX_REQUESTS.to_string().parse().unwrap(),
            );
            response.headers_mut().insert(
                "X-RateLimit-Remaining",
                remaining.to_string().parse().unwrap(),
            );
            response
        }
        Err(err) => {
            // Fail-Open: log the Redis error but allow the request through.
            tracing::error!(
                error = %err,
                key = %key,
                "Redis unavailable for API rate limiter — failing open"
            );
            next.run(req).await
        }
    }
}
