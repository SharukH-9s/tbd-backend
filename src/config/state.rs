use sqlx::PgPool;
use std::sync::Arc;

pub struct AppState {
    pub db: PgPool,// Thread-safe pool of PostgreSQL connections. No Mutex needed — PgPool handles concurrency internally.
    pub jwt_secret: Arc<Vec<u8>>
}

pub type SharedState = Arc<AppState>; // shorthand type alias. variables of type SharedState are an Arc-wrapped AppState, safe to share across threads.
