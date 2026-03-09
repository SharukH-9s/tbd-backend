use crate::config::state::SharedState;
use crate::domain::models::slots::{Slot, SlotStatus};
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::slot_repo::SlotRepository;
use chrono::{Duration, Utc};

pub struct BookingService;

impl BookingService {
    pub async fn initiate_booking(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Slot, String> {
        // 1. Check if slot exists and is Available
        let slot = SlotRepository::find_by_id(state, slot_id)
            .await
            .map_err(|_| "Database error".to_string())?
            .ok_or("Slot not found".to_string())?;

        if slot.status != SlotStatus::Available {
            return Err("Slot is not available".to_string());
        }

        // 2. Lock the slot for 15 minutes
        let pending_until = Utc::now() + Duration::minutes(15);
        let updated_slot =
            SlotRepository::update_status(state, slot_id, SlotStatus::Pending, Some(pending_until))
                .await
                .map_err(|_| "Failed to lock slot".to_string())?;

        // 3. Create a pending booking record
        BookingRepository::insert(state, slot_id, user_id)
            .await
            .map_err(|_| "Failed to create booking record".to_string())?;

        Ok(updated_slot)
    }

    pub async fn confirm_payment(state: &SharedState, slot_id: i32) -> Result<Slot, String> {
        // In a real system, you'd find the booking linked to this slot and user
        let slot = SlotRepository::update_status(state, slot_id, SlotStatus::Booked, None)
            .await
            .map_err(|_| "Failed to finalize booking".to_string())?;

        Ok(slot)
    }
}
