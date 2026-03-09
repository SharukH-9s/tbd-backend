use crate::config::state::SharedState;
use crate::repositories::slot_repo::SlotRepository;
use std::time::Duration;
use tokio::time::sleep;

pub async fn start_slot_cleanup_worker(state: SharedState) {
    println!("Slot cleanup worker started.");

    loop {
        // Run every 1 minute
        sleep(Duration::from_secs(60)).await;

        match SlotRepository::release_expired_slots(&state).await {
            Ok(count) => {
                if count > 0 {
                    println!("Released {} expired slots.", count);
                }
            }
            Err(e) => {
                eprintln!("Error in slot cleanup worker: {}", e);
            }
        }
    }
}
