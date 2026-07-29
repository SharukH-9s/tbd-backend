use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::bookings::Booking;
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::slot_repo::SlotRepository;
use crate::repositories::user_repo::UserRepository;

pub struct BookingService;

impl BookingService {
    /// Phase 1 — User initiates a booking on an Available slot.
    ///
    /// Steps:
    ///   1. Check: user must not already hold 2 or more Pending slots.
    ///   2. Fetch user's contact info (name and phone) from the users table.
    ///   3. Atomically lock the slot as 'Pending' for 15 minutes.
    ///      (Uses WHERE status = 'Available' — race condition safe)
    ///   4. If the slot was already taken (None returned), reject with SLOT_UNAVAILABLE.
    ///   5. Insert a booking record with payment_status = 'Unpaid' and contact info.
    ///   6. Commit.
    #[tracing::instrument(skip(state), fields(slot_id, caller_id), name = "booking.initiate")]
    pub async fn initiate_booking(
        state: &SharedState,
        slot_id: i32,
        caller_id: i32,
    ) -> Result<Booking, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // ── 1. Pending slot limit check ───────────────────────────────────────────────
        let at_limit = BookingRepository::user_has_pending_booking_tx(&mut *tx, caller_id).await?;
        if at_limit {
            return Err(AppError::conflict(error_codes::TOO_MANY_PENDING_BOOKINGS, "You already have 2 pending slots. Complete or abandon them before booking another."));
        }

        // ── 2. Fetch user's contact info ───────────────────────────────────────────────
        let user = match UserRepository::find_by_id_tx(&mut *tx, caller_id).await? {
            Some(u) => u,
            None => return Err(AppError::not_found(error_codes::USER_NOT_FOUND, "User profile not found")),
        };

        // ── 3. Atomically lock the slot as Pending ──────────────────────────────────
        // If the slot is already Pending or Booked, the WHERE clause won't match
        // and fetch_optional returns None — we never get a chance to double-book.
        let locked_slot = SlotRepository::lock_as_pending_tx(&mut *tx, slot_id).await?;

        // ── 4. Guard: slot not available ───────────────────────────────────────────
        if locked_slot.is_none() {
            return Err(AppError::conflict(error_codes::SLOT_UNAVAILABLE, "This slot is no longer available. It may have just been booked by someone else."));
        }

        // ── 5. Insert the booking record ─────────────────────────────────────────
        let booking = BookingRepository::insert_tx(&mut *tx, slot_id, caller_id, &user.username, &user.phone).await?;

        // ── 6. Commit ───────────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize booking"))?;

        Ok(booking)
    }

    /// Manager settles payment after the match.
    ///
    /// Steps:
    ///   1. Fetch booking → 404 if not found
    ///   2. Fetch slot → game → turf → verify caller is manager or Admin
    ///   3. Guard: booking must not be 'Fully_Paid' already → 400 ALREADY_SETTLED if not
    ///   4. Mark booking as 'Fully_Paid'
    ///   5. Commit
    #[tracing::instrument(skip(state), fields(booking_id, caller_id), name = "booking.update_payment")]
    pub async fn update_payment(
        state: &SharedState,
        booking_id: i32,
        caller_id: i32,
        caller_role: crate::domain::models::users::Role,
    ) -> Result<Booking, AppError> {
        use crate::repositories::game_repo::GameRepository;
        use crate::repositories::turf_repo::TurfRepository;

        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // ── 1. Fetch the booking ─────────────────────────────────────────────
        let booking = match BookingRepository::find_by_id_tx(&mut *tx, booking_id).await? {
            Some(b) => b,
            None => return Err(AppError::not_found(error_codes::BOOKING_NOT_FOUND, "Booking not found")),
        };

        // ── 2. Fetch slot → game → turf for ownership check ──────────────────
        let slot = match SlotRepository::find_by_id_tx(&mut *tx, booking.slot_id).await? {
            Some(s) => s,
            None => return Err(AppError::not_found(error_codes::SLOT_NOT_FOUND, "Parent slot not found")),
        };

        let game = match GameRepository::find_by_id_tx(&mut *tx, slot.game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Parent game not found")),
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "Only the turf manager or an Admin can settle payments"));
        }

        // ── 3. Guard: cannot mark an already settled booking ─────────────────
        if booking.payment_status == "Fully_Paid" {
            return Err(AppError::bad_request(error_codes::ALREADY_SETTLED, "This booking is already Fully Paid"));
        }

        // ── 4. Mark as Fully_Paid ────────────────────────────────────────────
        let updated =
            BookingRepository::mark_fully_paid_by_manager_tx(&mut *tx, booking_id).await?;

        // ── 5. Commit ────────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize payment update"))?;

        Ok(updated)
    }

    /// Manager cancels a booking (regardless of how it was created — offline or online).
    ///
    /// Steps:
    ///   1. Fetch booking → 404 if not found
    ///   2. Fetch slot → game → turf → verify caller is manager or Admin
    ///   3. Guard: cannot cancel a Fully_Paid booking (money already settled)
    ///   4. Delete the booking record
    ///   5. Reset slot: status = 'Available', pending_until = NULL
    ///   6. Commit
    #[tracing::instrument(skip(state), fields(booking_id, caller_id), name = "booking.cancel")]
    pub async fn cancel_booking(
        state: &SharedState,
        booking_id: i32,
        caller_id: i32,
        caller_role: crate::domain::models::users::Role,
    ) -> Result<(), AppError> {
        use crate::repositories::game_repo::GameRepository;
        use crate::repositories::turf_repo::TurfRepository;

        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // ── 1. Fetch the booking ─────────────────────────────────────────────
        let booking = match BookingRepository::find_by_id_tx(&mut *tx, booking_id).await? {
            Some(b) => b,
            None => return Err(AppError::not_found(error_codes::BOOKING_NOT_FOUND, "Booking not found")),
        };

        // ── 2. Fetch slot → game → turf for ownership check ──────────────────
        let slot = match SlotRepository::find_by_id_tx(&mut *tx, booking.slot_id).await? {
            Some(s) => s,
            None => return Err(AppError::not_found(error_codes::SLOT_NOT_FOUND, "Parent slot not found")),
        };

        let game = match GameRepository::find_by_id_tx(&mut *tx, slot.game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Parent game not found")),
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "Only the turf manager or an Admin can cancel bookings"));
        }

        // ── 3. Guard: do not cancel an already settled booking ───────────────
        if booking.payment_status == "Fully_Paid" {
            return Err(AppError::conflict(error_codes::ALREADY_SETTLED, "A Fully_Paid booking cannot be cancelled. Contact admin if needed."));
        }

        // ── 4. Delete the booking record ───────────────────────────────────────
        BookingRepository::delete_by_id_tx(&mut *tx, booking_id).await?;

        // ── 5. Reset the slot to Available ─────────────────────────────────
        SlotRepository::make_available_tx(&mut *tx, booking.slot_id).await?;

        // ── 6. Commit ───────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize booking cancellation"))?;

        Ok(())
    }

    /// Mock Payment Webhook — Advance 30%
    /// Sets booking to 'Partially_Paid' and locks slot as 'Booked'.
    /// Internal helper for mock gateway payments (advance or full).
    async fn process_gateway_payment(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
        payment_status: &str,
    ) -> Result<Booking, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Mark booking as paid
        let updated_booking =
            BookingRepository::update_payment_status_tx(&mut *tx, payment_status, None, Some(slot_id), Some(user_id)).await?;

        // 2. Mark slot as Booked
        SlotRepository::mark_booked_tx(&mut *tx, slot_id).await?;

        // 3. Write outbox event — atomic with the payment update above.
        //    Fetch user email and slot start_time in a single JOIN query while still in tx.
        let event_row = sqlx::query!(
            r#"
            SELECT u.email  AS user_email,
                   s.start_time::TEXT AS "slot_start!: String"
            FROM   users u
            JOIN   slots s ON s.id = $1
            WHERE  u.id = $2
            "#,
            slot_id,
            user_id,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| AppError::internal(error_codes::INTERNAL_ERROR, Box::leak(format!("Failed to fetch outbox data: {e}").into_boxed_str())))?;

        if let Some(ev) = event_row {
            // Insert first to get the Postgres-generated UUID, then use it as outbox_id
            // inside the payload so the worker's idempotency key (processed_jobs.outbox_id)
            // ties back to the correct outbox row.
            let outbox_id = sqlx::query_scalar!(
                "INSERT INTO outbox (event_type, payload)
                 VALUES ('BookingCreated', '{}'::jsonb)
                 RETURNING id",
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| AppError::internal(error_codes::INTERNAL_ERROR, Box::leak(format!("Failed to write outbox event: {e}").into_boxed_str())))?;

            let payload = serde_json::json!({
                "outbox_id":    outbox_id,
                "booking_id":   updated_booking.id,
                "user_email":   ev.user_email,
                "contact_name": updated_booking.contact_name,
                "slot_start":   ev.slot_start,
            });

            sqlx::query!(
                "UPDATE outbox SET payload = $1 WHERE id = $2",
                payload,
                outbox_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(error_codes::INTERNAL_ERROR, Box::leak(format!("Failed to update outbox payload: {e}").into_boxed_str())))?;
        }

        // 4. Commit — booking payment, slot status, and outbox row all land together.
        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize payment"))?;

        Ok(updated_booking)
    }

    /// Mock Payment Webhook — Advance 30%
    /// Sets booking to 'Partially_Paid' and locks slot as 'Booked'.
    #[tracing::instrument(skip(state), fields(slot_id, user_id), name = "booking.pay_advance")]
    pub async fn pay_advance(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, AppError> {
        Self::process_gateway_payment(state, slot_id, user_id, "Partially_Paid").await
    }

    /// Mock Payment Webhook — Full 100%
    /// Sets booking to 'Fully_Paid' and locks slot as 'Booked'.
    #[tracing::instrument(skip(state), fields(slot_id, user_id), name = "booking.pay_full")]
    pub async fn pay_full(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, AppError> {
        Self::process_gateway_payment(state, slot_id, user_id, "Fully_Paid").await
    }

    /// Fetch all bookings for a user (regular user sees only their bookings).
    #[tracing::instrument(skip(state), fields(user_id), name = "booking.get_user_bookings")]
    pub async fn get_user_bookings(
        state: &SharedState,
        user_id: i32,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError> {
        BookingRepository::find_by_user_id_enriched(state, user_id).await
    }

    /// Fetch all bookings for a specific game.
    /// For managers: verifies they own the turf that the game belongs to.
    /// For admins: allows access to all games.
    #[tracing::instrument(skip(state), fields(game_id, caller_id, is_manager), name = "booking.get_game_bookings")]
    pub async fn get_game_bookings(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        is_manager: bool,
    ) -> Result<Vec<crate::api::handlers::user_handlers::get_user_bookings::BookingDetail>, AppError> {
        use crate::repositories::game_repo::GameRepository;
        use crate::repositories::turf_repo::TurfRepository;

        // Fetch the game to get its turf_id
        let game = match GameRepository::find_by_id(state, game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
        };

        // If manager, verify they own the turf
        if is_manager {
            let turf = match TurfRepository::find_by_id(state, game.turf_id).await? {
                Some(t) => t,
                None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
            };

            if turf.manager_id != caller_id {
                return Err(AppError::forbidden(error_codes::FORBIDDEN, "You are not the manager of this turf"));
            }
        }

        BookingRepository::find_by_game_id_enriched(state, game_id).await
    }
}
