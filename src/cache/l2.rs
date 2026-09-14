use deadpool_redis::Pool;
use redis::AsyncCommands;

/// Distributed L2 cache backed by a `deadpool_redis::Pool`.
///
/// All values are JSON strings stored with a TTL.
/// All errors are **best-effort** — a Redis failure falls through to the DB
/// rather than surfacing a 500 to the caller (see spec Decision 6).
///
/// **TTL:** 5 minutes — safety net against orphaned stale keys. Explicit `DEL`
/// on every write is the primary correctness mechanism (see spec Decision 5).
#[derive(Clone)] // Cloning deadpool_redis::Pool increments an internal Arc reference count — zero copy.
pub struct L2Cache {
    pool: Pool,
    enabled: bool,
}

impl L2Cache {
    pub fn new(pool: Pool, enabled: bool) -> Self {
        Self { pool, enabled }
    }

    /// Returns the cached JSON string for `key`, or `None` on a miss or error.
    pub async fn get(&self, key: &str) -> Option<String> {
        if !self.enabled {
            return None;
        }
        // Borrow a pooled connection; any error is silently treated as a miss.
        let mut conn = self.pool.get().await.ok()?;
        // GET the key and parse the result into Option<String>.
        conn.get::<_, Option<String>>(key).await.ok().flatten()
    }

    /// Stores `value` under `key` with a TTL. Errors are silently discarded.
    pub async fn set(&self, key: &str, value: &str, ttl_secs: u64) {
        if !self.enabled {
            return;
        }
        if let Ok(mut conn) = self.pool.get().await {
            // `_: Result<(), _>` explicitly discards the return value.
            let _: Result<(), _> = conn.set_ex(key, value, ttl_secs).await;
        }
    }

    /// Deletes a single `key`. Errors are silently discarded.
    pub async fn delete(&self, key: &str) {
        if !self.enabled {
            return;
        }
        if let Ok(mut conn) = self.pool.get().await {
            let _: Result<(), _> = conn.del(key).await;
        }
    }

    /// Deletes all keys matching `pattern` (e.g. `"cache:turf:*"`) using SCAN.
    ///
    /// Uses SCAN instead of KEYS to avoid blocking the Redis event loop on
    /// large keyspaces.
    ///
    /// Two separate pool connections are used: one for SCAN (which borrows the
    /// connection for the iterator's lifetime via `AsyncIter<'_>`), and one for
    /// the subsequent DEL calls. Both are returned to the pool when done.
    pub async fn delete_pattern(&self, pattern: &str) {
        if !self.enabled {
            return;
        }
        // Phase 1: collect all matching keys via SCAN on a dedicated connection.
        // `AsyncIter<'_>` borrows `scan_conn` for its entire lifetime; we collect
        // into a Vec and drop both the stream and `scan_conn` before Phase 2.
        let keys: Vec<String> = {
            let Ok(mut scan_conn) = self.pool.get().await else {
                return;
            };
            let iter: Result<redis::AsyncIter<'_, String>, _> =
                scan_conn.scan_match(pattern).await;
            match iter {
                Ok(mut stream) => {
                    let mut collected = Vec::new();
                    while let Some(key) = stream.next_item().await {
                        collected.push(key);
                    }
                    collected
                }
                Err(_) => return,
            }
            // `stream` and `scan_conn` are both dropped here.
        };

        if keys.is_empty() {
            return;
        }

        // Phase 2: delete each key on a fresh connection.
        if let Ok(mut del_conn) = self.pool.get().await {
            for key in keys {
                let _: Result<(), _> = del_conn.del(&key).await;
            }
        }
    }
}
