use axum::{extract::MatchedPath, http::Request, middleware::Next, response::Response};
use metrics::{counter, gauge, histogram};
use sqlx::PgPool;
use std::time::{Duration, Instant};
use tokio::time::interval;

// Routes that are excluded from HTTP telemetry to prevent noise from
// health probes, uptime monitors, and the scraper itself.
const EXCLUDED_PATHS: &[&str] = &["/health", "/ping", "/metrics"];

/// Axum middleware that records HTTP request counts and latency histograms.
///
/// Uses `MatchedPath` (the parameterized route template, e.g. `/users/{id}`)
/// instead of the raw URL to prevent cardinality explosion in Prometheus.
/// Requests to excluded system routes are passed through without measurement.
pub async fn track_http_metrics(request: Request<axum::body::Body>, next: Next) -> Response {
    // Read the parameterized path template before consuming the request.
    // Falls back to "unknown" only if no route was matched (e.g. 404).
    let path = request
        .extensions()
        .get::<MatchedPath>()
        .map(|mp| mp.as_str().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());

    // Skip internal/system routes to keep dashboards clean.
    if EXCLUDED_PATHS.contains(&path.as_str()) {
        return next.run(request).await;
    }

    let method = request.method().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    // Increment the request count for this method + path + status combination.
    counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path"   => path.clone(),
        "status" => status
    )
    .increment(1);

    // Record how long this request took (in seconds).
    histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path"   => path
    )
    .record(latency);

    response
}

/// Background worker that polls the SQLx connection pool every 5 seconds
/// and publishes active/idle gauge metrics to the global Prometheus recorder.
///
/// Spawn once after the pool is created:
/// ```rust
/// tokio::spawn(track_database_metrics(db.clone()));
/// ```
pub async fn track_database_metrics(pool: PgPool) {
    let mut ticker = interval(Duration::from_secs(5));

    loop {
        ticker.tick().await;

        let total = pool.size() as f64;
        let idle = pool.num_idle() as f64;
        let active = total - idle;

        gauge!("db_pool_connections_active").set(active);
        gauge!("db_pool_connections_idle").set(idle);
    }
}
