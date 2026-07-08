use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{DecodingKey, Validation, decode};
use std::net::SocketAddr;

use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;

// Lua script for an atomic Fixed Window rate limiter.
// Returns the current count after increment.
// KEYS[1]  = the rate limit key (e.g. "rl:auth:1.2.3.4")
// ARGV[1]  = the window duration in seconds
// ARGV[2]  = the max allowed requests per window
//
// Atomicity guarantee: INCR + EXPIRE happen as a single unit inside Redis.
// This prevents the race condition where INCR succeeds but EXPIRE never runs,
// which would cause that key (and its memory) to live forever in Redis.
const RATE_LIMIT_LUA: &str = r#"
local current = redis.call('INCR', KEYS[1])
if current == 1 then
    redis.call('EXPIRE', KEYS[1], ARGV[1])
end
return current
"#;

/// Extracts the true client IP, respecting Render's load balancer headers.
/// Falls back to the raw socket address if no proxy header is present.
fn extract_ip(req: &Request, socket_addr: &SocketAddr) -> String {
    // X-Forwarded-For is the de-facto standard set by Render's load balancer.
    // We take the *first* (leftmost) entry as it represents the original client.
    if let Some(forwarded_for) = req.headers().get("X-Forwarded-For") {
        if let Ok(val) = forwarded_for.to_str() {
            if let Some(first_ip) = val.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }

    // X-Real-IP is a simpler single-value alternative used by some proxies.
    if let Some(real_ip) = req.headers().get("X-Real-IP") {
        if let Ok(val) = real_ip.to_str() {
            return val.trim().to_string();
        }
    }

    // Fallback: use the raw TCP connection's remote address.
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
/// Returns `Ok(current_count)` on success, or `Err` if Redis is unavailable.
async fn check_rate_limit(
    conn: &mut redis::aio::MultiplexedConnection,
    key: &str,
    window_secs: u64,
    max_requests: u64,
) -> Result<u64, redis::RedisError> {
    let count: u64 = redis::Script::new(RATE_LIMIT_LUA)
        .key(key)
        .arg(window_secs)
        .arg(max_requests)
        .invoke_async(conn)
        .await?;
    Ok(count)
}

/// **Auth Rate Limiter** — Strict tier for authentication routes.
///
/// Limits: 3 requests per 10-second window per IP.
/// Auth routes (/login, /register, etc.) are always rate-limited by IP because
/// the user is unauthenticated by definition at this point in the request.
///
/// **Fail-Open**: If Redis is unreachable, the request is allowed through and
/// the error is logged. This keeps the API online even if Redis has a blip.
pub async fn auth_rate_limiter(
    State(state): State<SharedState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    const WINDOW_SECS: u64 = 10;
    const MAX_REQUESTS: u64 = 3;

    let ip = extract_ip(&req, &socket_addr);
    let key = format!("rl:auth:ip:{}", ip);

    let mut conn = state.redis_conn.clone();

    match check_rate_limit(&mut conn, &key, WINDOW_SECS, MAX_REQUESTS).await {
        Ok(count) if count > MAX_REQUESTS => {
            tracing::warn!(
                ip = %ip,
                count = count,
                max = MAX_REQUESTS,
                "Auth rate limit exceeded"
            );
            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("X-RateLimit-Limit", MAX_REQUESTS.to_string()),
                    ("X-RateLimit-Remaining", "0".to_string()),
                    ("Retry-After", WINDOW_SECS.to_string()),
                ],
                "Too many requests. Please wait before trying again.",
            )
                .into_response()
        }
        Ok(count) => {
            let remaining = MAX_REQUESTS.saturating_sub(count);
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
            // This ensures the API stays available even during Redis downtime.
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
    const WINDOW_SECS: u64 = 10;
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

    let mut conn = state.redis_conn.clone();

    match check_rate_limit(&mut conn, &key, WINDOW_SECS, MAX_REQUESTS).await {
        Ok(count) if count > MAX_REQUESTS => {
            tracing::warn!(
                key = %key,
                count = count,
                max = MAX_REQUESTS,
                "API rate limit exceeded"
            );
            (
                StatusCode::TOO_MANY_REQUESTS,
                [
                    ("X-RateLimit-Limit", MAX_REQUESTS.to_string()),
                    ("X-RateLimit-Remaining", "0".to_string()),
                    ("Retry-After", WINDOW_SECS.to_string()),
                ],
                "Too many requests. Please slow down.",
            )
                .into_response()
        }
        Ok(count) => {
            let remaining = MAX_REQUESTS.saturating_sub(count);
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
