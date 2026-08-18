use crate::config::state::SharedState;
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::slot_repo::SlotRepository;
use tokio::time::{Duration, interval};

#[tracing::instrument(name = "worker.cleanup", skip(state))]
async fn execute_cleanup(state: &SharedState) {
    // Try to acquire a 10-second lock in Redis
    // "NX" means: Only set the key if it does not already exist
    // "EX 10" means: Automatically delete this key after 10 seconds (fail-safe expiration)
    let acquire_lock: Result<Option<String>, redis::RedisError> = match state.redis_pool.get().await {
        Ok(mut redis_conn) => {
            redis::cmd("SET") // sends a SET command to Redis
                .arg("lock:cleanup_worker") // with the key "lock:cleanup_worker"
                .arg("locked") // and value "locked"
                .arg("NX") // NX - Only set if the key does not already exist
                .arg("EX") // EX - Set an expiration time
                .arg(10) // 10 seconds
                .query_async(&mut redis_conn)
                .await
        }
        Err(e) => {
            tracing::error!(
                error = %e,
                "[cleanup] Failed to acquire Redis connection from pool for lock check"
            );
            return;
        }
    };

    match acquire_lock {
        Ok(Some(_)) => {
            // Successfully got the lock! This instance is chosen to do the work.
            tracing::debug!("[cleanup] Lock acquired. Executing database cleanups...");

            // Step 1: Reset expired Pending slots → Available
            match SlotRepository::reset_expired_pending_slots(&state.db).await {
                Ok(0) => {} // Nothing to reset — no noise in logs
                Ok(n) => {
                    tracing::info!("[cleanup] Reset {} expired pending slot(s) to Available", n)
                }
                Err(e) => tracing::error!(error = ?e, "[cleanup] Failed to reset pending slots"),
            }

            // Step 2: Delete the orphaned Unpaid booking records
            match BookingRepository::delete_orphaned_unpaid_bookings(&state.db).await {
                Ok(0) => {} // Nothing to delete
                Ok(n) => {
                    tracing::info!("[cleanup] Deleted {} orphaned Unpaid booking record(s)", n)
                }
                Err(e) => {
                    tracing::error!(error = ?e, "[cleanup] Failed to delete orphaned bookings")
                }
            }
        }
        Ok(None) => {
            // Lock already exists! Another instance beat us to it.
            // Fail open silently and try again on the next 60-second tick.
            tracing::debug!(
                "[cleanup] Another instance is already handling cleanup. Skipping."
            );
        }
        Err(e) => {
            // Redis is down or blipping.
            // Log the error and skip this run so we don't spam the DB during Redis downtime.
            tracing::error!(
                error = ?e,
                "[cleanup] Redis connection error during lock check"
            );
        }
    }
}

pub async fn run_cleanup_worker(state: SharedState) {
    let interval_secs: u64 = std::env::var("CLEANUP_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let mut ticker = interval(Duration::from_secs(interval_secs));

    loop {
        ticker.tick().await;
        execute_cleanup(&state).await;
    }
}
