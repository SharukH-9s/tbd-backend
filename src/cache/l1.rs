use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

/// In-process L1 cache backed by moka.
///
/// Stores all cache values as JSON-serialized `Arc<str>` to avoid heap
/// allocations on every concurrent cache read (readers just bump a refcount).
///
/// **TTL:** 30 seconds — bounds the maximum stale window on a single instance.
/// **Capacity:** 500 entries — sufficient for the set of Turfs, Games, and Settings
/// in a typical TurfBD deployment without consuming significant heap memory.
///
/// See spec Decision 2 (unified key design) and Decision 5 (TTL rationale).
#[derive(Clone)] // Cloning moka::future::Cache increments an internal Arc reference count — zero copy.
pub struct L1Cache {
    inner: Cache<String, Arc<str>>,
    enabled: bool,
}

impl L1Cache {
    pub fn new(enabled: bool) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(30))
                // Required for `invalidate_entries_if` (bulk prefix invalidation).
                .support_invalidation_closures()
                .build(),
            enabled,
        }
    }

    /// Fetches the value for `key`, or executes `init` to compute and insert it.
    ///
    /// Moka's stampede coalescing: if 100 concurrent requests arrive while L1 is
    /// cold, only ONE executes `init`. The other 99 wait and receive the same result,
    /// protecting L2 and DB from fan-out.
    ///
    /// If caching is disabled, executes `init` directly without touching Moka.
    pub async fn try_get_with<F, E>(&self, key: &str, init: F) -> Result<Arc<str>, Arc<E>>
    where
        F: std::future::Future<Output = Result<Arc<str>, E>>,
        E: Send + Sync + 'static,
    {
        if !self.enabled {
            return init.await.map_err(Arc::new);
        }
        self.inner.try_get_with(key.to_string(), init).await
    }

    /// Returns a value only when it is already present in the in-process cache.
    pub async fn get(&self, key: &str) -> Option<Arc<str>> {
        if !self.enabled {
            return None;
        }
        self.inner.get(key).await
    }

    /// Inserts `value` under `key`. Overwrites any existing entry.
    #[allow(dead_code)]
    pub async fn set(&self, key: &str, value: impl Into<Arc<str>>) {
        if !self.enabled {
            return;
        }
        self.inner.insert(key.to_string(), value.into()).await;
    }

    /// Removes `key` from the cache immediately.
    pub async fn invalidate(&self, key: &str) {
        if !self.enabled {
            return;
        }
        self.inner.invalidate(key).await;
    }

    /// Bulk-invalidates all entries whose key starts with `prefix`.
    ///
    /// Uses Moka's internal predicate worker — non-blocking for the caller.
    /// Errors are logged but never propagated (cache is a best-effort layer).
    /// Reserved for future use when horizontal scaling requires pattern-based invalidation.
    #[allow(dead_code)]
    pub fn invalidate_prefix(&self, prefix: &str) {
        if !self.enabled {
            return;
        }
        let prefix = prefix.to_string();
        // `move` transfers ownership of `prefix` into the closure.
        // Moka passes (key, value) for every cached entry; `_` discards the value.
        if let Err(e) = self.inner.invalidate_entries_if(move |k, _| k.starts_with(&prefix)) {
            tracing::error!("Failed to send invalidation closure to Moka: {}", e);
        }
    }
}

impl Default for L1Cache {
    fn default() -> Self {
        Self::new(true)
    }
}
