use crate::config::state::SharedState;
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::slot_repo::SlotRepository;
use tokio::time::{Duration, interval};

/// Background cleanup task that runs every 60 seconds.
/// Distributed-safe using Redis lock to prevent concurrent database updates.
pub async fn run_cleanup_worker(state: SharedState) {
    let mut ticker = interval(Duration::from_secs(60));

    loop {
        ticker.tick().await; // Wait 60 seconds before each run

        // Try to acquire a 10-second lock in Redis
        // "NX" means: Only set the key if it does not already exist
        // "EX 10" means: Automatically delete this key after 10 seconds (fail-safe expiration)
        let mut redis_conn = state.redis_conn.clone();
        let acquire_lock: Result<Option<String>, redis::RedisError> = redis::cmd("SET")
            .arg("lock:cleanup_worker")
            .arg("locked")
            .arg("NX")
            .arg("EX")
            .arg(10)
            .query_async(&mut redis_conn)
            .await;

        match acquire_lock {
            Ok(Some(_)) => {
                // Successfully got the lock! This instance is chosen to do the work.
                tracing::info!("[cleanup] Lock acquired. Executing database cleanups...");

                // Step 1: Reset expired Pending slots → Available
                match SlotRepository::reset_expired_pending_slots(&state.db).await {
                    Ok(0) => {} // Nothing to reset — no noise in logs
                    Ok(n) => tracing::info!("[cleanup] Reset {} expired pending slot(s) to Available", n),
                    Err(e) => tracing::error!("[cleanup] Failed to reset pending slots: {:?}", e),
                }

                // Step 2: Delete the orphaned Unpaid booking records
                match BookingRepository::delete_orphaned_unpaid_bookings(&state.db).await {
                    Ok(0) => {} // Nothing to delete
                    Ok(n) => tracing::info!("[cleanup] Deleted {} orphaned Unpaid booking record(s)", n),
                    Err(e) => tracing::error!("[cleanup] Failed to delete orphaned bookings: {:?}", e),
                }
            }
            Ok(None) => {
                // Lock already exists! Another instance beat us to it.
                // Fail open silently and try again on the next 60-second tick.
                tracing::debug!("[cleanup] Another instance is already handling cleanup. Skipping.");
            }
            Err(e) => {
                // Redis is down or blipping.
                // Log the error and skip this run so we don't spam the DB during Redis downtime.
                tracing::error!("[cleanup] Redis connection error during lock check: {:?}", e);
            }
        }
    }
}
