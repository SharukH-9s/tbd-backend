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

/// Background keep-alive task — prevents Render's free-tier from spinning down tbd-worker.
///
/// Render spins down free services after 15 minutes of inactivity. This task pings
/// the worker's `/ping` endpoint every 10 minutes so the worker stays warm and the
/// RabbitMQ consumers remain active.
///
/// Set `WORKER_URL` in your environment (e.g. `https://tbd-worker.onrender.com`).
/// If the variable is absent the task exits immediately with a warning.
pub async fn keep_alive_worker() {
    use std::time::Duration;
    use tokio::time::interval;

    let base_url = match std::env::var("WORKER_URL") {
        Ok(url) => url,
        Err(_) => {
            tracing::warn!("keep_alive_worker: WORKER_URL not set — keep-alive disabled");
            return;
        }
    };

    let ping_url = format!("{}/healthz", base_url.trim_end_matches('/'));

    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "keep_alive_worker: failed to build HTTP client — exiting");
            return;
        }
    };

    tracing::info!(url = %ping_url, "keep_alive_worker: started — pinging every 10 minutes");

    let mut ticker = interval(Duration::from_secs(10 * 60));
    loop {
        ticker.tick().await; // first tick fires immediately
        match client.get(&ping_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                tracing::info!(url = %ping_url, status = %resp.status(), "keep_alive_worker: ping OK");
            }
            Ok(resp) => {
                tracing::warn!(url = %ping_url, status = %resp.status(), "keep_alive_worker: ping returned non-2xx");
            }
            Err(e) => {
                tracing::error!(url = %ping_url, error = %e, "keep_alive_worker: ping failed");
            }
        }
    }
}
