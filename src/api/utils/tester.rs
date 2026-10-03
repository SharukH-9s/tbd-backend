//! Diagnostic / smoke-test handlers and background keep-alive task.
//! Routes: GET /ping, GET /test-amqp, GET /health

use crate::config::state::SharedState;
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;

/// Liveness probe — always returns 200 OK.
pub async fn ping() -> Json<serde_json::Value> {
    tracing::debug!("ping called");
    Json(json!({ "status": "ok" }))
}

/// TCP smoke-test for CloudAMQP connectivity.
/// Attempts to open a TCP connection to the broker host and reports the result.
pub async fn test_amqp() -> impl IntoResponse {
    use std::time::Duration;
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    let target = "beaver.rmq.cloudamqp.com:5671";
    tracing::info!(target, "test_amqp: attempting TCP connection");

    match timeout(Duration::from_secs(5), TcpStream::connect(target)).await {
        Ok(Ok(_)) => {
            tracing::info!(target, "test_amqp: connection succeeded");
            (
                axum::http::StatusCode::OK,
                format!("SUCCESS: Successfully opened TCP connection to {}", target),
            )
        }
        Ok(Err(e)) => {
            tracing::error!(target, error = %e, "test_amqp: connection failed");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("FAILURE: Connection failed: {}", e),
            )
        }
        Err(_) => {
            tracing::warn!(target, "test_amqp: connection timed out after 5s");
            (
                axum::http::StatusCode::GATEWAY_TIMEOUT,
                format!(
                    "FAILURE: Connection to {} timed out after 5 seconds",
                    target
                ),
            )
        }
    }
}

/// Deep health check handler.
/// Runs `SELECT 1` against the database to prove the connection pool is alive.
/// Returns `200 OK` if healthy, or `503 Service Unavailable` if the DB is unreachable or saturated.
/// Load balancers (e.g., Render) should be configured to poll `GET /health` and
/// remove this instance from the rotation when a non-2xx response is received.
pub async fn health_check(State(state): State<SharedState>) -> axum::response::Response {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => {
            tracing::debug!("health_check: database reachable");
            (
                axum::http::StatusCode::OK,
                Json(json!({ "status": "healthy", "database": "reachable" })),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "health_check: database unreachable");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unhealthy", "database": "unreachable" })),
            )
                .into_response()
        }
    }
}

/// Generic background keep-alive task — prevents Render's free-tier from spinning down
/// any named service.
///
/// Pings `ping_url` immediately on startup, then every `interval_secs` seconds.
/// Logs the result of every attempt at INFO level.
async fn keep_alive(service: &'static str, ping_url: String, interval_secs: u64) {
    use std::time::Duration;
    use tokio::time::sleep;

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(service, error = %e, "keep_alive: failed to build HTTP client — exiting");
            return;
        }
    };

    tracing::info!(service, url = %ping_url, interval_secs, "keep_alive: started");

    loop {
        match client.get(&ping_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(service, url = %ping_url, status = %resp.status(), "keep_alive: ping OK");
            }
            Ok(resp) => {
                tracing::warn!(service, url = %ping_url, status = %resp.status(), "keep_alive: ping returned non-2xx");
            }
            Err(e) => {
                tracing::warn!(service, url = %ping_url, error = %e, "keep_alive: ping failed");
            }
        }

        sleep(Duration::from_secs(interval_secs)).await;
    }
}

/// Keep-alive task for `tbd-worker`.
///
/// Reads `WORKER_URL` from the environment (e.g. `https://tbd-worker.onrender.com`).
/// Pings `<WORKER_URL>/healthz` every `KEEP_ALIVE_INTERVAL_SECS` (default 600s).
/// If `WORKER_URL` is absent the task exits immediately with a warning.
pub async fn keep_alive_worker() {
    let base_url = match std::env::var("WORKER_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::warn!("keep_alive_worker: WORKER_URL not set — keep-alive disabled");
            return;
        }
    };

    let ping_url = format!("{}/healthz", base_url.trim_end_matches('/'));
    let interval_secs: u64 = std::env::var("KEEP_ALIVE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10 * 60);

    keep_alive("tbd-worker", ping_url, interval_secs).await;
}

/// Keep-alive task for the Gotenberg PDF service.
///
/// Reads `GOTENBERG_URL` from the environment (e.g. `https://gotenberg.onrender.com`).
/// Pings `<GOTENBERG_URL>/health` every `KEEP_ALIVE_INTERVAL_SECS` (default 600s).
/// If `GOTENBERG_URL` is absent the task exits immediately with a warning.
pub async fn keep_alive_gotenberg() {
    let base_url = match std::env::var("GOTENBERG_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::warn!("keep_alive_gotenberg: GOTENBERG_URL not set — keep-alive disabled");
            return;
        }
    };

    // Gotenberg exposes a built-in /health endpoint that returns 200 when ready.
    let ping_url = format!("{}/health", base_url.trim_end_matches('/'));
    let interval_secs: u64 = std::env::var("KEEP_ALIVE_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10 * 60);

    keep_alive("gotenberg", ping_url, interval_secs).await;
}
