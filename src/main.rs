mod api;
mod config;
mod domain;
mod repositories;
mod services;

use crate::config::state::{AppState, SharedState};
use axum::{
    Json, Router,
    routing::{get, post},
};
use dotenvy::dotenv;
use http::{HeaderValue, Method};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use std::time::Duration;
use tower_governor::{
    GovernorLayer, governor::GovernorConfigBuilder, key_extractor::SmartIpKeyExtractor,
};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

async fn ping() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

#[tokio::main]
async fn main() {
    dotenv().ok(); // This line looks for a file named .env in the project's root folder, It loads "Environment Variables" from a text file into the computer's temporary memory while the app is running.
    let secret = env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");

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
    let shared_state: SharedState = Arc::new(AppState {
        db,
        jwt_secret: Arc::new(secret.into_bytes()),
    });

    // Professional Bootstrap: Ensure the root Administrator exists on startup
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
        .allow_credentials(true); // This tells the browser: "Yes, I am expecting requests from Vercel to include cookies/credentials."

    // Rate Limiting
    // Tier 1 (STRICT) — auth routes: 3 requests per 10 seconds, burst of 3.
    // Protects against brute-force and credential stuffing attacks.
    let auth_governor_conf = GovernorConfigBuilder::default() // opening a blank settings template with basic default values that we are about to customize
        .key_extractor(SmartIpKeyExtractor) // we are telling the system: "Look at the incoming network request, find the user's real IP address, and give them their own individual bucket
        .per_millisecond(3333) // This sets how fast tokens drop back into the bucket
        .burst_size(3) // Allow at most 3 back-to-back attempts. a user can make a maximum of 3 requests back-to-back instantly if their bucket is completely full
        .use_headers() // Adds X-RateLimit-* headers for frontend debugging. frontend team inspects the network response headers, they will see hidden values like X-RateLimit-Limit: 3 and X-RateLimit-Remaining: 2
        .finish() // builds the final configuration object
        .unwrap();

    // Tier 2 (RELAXED) — all other API routes: 10 requests/s, burst of 20.
    // Prevents DB pool saturation without impacting normal SPA usage.
    let api_governor_conf = GovernorConfigBuilder::default()
        .key_extractor(SmartIpKeyExtractor)
        .per_millisecond(100) // Replenish 10 tokens per second (1 token per 100ms)
        .burst_size(20) // Allow a burst of 20 for concurrent page-load requests
        .use_headers()
        .finish()
        .unwrap();

    // Background thread: cleans up stale IP entries from the in-memory governor
    // store every 60 seconds to prevent unbounded memory growth over time.
    let auth_limiter = auth_governor_conf.limiter().clone(); // grabbing a direct handle to the internal tracking storage (the list of all IP buckets)
    let api_limiter = api_governor_conf.limiter().clone();
    std::thread::spawn(move || {
        // boots up a completely separate Background Thread that runs parallel to your main web server. keyword move tells Rust to transfer the cloned auth_limiter and api_limiter handles into this new thread
        loop {
            std::thread::sleep(Duration::from_secs(60)); // sleeps for 1 minute between cleanup cycles.
            auth_limiter.retain_recent(); // the governor walks through its entire database of tracked IP addresses and checks how long ago each IP was seen. if the last request from that IP was more than 10 minutes ago (the default retention period), it throws that IP's entire history away
            api_limiter.retain_recent();
        }
    });

    //Router Assembly
    let auth_routes = Router::new()
        .route("/register", post(api::handlers::register::register))
        .route("/login", post(api::handlers::login::login_handler))
        .route(
            "/refresh",
            post(api::handlers::refresh_token::refresh_handler),
        )
        .route("/logout", post(api::handlers::logout::logout_handler))
        .layer(GovernorLayer::new(auth_governor_conf));

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
        .layer(GovernorLayer::new(api_governor_conf));

    // Merge all route groups
    let final_router = Router::new()
        .route("/health", get(|| async { "OK" }))
        .route("/ping", get(ping))
        .merge(auth_routes)
        .merge(api_routes)
        .fallback_service(ServeDir::new("frontend"))
        .layer(cors)
        .with_state(shared_state);

    let port = env::var("PORT").unwrap_or_else(|_| "6751".to_string());
    let address = format!("0.0.0.0:{}", port);
    println!("🚀 Server starting on {}", address);

    let listener = tokio::net::TcpListener::bind(&address).await.unwrap();

    axum::serve(
        listener,
        final_router.into_make_service_with_connect_info::<std::net::SocketAddr>(), // The SmartIpKeyExtractor needs an IP address to figure out which user’s bucket to penalize. so this line provides that ip address to the extractor.
    )
    .await
    .unwrap();
}
