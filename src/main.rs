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

    // Configure CORS so your friend's frontend can talk to your backend
    let cors = CorsLayer::new()
        .allow_origin([
            "turf-bd-frontend-gi8p9zuwy-emp3r0rrs-projects.vercel.app" // Vercel deployment
                .parse::<HeaderValue>()
                .unwrap(),
            "http://localhost:5500".parse::<HeaderValue>().unwrap(),
            "http://localhost:5501".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:5500".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:5501".parse::<HeaderValue>().unwrap(),
            "http://localhost:6751".parse::<HeaderValue>().unwrap(),
            "http://127.0.0.1:6751".parse::<HeaderValue>().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ACCEPT,
        ])
        .allow_credentials(true); // This tells the browser: "Yes, I am expecting requests from Vercel to include cookies/credentials."

    // 1. Create the axum router
    let final_router = Router::new()
        .route("/register", post(api::handlers::register::register))
        .route("/login", post(api::handlers::login::login_handler))
        .route(
            "/refresh",
            post(api::handlers::refresh_token::refresh_handler),
        )
        .route("/logout", post(api::handlers::logout::logout_handler))
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
