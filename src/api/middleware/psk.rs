use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};

use crate::config::state::SharedState;

/// PSK (Pre-Shared Key) middleware — Cloudflare "back-door" lock.
///
/// Every request that passes through the Cloudflare proxy has a static
/// `X-Cloudflare-PSK` header injected by a Cloudflare Transform Rule.
/// This middleware verifies that header so that any attacker who tries to
/// hit `https://tbd-backend-m5vx.onrender.com` directly — bypassing the
/// WAF, rate-limiter, and DDoS protection — receives an immediate 403.
///
/// Bypass rules (requests that skip the PSK check):
/// - `/health`, `/ping`, `/metrics` — Render's internal health probes and
///   the Prometheus scrape endpoint must not require the header.
/// - Local development (no `CLOUDFLARE_PSK` env var set) — `state.cloudflare_psk`
///   is `None`, so the check is skipped entirely, preserving the local dev workflow.
pub async fn require_cloudflare(
    State(state): State<SharedState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    // 1. Always allow internal health probes and Prometheus scrape endpoint.
    //    These must succeed even when Render's router hasn't yet set up the
    //    Cloudflare proxy (e.g., during zero-downtime deploys).
    if path == "/health" || path == "/ping" || path == "/metrics" {
        return Ok(next.run(req).await);
    }

    // 2. If no PSK is configured (local development), bypass the check entirely.
    //    This means developers don't need to set a dummy PSK value in their .env.
    let expected_psk = match &state.cloudflare_psk {
        Some(psk) => psk,
        None => return Ok(next.run(req).await),
    };

    // 3. Validate the X-Cloudflare-PSK header value.
    let provided_psk = req
        .headers()
        .get("X-Cloudflare-PSK")
        .and_then(|h| h.to_str().ok());

    if provided_psk != Some(expected_psk.as_str()) {
        tracing::warn!(
            path = %path,
            provided = ?provided_psk,
            "Blocked direct-to-Render access attempt (missing or invalid X-Cloudflare-PSK)"
        );
        return Err(StatusCode::FORBIDDEN);
    }

    // 4. PSK is valid — forward the request to the next layer.
    Ok(next.run(req).await)
}
