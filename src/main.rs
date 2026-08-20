mod api;
mod config;
mod domain;
mod repositories;
mod services;

use crate::config::state::{AppState, SharedState};
use axum::{
    http::StatusCode,
    middleware,
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use http::{HeaderValue, Method};
use metrics_exporter_prometheus::PrometheusBuilder;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() {
    // STEP 1: Initialize the global tracing subscriber — MUST run before anything
    // else. If the DB or Redis connection below panics, the error will be caught
    // and formatted here rather than silently crashing with a blank log screen.

    // Non-blocking writer: app dumps logs into an in-memory buffer instantly;
    // a background thread handles the slow write to stdout.
    // _guard must be kept alive for the lifetime of main() — dropping it early
    // flushes the buffer and stops the background writer prematurely.
    let (non_blocking_writer, _guard) = tracing_appender::non_blocking(std::io::stdout());

    // EnvFilter reads RUST_LOG at runtime (defaults to "info" if not set).
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // Environment-aware format layer:
    //   APP_ENV=production  → JSON output (ingested by Render log drains, Datadog, etc.)
    //   anything else       → compact, human-readable text for local development
    let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

    // Each layer is Option<_>: tracing-subscriber treats None as a zero-cost no-op,
    // so exactly one of these will be active. Both share the same non-blocking writer.
    let json_layer = (app_env == "production").then(|| {
        tracing_subscriber::fmt::layer()
            .json()
            .with_writer(non_blocking_writer.clone())
    });
    let text_layer = (app_env != "production").then(|| {
        tracing_subscriber::fmt::layer()
            .compact()
            .with_writer(non_blocking_writer)
    });

    // dotenv() MUST run before OTel exporter construction so that
    // OTEL_EXPORTER_OTLP_ENDPOINT (if set in .env) is visible to the exporter.
    dotenv().ok();
    let secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");

    // STEP 2.5: Install the global Prometheus recorder.
    // Must happen before any metrics macros (counter!/gauge!/histogram!) are called.
    // The returned handle lets us render the accumulated metrics on demand via the /metrics route.
    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("Failed to install global Prometheus recorder");
    // Read the optional bearer token that guards /metrics.
    // Set METRICS_BEARER_TOKEN in .env for production; leave unset for local development.
    let metrics_token = env::var("METRICS_BEARER_TOKEN").ok();

    // STEP 2: Register the W3C TraceContext Propagator
    // Inbound middleware (like Axum's TraceLayer) hands the raw HTTP headers over to this global TraceContextPropagator, the propagator performs a string extraction process on the traceparent header value.
    // The propagator parses this raw text string and returns a structured object into oour application's memory called a SpanContext.
    opentelemetry::global::set_text_map_propagator(
        opentelemetry_sdk::propagation::TraceContextPropagator::new(),
    );

    // STEP 3: Configure the exporter and provider
    let service_name = env::var("OTEL_SERVICE_NAME")
        .unwrap_or_else(|_| "turf_bd_backend".to_string());

    let resource = Resource::builder() // A Resource represents the entity generating the data. Every trace line emitted by this backend will be permanently stamped with identity tags (service.name and deployment.environment)
        .with_service_name(service_name.clone())
        .with_attribute(KeyValue::new("deployment.environment", app_env.clone()))
        .build();
    let exporter = SpanExporter::builder() // The exporter packages completed spans and pushes them via HTTP/Protobuf to an OTel Collector (defaulting to http://localhost:4318/v1/traces)
        .with_http()
        .build()
        .expect("Failed to build OTLP exporter");

    // Coordinate via SdkTracerProvider
    let tracer_provider = SdkTracerProvider::builder() // The SdkTracerProvider ties the resource identity and network exporter together into a unified runtime engine.
        .with_resource(resource)
        .with_batch_exporter(exporter) // the SDK bundles finished spans into an optimized in-memory memory buffer. A background Tokio task periodically flushes these batches across the network.
        .build();

    // Register the provider as the global standard. We invoke .clone() here so the main.rs loop retains a distinct handle to this provider, which is required for an explicit flush during server shutdown.
    opentelemetry::global::set_tracer_provider(tracer_provider.clone());

    // Create a tracer instance from the provider, naming it after our service.
    let otel_tracer = tracer_provider.tracer(service_name);

    // STEP 4: Attach the tracing-opentelemetry layer to the subscriber registry
    let otel_layer = tracing_opentelemetry::layer().with_tracer(otel_tracer);

    // Single .init() call — no risk of calling it twice.
    tracing_subscriber::registry()
        .with(env_filter)
        .with(otel_layer)
        .with(json_layer)
        .with(text_layer)
        .init();

    tracing::info!(env = %app_env, "Tracing subscriber initialized");

    // Connect to PostgreSQL and create a connection pool.
    // PgPoolOptions lets us configure the pool before connecting.
    // max_connections(5) means at most 5 simultaneous DB connections will be open.
    // If all 5 are busy, the 6th request waits until one is free.
    let max_connections: u32 = env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20); // Neon Free tier limit ~20 connections

    let acquire_timeout_secs: u64 = env::var("DATABASE_ACQUIRE_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(3); // Fail fast if pool is saturated

    let db = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(std::time::Duration::from_secs(acquire_timeout_secs))
        .connect(&database_url)
        .await
        .expect("Failed to connect to the database. Check DATABASE_URL in .env");

    tracing::info!("Connected to PostgreSQL database");

    // Run pending database migrations on startup
    tracing::info!("Running database migrations...");
    sqlx::migrate!()
        .run(&db)
        .await
        .expect("Database migration failed on startup");
    tracing::info!("Database migrations complete");

    // Connect to Redis using a deadpool-redis connection pool.
    // Connection pooling eliminates the bottleneck of a single shared TCP connection under high concurrency.
    let redis_cfg = deadpool_redis::Config::from_url(redis_url);
    let redis_pool = redis_cfg
        .create_pool(Some(deadpool_redis::Runtime::Tokio1))
        .expect("Failed to create Redis connection pool. Ensure REDIS_URL is reachable.");
    tracing::info!("Initialized Redis connection pool");

    let shared_state: SharedState = Arc::new(AppState {
        db,
        jwt_secret: Arc::new(secret.into_bytes()),
        redis_pool,
    });

    // Ensure the root Administrator exists on startup
    api::utils::seed::ensure_admin_exists(&shared_state).await;

    // Background worker: reset expired Pending slots and delete orphaned Unpaid bookings.
    // Runs every 60 seconds concurrently without blocking the server.
    tokio::spawn(api::utils::cleanup::run_cleanup_worker(
        shared_state.clone(),
    ));

    // Background worker: publish SQLx connection pool gauges to Prometheus every 5 seconds.
    tokio::spawn(api::middleware::metrics::track_database_metrics(
        shared_state.db.clone(),
    ));

    // Read CloudAMQP URL — required at startup
    let amqp_url = std::env::var("AMQP_URL")
        .expect("AMQP_URL must be set — add it to .env (local) or Render env vars (production)");

    // Background relay task: polls outbox and publishes to CloudAMQP
    tokio::spawn(crate::services::relay::run_relay(
        shared_state.db.clone(),
        amqp_url,
    ));

    // Background keep-alive task: pings tbd-worker every 10 minutes to prevent
    // Render's free-tier from spinning it down and silencing the consumers.
    // Requires WORKER_URL in env (e.g. https://tbd-worker.onrender.com).
    tokio::spawn(api::utils::tester::keep_alive_worker());

    let cors_origins_raw = env::var("CORS_ORIGIN")
        .unwrap_or_else(|_| "http://localhost:5173,https://unique-kelpie-7632c3.netlify.app".to_string());
    let cors_origins: Vec<HeaderValue> = cors_origins_raw
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<HeaderValue>()
                .expect("CORS_ORIGIN contains an invalid HTTP header value")
        })
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(cors_origins)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ACCEPT,
        ])
        .allow_credentials(true);

    // Rate Limiting (Redis-backed, distributed)
    // Tier 1 (STRICT)  — auth routes : max 3 requests per 10-second window per IP
    // Tier 2 (RELAXED) — api routes  : max 100 requests per 10-second window per IP
    // Implemented as custom Axum middleware using an atomic Lua script (Fixed Window).

    //Router Assembly
    let auth_routes = Router::new()
        .route("/register", post(api::handlers::register::register))
        .route("/login", post(api::handlers::login::login_handler))
        .route(
            "/refresh",
            post(api::handlers::refresh_token::refresh_handler),
        )
        .route("/logout", post(api::handlers::logout::logout_handler))
        .layer(middleware::from_fn_with_state(
            // Applies a middleware to a router. The middleware is a function that takes the state and the request as arguments and returns a response.
            shared_state.clone(), // We clone the shared state to give the middleware its own reference to it.
            api::middleware::rate_limiter::auth_rate_limiter, // Passes the incoming Request directly into the auth_rate_limiter function
        ));

    let public_api_routes = Router::new()
        .route(
            "/turfs",
            axum::routing::get(api::handlers::turf_handlers::get_all_turfs::get_all_turfs),
        )
        .route(
            "/turfs/{id}",
            axum::routing::get(api::handlers::turf_handlers::get_turf::get_a_turf_by_id),
        )
        .route(
            "/turfs/{id}/games",
            axum::routing::get(api::handlers::game_handlers::get_games::get_games_handler),
        )
        .route(
            "/games/{id}",
            axum::routing::get(
                api::handlers::game_handlers::get_game_by_id::get_game_by_id_handler,
            ),
        )
        .route(
            "/games/{id}/settings",
            axum::routing::get(
                api::handlers::game_settings_handlers::get_settings::get_settings_handler,
            ),
        )
        .route(
            "/games/{id}/slots",
            axum::routing::get(api::handlers::slot_handlers::get_slots::get_slots_handler),
        )
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::middleware::rbac::require_auth,
        ));

    let user_api_routes = Router::new()
        .route(
            "/slots/{id}/book",
            axum::routing::post(
                api::handlers::booking_handlers::initiate_booking::initiate_booking_handler,
            ),
        )
        .route(
            "/slots/{id}/pay/advance",
            axum::routing::post(api::handlers::booking_handlers::pay_advance::pay_advance_handler),
        )
        .route(
            "/slots/{id}/pay/full",
            axum::routing::post(api::handlers::booking_handlers::pay_full::pay_full_handler),
        )
        .route(
            "/bookings/{id}",
            axum::routing::delete(
                api::handlers::booking_handlers::cancel_booking::cancel_booking_handler,
            ),
        )
        .route(
            "/users/{id}",
            axum::routing::get(api::handlers::user_handlers::get_profile::get_profile_handler)
                .put(api::handlers::user_handlers::update_profile::update_profile_handler),
        )
        .route(
            "/users/{id}/bookings",
            axum::routing::get(
                api::handlers::user_handlers::get_user_bookings::get_user_bookings_handler,
            ),
        )
        .route(
            "/users/{id}/turfs",
            axum::routing::get(
                api::handlers::user_handlers::get_managed_turfs::get_managed_turfs_handler,
            ),
        )
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::middleware::rbac::require_auth,
        ));

    let manager_api_routes = Router::new()
        .route(
            "/turfs/{id}/games",
            axum::routing::post(api::handlers::game_handlers::create_game::create_game_handler),
        )
        .route(
            "/games/{id}",
            axum::routing::put(api::handlers::game_handlers::update_game::update_game_handler)
                .delete(api::handlers::game_handlers::delete_game::delete_game_handler),
        )
        .route(
            "/games/{id}/settings",
            axum::routing::post(
                api::handlers::game_settings_handlers::create_settings::create_settings_handler,
            ),
        )
        .route(
            "/settings/{id}",
            axum::routing::delete(
                api::handlers::game_settings_handlers::delete_settings::delete_settings_handler,
            ),
        )
        .route(
            "/games/{id}/slots",
            axum::routing::post(api::handlers::slot_handlers::create_slot::create_slot_handler),
        )
        .route(
            "/games/{id}/slots/custom",
            axum::routing::post(
                api::handlers::slot_handlers::create_custom_slot::create_custom_slot_handler,
            ),
        )
        .route(
            "/slots/{id}",
            axum::routing::delete(api::handlers::slot_handlers::delete_slot::delete_slot_handler),
        )
        .route(
            "/games/{id}/bookings",
            axum::routing::get(
                api::handlers::booking_handlers::get_game_bookings::get_game_bookings_handler,
            ),
        )
        .route(
            "/bookings/{id}/payment",
            axum::routing::patch(
                api::handlers::booking_handlers::update_payment::update_payment_handler,
            ),
        )
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::middleware::rbac::require_manager_or_admin,
        ));

    let admin_api_routes = Router::new()
        .route(
            "/turfs",
            axum::routing::post(api::handlers::turf_handlers::create_turf::create_turf),
        )
        .route(
            "/turfs/{id}",
            axum::routing::put(api::handlers::turf_handlers::update_turf::update_turf_handler)
                .delete(api::handlers::turf_handlers::delete_turf::delete_turf_handler),
        )
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::middleware::rbac::require_admin,
        ));

    let api_routes = Router::new()
        .merge(public_api_routes)
        .merge(user_api_routes)
        .merge(manager_api_routes)
        .merge(admin_api_routes)
        .layer(middleware::from_fn_with_state(
            shared_state.clone(),
            api::middleware::rate_limiter::api_rate_limiter,
        ));

    // Capture values for the /metrics closure.
    let captured_handle = metrics_handle.clone();
    let captured_token = metrics_token.clone();

    // Merge all route groups
    let final_router = Router::new()
        .route("/health", get(api::utils::tester::health_check))
        .route("/ping", get(api::utils::tester::ping))
        .route("/test-amqp", get(api::utils::tester::test_amqp))
        // Prometheus scrape endpoint — guarded by an optional bearer token.
        // Set METRICS_BEARER_TOKEN in .env to enable auth; leave unset for open local access.
        .route(
            "/metrics",
            get(move |headers: axum::http::HeaderMap| {
                let handle = captured_handle.clone();
                let token = captured_token.clone();
                async move {
                    if let Some(expected) = token {
                        let provided = headers
                            .get(axum::http::header::AUTHORIZATION)
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("");
                        if provided != format!("Bearer {}", expected) {
                            return Err(StatusCode::UNAUTHORIZED);
                        }
                    }
                    Ok(handle.render())
                }
            }),
        )
        .merge(auth_routes)
        .merge(api_routes)
        .layer(middleware::from_fn(
            api::middleware::metrics::track_http_metrics,
        ))
        .layer(cors)
        .layer(
            // Auto-creates spans for all HTTP requests, but suppresses span
            // creation entirely for internal probes (/health, /ping, /metrics)
            // so they never appear in OTel traces or consume collector quota.
            TraceLayer::new_for_http().make_span_with(
                |request: &axum::http::Request<axum::body::Body>| {
                    let path = request.uri().path();
                    if matches!(path, "/health" | "/ping" | "/metrics") {
                        // tracing::Span::none() is a zero-cost disabled span —
                        // nothing is recorded or exported for these routes.
                        tracing::Span::none()
                    } else {
                        tracing::info_span!(
                            "http_request",
                            method = %request.method(),
                            path   = %path,
                        )
                    }
                },
            ),
        )
        .with_state(shared_state);

    let port = env::var("PORT").unwrap_or_else(|_| "6751".to_string()); // PORT will be set by the deployer
    let address = format!("0.0.0.0:{}", port); // 0.0.0.0 means "listen on all available network interfaces."
    tracing::info!(address = %address, "Server starting");

    let listener = tokio::net::TcpListener::bind(&address).await.unwrap();

    axum::serve(
        listener,
        // ConnectInfo<SocketAddr> is extracted by our custom rate limiter middleware
        final_router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async {
        // Wait for Ctrl-C (or SIGTERM on Linux/Render). Once the signal fires,
        // axum drains in-flight requests, then this future completes and we fall
        // through to the OTel shutdown below, flushing any buffered spans.
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("Shutdown signal received, flushing telemetry");
    })
    .await
    .unwrap();

    // Flush remaining in-memory spans before the process exits.
    // This runs because with_graceful_shutdown() properly returns after the signal.
    if let Err(err) = tracer_provider.shutdown() {
        tracing::error!("Error shutting down tracer provider: {err:?}");
    }
}
