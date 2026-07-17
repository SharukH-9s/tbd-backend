use crate::{db_err};
use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::domain::models::slots::Slot;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};

pub struct SlotRepository;

impl SlotRepository {
    /// Returns the latest end_time among all slots for a game in a business day window.
    /// Returns None if no slots exist yet in that window (service then starts from game_start_time).
    /// Runs inside a transaction to stay consistent with the subsequent insert.
    pub async fn find_latest_end_time_tx(
        executor: &mut sqlx::PgConnection,
        game_id: i32,
        business_day_start: DateTime<Utc>, // e.g. 2026-04-10 01:00 UTC (07:00 Dhaka)
        business_day_end: DateTime<Utc>,   // e.g. 2026-04-10 19:00 UTC (01:00 Dhaka next day)
    ) -> Result<Option<DateTime<Utc>>, AppError> {
        // query_scalar! returns the first column of the first row directly.
        // MAX(end_time) always returns exactly one row, but the value can be NULL when no
        // slots exist in the window — hence the return type is Option<DateTime<Utc>>.
        sqlx::query_scalar!(
            "SELECT MAX(end_time)
             FROM slots
             WHERE game_id = $1
               AND start_time >= $2
               AND start_time < $3",
            game_id,
            business_day_start,
            business_day_end,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to calculate next slot start time"))
    }

    /// Fetches all slots for a game within a business day window, ordered by start_time.
    /// Used for GET endpoint — no transaction needed (read-only).
    pub async fn find_all_by_game_and_window(
        state: &SharedState,
        game_id: i32,
        business_day_start: DateTime<Utc>,
        business_day_end: DateTime<Utc>,
    ) -> Result<Vec<Slot>, AppError> {
        sqlx::query_as!(
            Slot,
            "SELECT id, game_id, settings_id, label, start_time, end_time,
                    final_price, status, pending_until, created_at
             FROM slots
             WHERE game_id = $1
               AND start_time >= $2
               AND start_time < $3
             ORDER BY start_time ASC",
            game_id,
            business_day_start,
            business_day_end,
        )
        .fetch_all(&state.db)
        .await
        .map_err(db_err!("Failed to fetch slots from database")) // Result<Vec<Slot>, AppError>
    }

    /// Finds all slots that overlap with the given time window (used for overlap detection).
    /// A slot overlaps if: start_time < new_end AND end_time > new_start
    /// Runs inside a transaction to ensure consistency.
    pub async fn find_overlapping_slots_tx(
        executor: &mut sqlx::PgConnection,
        game_id: i32,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
    ) -> Result<Vec<Slot>, AppError> {
        sqlx::query_as!(
            Slot,
            "SELECT id, game_id, settings_id, label, start_time, end_time,
                    final_price, status, pending_until, created_at
             FROM slots
             WHERE game_id = $1
               AND start_time < $2
               AND end_time > $3",
            game_id,
            end_time,
            start_time,
        )
        .fetch_all(executor)
        .await
        .map_err(db_err!("Failed to check for slot overlaps")) // Result<Vec<Slot>, AppError>
    }

    /// Inserts a new slot row inside a transaction.
    /// final_price = settings.base_price — frozen at creation time.
    /// status is either "Available" (default) or "Booked" (offline booking).
    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        game_id: i32,
        settings_id: Option<i32>,
        label: Option<&str>,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        final_price: BigDecimal,
        status: &str,
    ) -> Result<Slot, AppError> {
        sqlx::query_as!(
            Slot,
            "INSERT INTO slots (game_id, settings_id, label, start_time, end_time, final_price, status)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, game_id, settings_id, label, start_time, end_time,
                       final_price, status, pending_until, created_at",
            game_id,
            settings_id,
            label,
            start_time,
            end_time,
            final_price,
            status,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to create slot in database")) // Result<Slot, AppError>
    }

    /// Fetches a single slot by ID inside a transaction.
    /// Used by the delete flow to verify the slot's status before deleting.
    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
    ) -> Result<Option<Slot>, AppError> {
        sqlx::query_as!(
            Slot,
            "SELECT id, game_id, settings_id, label, start_time, end_time,
                    final_price, status, pending_until, created_at
             FROM slots WHERE id = $1",
            slot_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(db_err!("Failed to fetch slot from database")) // Result<Option<Slot>, AppError>
    }

    /// Counts active (Booked or Pending) slots for a game that fall OUTSIDE a proposed
    /// business-day window [new_start, new_end).
    ///
    /// Used by GameService::update_game to reject time-window changes that would orphan
    /// existing active bookings. A slot is "outside" if its start_time is before the new
    /// opening time OR at/after the new closing time.
    ///
    /// Runs inside a transaction so the count is consistent with the subsequent UPDATE.
    pub async fn count_active_slots_outside_window_tx(
        executor: &mut sqlx::PgConnection,
        game_id: i32,
        new_window_start: DateTime<Utc>,
        new_window_end: DateTime<Utc>,
    ) -> Result<i64, AppError> {
        // query_scalar! replaces the (i64,) tuple pattern, returning the COUNT directly.
        sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM slots
             WHERE game_id = $1
               AND status IN ('Booked', 'Pending')
               AND (start_time < $2 OR start_time >= $3)",
            game_id,
            new_window_start,
            new_window_end,
        )
        .fetch_one(executor)
        .await
        .map(|n| n.unwrap_or(0))
        .map_err(db_err!("Failed to check for orphaned active slots"))
    }

    /// Counts all active (Booked or Pending) slots for a game.
    /// Used by GameService::delete_game to prevent deleting games with upcoming matches.
    pub async fn count_active_slots_by_game_tx(
        executor: &mut sqlx::PgConnection,
        game_id: i32,
    ) -> Result<i64, AppError> {
        sqlx::query_scalar!(
            "SELECT COUNT(*) FROM slots WHERE game_id = $1 AND status IN ('Booked', 'Pending')",
            game_id,
        )
        .fetch_one(executor)
        .await
        .map(|n| n.unwrap_or(0))
        .map_err(db_err!("Failed to check for active bookings before deletion"))
    }

    /// Deletes a single slot by ID inside a transaction.
    /// The service layer MUST verify status == 'Available' before calling this.
    pub async fn delete_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query!("DELETE FROM slots WHERE id = $1", slot_id)
            .execute(executor)
            .await
            .map_err(db_err!("Failed to delete slot from database")) // Result<PgQueryResult, AppError>
    }

    /// Marks a slot as available inside a transaction
    pub async fn make_available_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
    ) -> Result<Slot, AppError> {
        sqlx::query_as!(
            Slot,
            "UPDATE slots SET status = 'Available', pending_until = NULL
             WHERE id = $1
             RETURNING id, game_id, settings_id, label, start_time, end_time,
                       final_price, status, pending_until, created_at",
            slot_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to update slot status to available"))
    }

    /// Atomically locks a slot as Pending for 15 minutes.
    /// Uses WHERE status = 'Available' so two concurrent requests cannot both succeed.
    /// Returns None if the slot was already Pending or Booked (lost the race).
    pub async fn lock_as_pending_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
    ) -> Result<Option<Slot>, AppError> {
        sqlx::query_as!(
            Slot,
            "UPDATE slots
             SET status = 'Pending',
                 pending_until = NOW() + INTERVAL '15 minutes'
             WHERE id = $1 AND status = 'Available'
             RETURNING id, game_id, settings_id, label, start_time, end_time,
                       final_price, status, pending_until, created_at",
            slot_id,
        )
        .fetch_optional(executor)
        .await
        .map_err(db_err!("Failed to lock slot for booking"))
    }

    /// Marks a slot as fully Booked (clears pending lock).
    /// Called when payment (advance or full) is successful.
    pub async fn mark_booked_tx(
        executor: &mut sqlx::PgConnection,
        slot_id: i32,
    ) -> Result<Slot, AppError> {
        sqlx::query_as!(
            Slot,
            "UPDATE slots SET status = 'Booked', pending_until = NULL
             WHERE id = $1
             RETURNING id, game_id, settings_id, label, start_time, end_time,
                       final_price, status, pending_until, created_at",
            slot_id,
        )
        .fetch_one(executor)
        .await
        .map_err(db_err!("Failed to mark slot as booked"))
    }

    /// Resets expired Pending slots back to Available.
    /// Used by the cleanup worker.
    /// Note: The cleanup worker also deletes orphaned bookings associated with these slots,
    /// so we don't need to handle that here.
    pub async fn reset_expired_pending_slots(db: &sqlx::PgPool) -> Result<u64, AppError> {
        sqlx::query!(
            "UPDATE slots
             SET status = 'Available', pending_until = NULL
             WHERE status = 'Pending' AND pending_until < NOW()",
        )
        .execute(db)
        .await
        .map(|result| result.rows_affected())
        .map_err(db_err!("Failed to reset expired pending slots"))
    }
}
