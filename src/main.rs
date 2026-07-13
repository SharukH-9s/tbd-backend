mod api;
mod config;
mod domain;
mod repositories;
mod services;

use crate::config::state::{AppState, SharedState};
use axum::{
    Json, Router, middleware,
    response::IntoResponse,
    routing::{get, post},
};
use dotenvy::dotenv;
use http::{HeaderValue, Method};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use tower_http::cors::CorsLayer;

async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

/// Deep health check handler.
/// Runs `SELECT 1` against the database to prove the connection pool is alive.
/// Returns `200 OK` if healthy, or `503 Service Unavailable` if the DB is unreachable or saturated.
/// Load balancers (e.g., Render) should be configured to poll `GET /health` and
/// remove this instance from the rotation when a non-2xx response is received.
async fn health_check(
    axum::extract::State(state): axum::extract::State<SharedState>,
) -> axum::response::Response {
    match sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
    {
        Ok(_) => (
            axum::http::StatusCode::OK,
            Json(json!({ "status": "healthy", "database": "reachable" })),
        )
            .into_response(),
        Err(e) => {
            tracing::error!(error = %e, "Health check failed — database unreachable");
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "status": "unhealthy", "database": "unreachable" })),
            )
                .into_response()
        }
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok(); // This line looks for a file named .env in the project's root folder, It loads "Environment Variables" from a text file into the computer's temporary memory while the app is running.
    let secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");

    // Connect to PostgreSQL and create a connection pool.
    // PgPoolOptions lets us configure the pool before connecting.
    // max_connections(5) means at most 5 simultaneous DB connections will be open.
    // If all 5 are busy, the 6th request waits until one is free.
    let db = PgPoolOptions::new()
        .max_connections(20) // Neon Free tier limit ~20 connections
        .acquire_timeout(std::time::Duration::from_secs(3)) // Fail fast if pool is saturated
        .connect(&database_url)
        .await
        .expect("Failed to connect to the database. Check DATABASE_URL in .env");

    println!("Connected to database");

    // Connect to Render Key Value (Redis) and get a multiplexed async connection.
    // A multiplexed connection can be cloned cheaply across handlers — no Mutex needed.
    let redis_client = redis::Client::open(redis_url)
        .expect("Invalid REDIS_URL — check the format in your .env file");
    let redis_conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .expect("Failed to connect to Redis. Ensure REDIS_URL is reachable.");
    println!("Connected to Redis");

    let shared_state: SharedState = Arc::new(AppState {
        db,
        jwt_secret: Arc::new(secret.into_bytes()),
        redis_conn,
    });

    // Ensure the root Administrator exists on startup
    api::utils::seed::ensure_admin_exists(&shared_state).await;

    // Background worker: reset expired Pending slots and delete orphaned Unpaid bookings.
    // Runs every 60 seconds concurrently without blocking the server.
    tokio::spawn(api::utils::cleanup::run_cleanup_worker(
        shared_state.clone(),
    ));

    let cors = CorsLayer::new()
        .allow_origin(["https://unique-kelpie-7632c3.netlify.app"
            .parse::<HeaderValue>()
            .unwrap()])
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

    let api_routes = Router::new()
        .route(
            "/turfs",
            axum::routing::get(api::handlers::turf_handlers::get_all_turfs::get_all_turfs)
                .post(api::handlers::turf_handlers::create_turf::create_turf),
        )
        .route(
            "/turfs/{id}",
            axum::routing::get(api::handlers::turf_handlers::get_turf::get_a_turf_by_id)
                .put(api::handlers::turf_handlers::update_turf::update_turf_handler)
                .delete(api::handlers::turf_handlers::delete_turf::delete_turf_handler),
        )
        .route(
            "/turfs/{id}/games",
            axum::routing::post(api::handlers::game_handlers::create_game::create_game_handler)
                .get(api::handlers::game_handlers::get_games::get_games_handler),
        )
        .route(
            "/games/{id}",
            axum::routing::get(
                api::handlers::game_handlers::get_game_by_id::get_game_by_id_handler,
            )
            .put(api::handlers::game_handlers::update_game::update_game_handler)
            .delete(api::handlers::game_handlers::delete_game::delete_game_handler),
        )
        .route(
            "/games/{id}/settings",
            axum::routing::post(
                api::handlers::game_settings_handlers::create_settings::create_settings_handler,
            )
            .get(api::handlers::game_settings_handlers::get_settings::get_settings_handler),
        )
        .route(
            "/settings/{id}",
            axum::routing::put(
                api::handlers::game_settings_handlers::update_settings::update_settings_handler,
            )
            .delete(
                api::handlers::game_settings_handlers::delete_settings::delete_settings_handler,
            ),
        )
        .route(
            "/games/{id}/slots",
            axum::routing::post(api::handlers::slot_handlers::create_slot::create_slot_handler)
                .get(api::handlers::slot_handlers::get_slots::get_slots_handler),
        )
        .route(
            "/games/{id}/slots/custom",
            axum::routing::post(
                api::handlers::slot_handlers::create_custom_slot::create_custom_slot_handler,
            ),
        )
        .route(
            "/games/{id}/bookings",
            axum::routing::get(
                api::handlers::booking_handlers::get_game_bookings::get_game_bookings_handler,
            ),
        )
        .route(
            "/slots/{id}",
            axum::routing::delete(api::handlers::slot_handlers::delete_slot::delete_slot_handler),
        )
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
            "/bookings/{id}/payment",
            axum::routing::patch(
                api::handlers::booking_handlers::update_payment::update_payment_handler,
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
            api::middleware::rate_limiter::api_rate_limiter,
        ));

    // Merge all route groups
    let final_router = Router::new()
        .route("/health", get(health_check))
        .route("/ping", get(ping))
        .merge(auth_routes)
        .merge(api_routes)
        .layer(cors)
        .with_state(shared_state);

    let port = env::var("PORT").unwrap_or_else(|_| "6751".to_string()); // PORT will be set by the deployer
    let address = format!("0.0.0.0:{}", port); // 0.0.0.0 means "listen on all available network interfaces."
    println!("Server starting on {}", address);

    let listener = tokio::net::TcpListener::bind(&address).await.unwrap();

    axum::serve(
        listener,
        // ConnectInfo<SocketAddr> is extracted by our custom rate limiter middleware
        final_router.into_make_service_with_connect_info::<std::net::SocketAddr>(), // if run locally itll be the the raw TCP connection's remote address, if deployed on a platform that uses a reverse proxy (like Render, Cloudflare, or Nginx), the socketaddress will be set by the load balancer
    )
    .await
    .unwrap();
}
