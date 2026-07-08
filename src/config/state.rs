use redis::aio::MultiplexedConnection;
use sqlx::PgPool;
use std::sync::Arc;

pub struct AppState {
    pub db: PgPool, // Thread-safe pool of PostgreSQL connections. No Mutex needed — PgPool handles concurrency internally.
    pub jwt_secret: Arc<Vec<u8>>,
    pub redis_conn: MultiplexedConnection, // Cheaply cloneable handle to a shared async Redis connection. Safe to clone across handlers without Mutex overhead.
}

pub type SharedState = Arc<AppState>; // shorthand type alias. variables of type SharedState are an Arc-wrapped AppState, safe to share across threads.
