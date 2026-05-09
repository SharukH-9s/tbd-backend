use crate::config::state::SharedState;
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::slot_repo::SlotRepository;
use tokio::time::{Duration, interval};

/// Background cleanup task that runs every 60 seconds.
///
/// Step 1 — Reset expired slots:
///   Any slot with status = 'Pending' AND pending_until < NOW() is reset back to
///   'Available'. This frees up slots where the user initiated a booking but never
///   paid the advance within the 15-minute window.
///
/// Step 2 — Delete orphaned booking records:
///   After slots are freed, their corresponding 'Unpaid' booking rows are deleted.
///   These rows were created during Phase 1 (initiate_booking) but are now stale
///   since the slot is back to 'Available' and the user's session has expired.
pub async fn run_cleanup_worker(state: SharedState) {
    let mut ticker = interval(Duration::from_secs(60));

    loop {
        ticker.tick().await; // Wait 60 seconds before each run

        // Step 1: Reset expired Pending slots → Available
        match SlotRepository::reset_expired_pending_slots(&state.db).await {
            Ok(0) => {} // Nothing to reset — no noise in logs
            Ok(n) => println!("[cleanup] Reset {} expired pending slot(s) to Available", n),
            Err(e) => eprintln!("[cleanup] Failed to reset pending slots: {:?}", e),
        }

        // Step 2: Delete the orphaned Unpaid booking records
        match BookingRepository::delete_orphaned_unpaid_bookings(&state.db).await {
            Ok(0) => {} // Nothing to delete
            Ok(n) => println!("[cleanup] Deleted {} orphaned Unpaid booking record(s)", n),
            Err(e) => eprintln!("[cleanup] Failed to delete orphaned bookings: {:?}", e),
        }
    }
}
