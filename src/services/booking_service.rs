use crate::config::state::SharedState;
use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::bookings::{Booking, PaymentStatus};
use crate::domain::models::slots::SlotStatus;
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::slot_repo::SlotRepository;
use crate::repositories::turf_repo::TurfRepository;
use crate::repositories::user_repo::UserRepository;
use uuid::Uuid;

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
    #[tracing::instrument(skip(state), fields(slot_id, caller_id, ?caller_role), name = "booking.initiate")]
    pub async fn initiate_booking(
        state: &SharedState,
        slot_id: i32,
        caller_id: i32,
        caller_role: &crate::domain::models::users::Role,
    ) -> Result<Booking, AppError> {
        // ── 0. Role guard: Managers cannot initiate bookings ──────────────────
        // Managers are staff — they manage turf operations but book slots via
        // the offline (walk-in) flow, not the online payment gateway.
        if matches!(caller_role, crate::domain::models::users::Role::Manager) {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "Managers cannot initiate online bookings. Use the offline booking process instead.",
            ));
        }

        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // ── 1. Pending slot limit check ───────────────────────────────────────────────
        let at_limit = BookingRepository::user_has_pending_booking_tx(&mut *tx, caller_id).await?;
        if at_limit {
            return Err(AppError::conflict(error_codes::TOO_MANY_PENDING_BOOKINGS, "You already have 2 pending slots. Complete or abandon them before booking another."));
        }

        // ── 2. Fetch user's contact info ───────────────────────────────────────────────
        let user = match UserRepository::find_by_id_tx(&mut *tx, caller_id).await? {
            Some(u) => u,
            None => {
                return Err(AppError::not_found(
                    error_codes::USER_NOT_FOUND,
                    "User profile not found",
                ))
            }
        };

        // ── 3. Atomically lock the slot as Pending ──────────────────────────────────
        // If the slot is already Pending or Booked, the WHERE clause won't match
        // and fetch_optional returns None — we never get a chance to double-book.
        let locked_slot = match SlotRepository::lock_as_pending_tx(&mut *tx, slot_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError::conflict(
                    error_codes::SLOT_UNAVAILABLE,
                    "This slot is no longer available. It may have just been booked by someone else.",
                ));
            }
        };

        // ── 5. Insert the booking record ─────────────────────────────────────────
        let booking = BookingRepository::insert_tx(
            &mut *tx,
            slot_id,
            caller_id,
            &user.username,
            &user.phone,
            &locked_slot.final_price,
        )
        .await?;

        // ── 6. Commit ───────────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize booking",
            )
        })?;

        Ok(booking)
    }

    /// Manager settles payment after the match.
    ///
    /// Steps:
    ///   1. Fetch booking → 404 if not found
    ///   2. Fetch slot → game → turf → verify caller is manager or Admin
    ///   3. Guard: slot must be 'Booked' (confirmed) → 400 BOOKING_NOT_CONFIRMED if not
    ///   4. Guard: booking must not be 'Fully_Paid' already → 400 ALREADY_SETTLED if not
    ///   5. Mark booking as 'Fully_Paid'
    ///   6. Commit
    #[tracing::instrument(
        skip(state),
        fields(booking_id, caller_id),
        name = "booking.update_payment"
    )]
    pub async fn update_payment(
        state: &SharedState,
        booking_id: i32,
        caller_id: i32,
        caller_role: crate::domain::models::users::Role,
    ) -> Result<Booking, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // ── 1. Fetch the booking ─────────────────────────────────────────────
        let booking = match BookingRepository::find_by_id_tx(&mut *tx, booking_id).await? {
            Some(b) => b,
            None => {
                return Err(AppError::not_found(
                    error_codes::BOOKING_NOT_FOUND,
                    "Booking not found",
                ))
            }
        };

        // ── 2. Fetch slot → game → turf for ownership check ──────────────────
        let slot = match SlotRepository::find_by_id_tx(&mut *tx, booking.slot_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError::not_found(
                    error_codes::SLOT_NOT_FOUND,
                    "Parent slot not found",
                ))
            }
        };

        let game = match GameRepository::find_by_id_tx(&mut *tx, slot.game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Parent game not found",
                ))
            }
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Parent turf not found",
                ))
            }
        };

        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "Only the turf manager or an Admin can settle payments",
            ));
        }

        // ── 3. Guards ────────────────────────────────────────────────────────
        // Cannot settle a booking whose slot is still pending or not confirmed
        if slot.status != SlotStatus::Booked {
            return Err(AppError::bad_request(
                error_codes::BOOKING_NOT_CONFIRMED,
                "Cannot settle payment for a booking that is still pending or not confirmed",
            ));
        }

        // Cannot mark an already settled booking
        if booking.payment_status == PaymentStatus::FullyPaid {
            return Err(AppError::bad_request(
                error_codes::ALREADY_SETTLED,
                "This booking is already Fully Paid",
            ));
        }

        // ── 4. Mark as Fully_Paid ────────────────────────────────────────────
        let updated =
            BookingRepository::mark_fully_paid_by_manager_tx(&mut *tx, booking_id).await?;

        // ── 5. Commit ────────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize payment update",
            )
        })?;

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
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // ── 1. Fetch the booking ─────────────────────────────────────────────
        let booking = match BookingRepository::find_by_id_tx(&mut *tx, booking_id).await? {
            Some(b) => b,
            None => {
                return Err(AppError::not_found(
                    error_codes::BOOKING_NOT_FOUND,
                    "Booking not found",
                ))
            }
        };

        // ── 2. Fetch slot → game → turf for ownership check ──────────────────
        let slot = match SlotRepository::find_by_id_tx(&mut *tx, booking.slot_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError::not_found(
                    error_codes::SLOT_NOT_FOUND,
                    "Parent slot not found",
                ))
            }
        };

        let game = match GameRepository::find_by_id_tx(&mut *tx, slot.game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Parent game not found",
                ))
            }
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Parent turf not found",
                ))
            }
        };

        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "Only the turf manager or an Admin can cancel bookings",
            ));
        }

        // ── 3. Guard: do not cancel an already settled booking ───────────────
        if booking.payment_status == PaymentStatus::FullyPaid {
            return Err(AppError::conflict(
                error_codes::ALREADY_SETTLED,
                "A Fully_Paid booking cannot be cancelled. Contact admin if needed.",
            ));
        }

        // ── 4. Delete the booking record ───────────────────────────────────────
        BookingRepository::delete_by_id_tx(&mut *tx, booking_id).await?;

        // ── 5. Reset the slot to Available ─────────────────────────────────
        SlotRepository::make_available_tx(&mut *tx, booking.slot_id).await?;

        // ── 6. Commit ───────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize booking cancellation",
            )
        })?;

        Ok(())
    }

    /// Internal helper for mock gateway payments (advance or full).
    ///
    /// Steps:
    ///   1. Fetch slot → guard: must be 'Pending'
    ///   2. Fetch booking → guard: must be 'Unpaid'
    ///   3. Update booking payment status
    ///   4. Mark slot as 'Booked'
    ///   5. Write outbox event (email confirmation)
    ///   6. Commit
    async fn process_gateway_payment(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
        payment_status: PaymentStatus,
    ) -> Result<Booking, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // ── 1. Fetch the slot and validate it is still Pending ────────────────
        let slot = match SlotRepository::find_by_id_tx(&mut *tx, slot_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError::not_found(
                    error_codes::SLOT_NOT_FOUND,
                    "Slot not found",
                ))
            }
        };

        if slot.status != SlotStatus::Pending {
            return Err(AppError::conflict(
                error_codes::PAYMENT_NOT_ALLOWED,
                "Your reservation has already been confirmed or is no longer active. Payment cannot be processed.",
            ));
        }

        // ── 2. Fetch the booking and validate it is still Unpaid ──────────────
        let booking =
            match BookingRepository::find_by_slot_and_user_tx(&mut *tx, slot_id, user_id).await? {
                Some(b) => b,
                None => {
                    return Err(AppError::not_found(
                        error_codes::BOOKING_NOT_FOUND,
                        "No pending booking found for this slot",
                    ))
                }
            };

        if booking.payment_status != PaymentStatus::Unpaid {
            return Err(AppError::conflict(
                error_codes::PAYMENT_NOT_ALLOWED,
                "This booking has already been paid. Payment cannot be processed again.",
            ));
        }

        // ── 3. Mark booking as paid ───────────────────────────────────────────
        let updated_booking = match payment_status {
            PaymentStatus::PartiallyPaid => {
                BookingRepository::mark_partially_paid_tx(&mut *tx, slot_id, user_id).await?
            }
            PaymentStatus::FullyPaid => {
                BookingRepository::update_payment_status_tx(
                    &mut *tx,
                    PaymentStatus::FullyPaid,
                    None,
                    Some(slot_id),
                    Some(user_id),
                )
                .await?
            }
            PaymentStatus::Unpaid => {
                unreachable!("process_gateway_payment is never called with Unpaid")
            }
        };

        // 4. Mark slot as Booked
        SlotRepository::mark_booked_tx(&mut *tx, slot_id).await?;

        // 5. Write outbox event — atomic with the payment update above.
        let event_amount = match payment_status {
            PaymentStatus::PartiallyPaid => updated_booking.paid_amount.clone(),
            PaymentStatus::FullyPaid => slot.final_price.clone(),
            PaymentStatus::Unpaid => {
                unreachable!("process_gateway_payment is never called with Unpaid")
            }
        };

        //    Fetch user email for outbox event
        let user = match UserRepository::find_by_id_tx(&mut *tx, user_id).await? {
            Some(u) => u,
            None => {
                return Err(AppError::not_found(
                    error_codes::USER_NOT_FOUND,
                    "User profile not found",
                ))
            }
        };

        let game = match GameRepository::find_by_id_tx(&mut *tx, slot.game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Parent game not found",
                ))
            }
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Parent turf not found",
                ))
            }
        };

        let outbox_id = Uuid::new_v4();

        let payload = serde_json::json!({
            "outbox_id":       outbox_id,
            "booking_id":      updated_booking.id,
            "user_email":      user.email,
            "contact_name":    updated_booking.contact_name,
            "turf_name":       turf.name,
            "game_name":       game.name,
            "slot_start":      slot.start_time.to_rfc3339(),
            "amount":          event_amount.to_string(),
            "total_price":     updated_booking.total_price.to_string(),
            "paid_amount":     updated_booking.paid_amount.to_string(),
            "due_amount":      updated_booking.unpaid_amount.to_string(),
            "payment_status":  updated_booking.payment_status,
        });

        sqlx::query(
            "INSERT INTO outbox (id, event_type, payload)
             VALUES ($1, 'BookingCreated', $2)",
        )
        .bind(outbox_id)
        .bind(payload)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to insert outbox row");
            AppError::internal(
                error_codes::INTERNAL_ERROR,
                "Failed to queue booking confirmation event",
            )
        })?;

        // 6. Commit — booking payment, slot status, and outbox row all land together.
        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize payment",
            )
        })?;

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
        Self::process_gateway_payment(state, slot_id, user_id, PaymentStatus::PartiallyPaid).await
    }

    /// Mock Payment Webhook — Full 100%
    /// Sets booking to 'Fully_Paid' and locks slot as 'Booked'.
    #[tracing::instrument(skip(state), fields(slot_id, user_id), name = "booking.pay_full")]
    pub async fn pay_full(
        state: &SharedState,
        slot_id: i32,
        user_id: i32,
    ) -> Result<Booking, AppError> {
        Self::process_gateway_payment(state, slot_id, user_id, PaymentStatus::FullyPaid).await
    }

    /// Unified booking fetcher.
    /// Supports user bookings, game bookings, manager's today schedule, and admin views.
    #[tracing::instrument(
        skip(state),
        fields(caller_id, ?caller_role, ?filter),
        name = "booking.get_bookings"
    )]
    pub async fn get_bookings(
        state: &SharedState,
        caller_id: i32,
        caller_role: &crate::domain::models::users::Role,
        filter: crate::domain::models::bookings::BookingFilter,
    ) -> Result<Vec<crate::domain::models::bookings::BookingDetail>, AppError> {
        use crate::domain::models::users::Role;

        let is_admin = matches!(caller_role, Role::Admin);
        let is_manager = matches!(caller_role, Role::Manager);
        let for_today = filter.today.unwrap_or(false);

        // 1. Today's schedule filter
        if for_today {
            if !is_admin && !is_manager {
                return Err(AppError::forbidden(
                    error_codes::FORBIDDEN,
                    "Only managers and admins can view today's schedule",
                ));
            }
            let manager_id = if is_admin { None } else { Some(caller_id) };
            return BookingRepository::find_bookings_enriched(state, None, None, manager_id, true)
                .await;
        }

        // 2. Specific game filter
        if let Some(game_id) = filter.game_id {
            if !is_admin && !is_manager {
                return Err(AppError::forbidden(
                    error_codes::FORBIDDEN,
                    "Regular users cannot view game bookings",
                ));
            }
            if is_manager {
                let game = match GameRepository::find_by_id(state, game_id).await? {
                    Some(g) => g,
                    None => {
                        return Err(AppError::not_found(
                            error_codes::GAME_NOT_FOUND,
                            "Game not found",
                        ))
                    }
                };

                let turf = match TurfRepository::find_by_id(state, game.turf_id).await? {
                    Some(t) => t,
                    None => {
                        return Err(AppError::not_found(
                            error_codes::TURF_NOT_FOUND,
                            "Turf not found",
                        ))
                    }
                };

                if turf.manager_id != caller_id {
                    return Err(AppError::forbidden(
                        error_codes::FORBIDDEN,
                        "You are not the manager of this turf",
                    ));
                }
            }
            return BookingRepository::find_bookings_enriched(
                state,
                None,
                Some(game_id),
                None,
                false,
            )
            .await;
        }

        // 3. User bookings filter
        if let Some(user_id) = filter.user_id {
            if user_id != caller_id && !is_admin {
                return Err(AppError::forbidden(
                    error_codes::FORBIDDEN,
                    "You can only view your own bookings",
                ));
            }
            return BookingRepository::find_bookings_enriched(
                state,
                Some(user_id),
                None,
                None,
                false,
            )
            .await;
        }

        // 4. Default fallback: user sees own bookings, manager sees their turfs, admin sees all
        if is_admin {
            BookingRepository::find_bookings_enriched(state, None, None, None, false).await
        } else if is_manager {
            BookingRepository::find_bookings_enriched(state, None, None, Some(caller_id), false)
                .await
        } else {
            BookingRepository::find_bookings_enriched(state, Some(caller_id), None, None, false)
                .await
        }
    }
}
