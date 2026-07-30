//! Diagnostic / smoke-test handlers.
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
