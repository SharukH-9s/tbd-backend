use crate::cache::l1::L1Cache;
use crate::cache::l2::L2Cache;
use sqlx::PgPool;
use std::sync::Arc;

pub struct AppState {
    pub db: PgPool,            // Thread-safe pool of PostgreSQL connections. No Mutex needed — PgPool handles concurrency internally.
    pub jwt_secret: Arc<Vec<u8>>,
    pub redis_pool: deadpool_redis::Pool, // Thread-safe pool of Redis connections. Used by rate limiter, auth extractor, logout, and cleanup worker.
    pub l2_cache: L2Cache,     // L2: Redis distributed cache. Backed by a clone of redis_pool (zero cost — Pool is Arc-wrapped internally).
    pub cache: L1Cache,        // L1: in-process moka cache. 30s TTL, 500 entry cap.
    pub redis_enabled: bool,   // When false, all Redis operations (rate limiting, blacklist, L2 cache, locks) are bypassed.
    pub cloudflare_psk: Option<String>, // Pre-shared key injected by Cloudflare Transform Rules. None in local dev (check skipped); Some in production.
}

pub type SharedState = Arc<AppState>; // shorthand type alias. variables of type SharedState are an Arc-wrapped AppState, safe to share across threads.
