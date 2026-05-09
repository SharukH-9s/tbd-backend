mod api;
mod config;
mod domain;
mod repositories;
mod services;

use crate::config::state::{AppState, SharedState};
use axum::{Router, routing::post};
use dotenvy::dotenv;
use http::{HeaderValue, Method};
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

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
        .max_connections(5)
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
    tokio::spawn(api::utils::cleanup::run_cleanup_worker(shared_state.clone()));

    // Configure CORS so your friend's frontend can talk to your backend
    let cors = CorsLayer::new()
        .allow_origin([
        ])
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

    // 1. Create the axum router
    let final_router = Router::new()
        .route("/health", axum::routing::get(|| async { "OK" }))
        .route("/register", post(api::handlers::register::register))
        .route("/login", post(api::handlers::login::login_handler))
        .route(
            "/refresh",
            post(api::handlers::refresh_token::refresh_handler),
        )
        .route("/logout", post(api::handlers::logout::logout_handler))
        .route(
            "/turfs",
            axum::routing::get(api::handlers::turf_handlers::get_all_turfs::get_all_turfs)
                .post(api::handlers::turf_handlers::create_turf::create_turf),
        )
        .route(
            "/turfs/:id",
            axum::routing::get(api::handlers::turf_handlers::get_turf::get_a_turf_by_id)
                .put(api::handlers::turf_handlers::update_turf::update_turf_handler)
                .delete(api::handlers::turf_handlers::delete_turf::delete_turf_handler),
        )
        .route(
            "/turfs/:id/games",
            axum::routing::post(api::handlers::game_handlers::create_game::create_game_handler)
                .get(api::handlers::game_handlers::get_games::get_games_handler),
        )
        .route(
            "/games/:id",
            axum::routing::get(
                api::handlers::game_handlers::get_game_by_id::get_game_by_id_handler,
            )
            .put(api::handlers::game_handlers::update_game::update_game_handler)
            .delete(api::handlers::game_handlers::delete_game::delete_game_handler),
        )
        .route(
            "/games/:id/settings",
            axum::routing::post(
                api::handlers::game_settings_handlers::create_settings::create_settings_handler,
            )
            .get(api::handlers::game_settings_handlers::get_settings::get_settings_handler),
        )
        .route(
            "/settings/:id",
            axum::routing::put(
                api::handlers::game_settings_handlers::update_settings::update_settings_handler,
            )
            .delete(
                api::handlers::game_settings_handlers::delete_settings::delete_settings_handler,
            ),
        )
        .route(
            "/games/:id/slots",
            axum::routing::post(api::handlers::slot_handlers::create_slot::create_slot_handler)
                .get(api::handlers::slot_handlers::get_slots::get_slots_handler),
        )
        .route(
            "/games/:id/slots/custom",
            axum::routing::post(api::handlers::slot_handlers::create_custom_slot::create_custom_slot_handler),
        )
        .route(
            "/games/:id/bookings",
            axum::routing::get(api::handlers::booking_handlers::get_game_bookings::get_game_bookings_handler),
        )
        .route(
            "/slots/:id",
            axum::routing::delete(api::handlers::slot_handlers::delete_slot::delete_slot_handler),
        )
        .route(
            "/slots/:id/book",
            axum::routing::post(api::handlers::booking_handlers::initiate_booking::initiate_booking_handler),
        )
        .route(
            "/slots/:id/pay/advance",
            axum::routing::post(api::handlers::booking_handlers::pay_advance::pay_advance_handler),
        )
        .route(
            "/slots/:id/pay/full",
            axum::routing::post(api::handlers::booking_handlers::pay_full::pay_full_handler),
        )
        .route(
            "/bookings/:id",
            axum::routing::delete(api::handlers::booking_handlers::cancel_booking::cancel_booking_handler),
        )
        .route(
            "/bookings/:id/payment",
            axum::routing::patch(api::handlers::booking_handlers::update_payment::update_payment_handler),
        )
        .route(
            "/users/:id",
            axum::routing::get(api::handlers::user_handlers::get_profile::get_profile_handler)
                .put(api::handlers::user_handlers::update_profile::update_profile_handler),
        )
        .route(
            "/users/:id/bookings",
            axum::routing::get(api::handlers::user_handlers::get_user_bookings::get_user_bookings_handler),
        )
        .route(
            "/users/:id/turfs",
            axum::routing::get(api::handlers::user_handlers::get_managed_turfs::get_managed_turfs_handler),
        )
        // This serves everything in the 'frontend' folder to the internet
        // If a user goes to your URL, it will look for 'index.html' inside that folder
        .fallback_service(ServeDir::new("frontend"))
        .layer(cors) // Ensure CORS is applied by adding this layer
        .with_state(shared_state); // State is applied to everything above

    // Railway will provide a PORT environment variable. If not found, use 6751 locally.
    let port = env::var("PORT").unwrap_or_else(|_| "6751".to_string());
    let address = format!("0.0.0.0:{}", port);
    println!("🚀 Server starting on {}", address);

    // Making the bridge between our code and the physical network card
    let listener = tokio::net::TcpListener::bind(&address) //It goes to the Operating System and says: "I want to claim Port 6750
        .await
        .unwrap(); //Because we are in an async function, .await tells the CPU: "I'm waiting for the OS to finish opening this port.
    // You can go do other tasks in the meantime, just wake me up when the port is ready."

    // 3. Launch the web server
    axum::serve(listener, final_router).await.unwrap();
}
