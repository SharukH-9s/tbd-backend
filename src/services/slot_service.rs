use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::slots::Slot;
use crate::domain::models::users::Role;
use crate::repositories::booking_repo::BookingRepository;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::game_settings_repo::GameSettingsRepository;
use crate::repositories::slot_repo::SlotRepository;
use crate::repositories::turf_repo::TurfRepository;
use axum::http::StatusCode;
use bigdecimal::BigDecimal;
use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Asia::Dhaka;

pub struct SlotService;

impl SlotService {
    /// Creates the next sequential slot for a game on a given business day.
    ///
    /// Steps:
    ///   1. Verify game exists → fetch game_start_time / game_end_time
    ///   2. Verify settings template exists → fetch duration, interval, base_price
    ///   3. Security check (must be the game's turf manager or Admin)
    ///   4. Compute business day window (handles midnight rollover)
    ///   5. Find start_time: MAX(existing end_times) + interval, or game_start_time if none exist
    ///   6. Calculate end_time = start_time + slot_duration_mins
    ///   7. Boundary guard: end_time must not exceed game_end_time
    ///   8. Insert slot
    ///   9. If initial_status == "Booked": validate contact fields, insert booking_repo record
    ///  10. Commit transaction
    pub async fn create_slot(
        state: &SharedState,
        game_id: i32,
        settings_id: i32,
        slot_date: NaiveDate,
        slot_interval_mins: Option<i32>,
        initial_status: Option<String>,
        contact_name: Option<String>,
        contact_phone: Option<String>,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<Slot, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // ── 1. Verify the game exists ─
        let game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError {
                    code: error_codes::GAME_NOT_FOUND,
                    message: "Game not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // ── 2. Verify the settings template exists ───────────────────────────
        let settings = match GameSettingsRepository::find_by_id_tx(&mut *tx, settings_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError {
                    code: error_codes::SETTINGS_NOT_FOUND,
                    message: "Game settings template not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // Guard: settings must belong to the same game
        if settings.game_id != game_id {
            return Err(AppError {
                code: error_codes::SETTINGS_NOT_FOUND,
                message: "Settings template does not belong to this game",
                status: StatusCode::BAD_REQUEST,
            });
        }

        // ── 3. Security check ────────────────────────────────────────────────
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError {
                    code: error_codes::TURF_NOT_FOUND,
                    message: "Parent turf not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "Only the turf manager or an Admin can create slots",
                status: StatusCode::FORBIDDEN,
            });
        }

        // ── 4. Compute business day window (midnight rollover safe) ───────────
        // Convert NaiveDate + NaiveTime into a UTC DateTime using Dhaka timezone.
        // If game_end_time < game_start_time it means the game crosses midnight
        // (e.g. 07:00 AM → 01:00 AM next day), so end belongs to date + 1 day.
        let business_day_start: DateTime<Utc> = {
            let naive = NaiveDateTime::new(slot_date, game.game_start_time); // takes naive date and time and returns a NaiveDateTime
            Dhaka
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute business day start time",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })? // unwraps the Result to DateTime<Utc>
                .with_timezone(&Utc) // converts to UTC DateTime and returned as business_day_start
        };

        let business_day_end: DateTime<Utc> = {
            let end_date = if game.game_end_time < game.game_start_time {
                // Crosses midnight — end belongs to the NEXT calendar day
                slot_date.succ_opt().ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute next business day",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })? // unwraps the Result to DateTime<Utc>
            } else {
                slot_date
            };
            let naive = NaiveDateTime::new(end_date, game.game_end_time); // takes naive date and time and returns a NaiveDateTime
            Dhaka
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute business day end time",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })? // unwraps the Result to DateTime<Utc>
                .with_timezone(&Utc) // converts to UTC DateTime and returned as business_day_end
        };

        // ── 5. Find start_time ───────────────────────────────────────────────
        // Ask: "What is the latest end_time of slots already created today?"
        // If None → this is the first slot of the day → start at game_start_time.
        // If Some → start after (latest_end + interval gap).
        let interval = slot_interval_mins.unwrap_or(10) as i64; // default 10 mins
        let start_time: DateTime<Utc> = match SlotRepository::find_latest_end_time_tx(
            &mut *tx,
            game_id,
            business_day_start,
            business_day_end,
        )
        .await? // unwraps the Result to Option<DateTime<Utc>>
        {
            None => business_day_start, // No slots yet — start at opening time
            Some(latest_end) => latest_end + Duration::minutes(interval),
        };

        // ── 6. Calculate end_time ────────────────────────────────────────────
        let end_time: DateTime<Utc> =
            start_time + Duration::minutes(settings.slot_duration_mins as i64);

        // ── 7. Boundary guard ────────────────────────────────────────────────
        // The calculated end_time must not go past the game's closing time.
        if end_time > business_day_end {
            return Err(AppError {
                code: error_codes::SLOT_EXCEEDS_BOUNDARY,
                message: "Cannot fit the requested slot — calculated end time exceeds the game closing time",
                status: StatusCode::BAD_REQUEST,
            });
        }

        // ── 8. Resolve status & offline booking validation ───────────────────
        let status = initial_status.as_deref().unwrap_or("Available");

        let mut final_contact_name = String::new();
        let mut final_contact_phone = String::new();

        if status == "Booked" {
            final_contact_name = contact_name.as_deref().unwrap_or("").trim().to_string();
            final_contact_phone = contact_phone.as_deref().unwrap_or("").trim().to_string();

            if final_contact_name.is_empty() || final_contact_phone.is_empty() {
                return Err(AppError {
                    code: error_codes::BOTH_CONTACT_NAME_AND_PHONE_REQUIRED,
                    message: "both contact_name and contact_phone are required for offline bookings",
                    status: StatusCode::BAD_REQUEST,
                });
            }
        } else if status != "Available" {
            // Only "Available" and "Booked" are valid initial statuses
            return Err(AppError {
                code: error_codes::SLOT_INVALID_STATUS,
                message: "initial_status must be 'Available' or 'Booked'",
                status: StatusCode::BAD_REQUEST,
            });
        }

        // ── 9. Insert the slot ───────────────────────────────────────────────
        let slot = SlotRepository::insert_tx(
            &mut *tx,
            game_id,
            Some(settings_id),
            None, // label not applicable for sequential slots
            start_time,
            end_time,
            settings.base_price,
            status,
        )
        .await?;

        // ── 9b. If offline booking — insert the booking record atomically ─────
        // If a non-registered user makes a booking through the manager,
        // that booking needs to be recorded in the database as an offline booking.
        // So in the booking table, only contact_name and contact_phone will be filled and user_id will be NULL.
        // This runs inside the same transaction so either both succeed or both rollback.
        if status == "Booked" {
            BookingRepository::insert_offline_tx(
                &mut *tx,
                slot.id,
                &final_contact_name,
                &final_contact_phone,
            )
            .await?;
        }

        // ── 10. Commit ──────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize slot creation",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(slot)
    }

    /// Returns all slots for a specific game on a business day.
    /// No auth required — used for the manager's slot calendar view.
    pub async fn get_slots_for_game_on_date(
        state: &SharedState,
        game_id: i32,
        slot_date: NaiveDate,
    ) -> Result<Vec<Slot>, AppError> {
        // Need one connection to fetch the game's opening hours first
        let mut conn = state.db.acquire().await.map_err(|_| AppError {
            code: error_codes::DATABASE_ERROR,
            message: "Failed to acquire database connection",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        let game = match GameRepository::find_by_id_tx(&mut *conn, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError {
                    code: error_codes::GAME_NOT_FOUND,
                    message: "Game not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // Compute business day start (on the requested date at game's opening time)
        let business_day_start: DateTime<Utc> = {
            let naive = NaiveDateTime::new(slot_date, game.game_start_time);
            Dhaka
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute business day start time",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
                .with_timezone(&Utc)
        };

        // Compute business day end
        // If game crosses midnight (end_time < start_time): end belongs to next calendar day
        // Otherwise: end belongs to same calendar day
        let business_day_end: DateTime<Utc> = {
            let end_date = if game.game_end_time < game.game_start_time {
                slot_date.succ_opt().ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute next business day",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
            } else {
                slot_date
            };
            let naive = NaiveDateTime::new(end_date, game.game_end_time);
            Dhaka
                .from_local_datetime(&naive)
                .single()
                .ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute business day end time",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
                .with_timezone(&Utc)
        };

        // Single query: fetch all slots in the business day window
        // For midnight-crossing games (e.g., 8 AM to 2 AM):
        //   - May 5 query: May 5 08:00 to May 6 02:00 (includes early morning slots from May 6)
        //   - May 6 query: May 6 08:00 to May 7 02:00 (includes early morning slots from May 7)
        // Early morning slots naturally belong to their correct business day
        let slots = SlotRepository::find_all_by_game_and_window(
            state,
            game_id,
            business_day_start,
            business_day_end,
        )
        .await?;

        Ok(slots)
    }

    /// Creates a custom slot at explicit start_time and end_time (allows filling gaps after deletions).
    ///
    /// Steps:
    ///   1. Verify game exists → fetch game_start_time / game_end_time
    ///   2. Verify settings template exists → fetch base_price
    ///   3. Security check (must be the game's turf manager or Admin)
    ///   4. Compute business day window (handles midnight rollover)
    ///   5. Validate: start_time & end_time must be within business hours
    ///   6. Check for overlaps with existing slots
    ///   7. Insert slot
    ///   8. If initial_status == "Booked": validate contact fields, insert booking record
    ///   9. Commit transaction
    pub async fn create_custom_slot(
        state: &SharedState,
        game_id: i32,
        label: Option<String>,
        price: BigDecimal,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        initial_status: Option<String>,
        contact_name: Option<String>,
        contact_phone: Option<String>,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<Slot, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // ── 1. Verify the game exists ─
        let game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError {
                    code: error_codes::GAME_NOT_FOUND,
                    message: "Game not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };


        // ── 3. Security check ────────────────────────────────────────────────
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError {
                    code: error_codes::TURF_NOT_FOUND,
                    message: "Parent turf not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "Only the turf manager or an Admin can create slots",
                status: StatusCode::FORBIDDEN,
            });
        }

        // ── 5. Validate boundaries ───────────────────────────────────────────
        // Convert times to Dhaka to check if they fall within business hours
        let start_time_dhaka = start_time.with_timezone(&Dhaka);
        let end_time_dhaka = end_time.with_timezone(&Dhaka);

        let start_local_time = start_time_dhaka.time();
        let end_local_time = end_time_dhaka.time();

        // Check if times are within business hours (accounting for midnight crossing)
        let start_within_hours = if game.game_end_time < game.game_start_time {
            // Game crosses midnight (e.g., 8 AM to 2 AM)
            start_local_time >= game.game_start_time || start_local_time < game.game_end_time
        } else {
            // Game doesn't cross midnight
            start_local_time >= game.game_start_time && start_local_time < game.game_end_time
        };

        let end_within_hours = if game.game_end_time < game.game_start_time {
            // Game crosses midnight
            end_local_time >= game.game_start_time || end_local_time < game.game_end_time
        } else {
            // Game doesn't cross midnight
            end_local_time >= game.game_start_time && end_local_time < game.game_end_time
        };

        if !start_within_hours {
            return Err(AppError {
                code: error_codes::BAD_REQUEST,
                message: "Start time is before the game's opening time",
                status: StatusCode::BAD_REQUEST,
            });
        }

        if !end_within_hours {
            return Err(AppError {
                code: error_codes::SLOT_EXCEEDS_BOUNDARY,
                message: "End time exceeds the game's closing time",
                status: StatusCode::BAD_REQUEST,
            });
        }

        if end_time <= start_time {
            return Err(AppError {
                code: error_codes::BAD_REQUEST,
                message: "End time must be after start time",
                status: StatusCode::BAD_REQUEST,
            });
        }

        // ── 7. Check for overlaps with existing slots ────────────────────────
        let conflicting_slots = SlotRepository::find_overlapping_slots_tx(
            &mut *tx,
            game_id,
            start_time,
            end_time,
        )
        .await?;

        if !conflicting_slots.is_empty() {
            return Err(AppError {
                code: error_codes::SLOT_OVERLAP,
                message: "The requested time slot overlaps with an existing slot",
                status: StatusCode::CONFLICT,
            });
        }

        // ── 8. Resolve status & offline booking validation ───────────────────
        let status = initial_status.as_deref().unwrap_or("Available");

        let mut final_contact_name = String::new();
        let mut final_contact_phone = String::new();

        if status == "Booked" {
            final_contact_name = contact_name.as_deref().unwrap_or("").trim().to_string();
            final_contact_phone = contact_phone.as_deref().unwrap_or("").trim().to_string();

            if final_contact_name.is_empty() || final_contact_phone.is_empty() {
                return Err(AppError {
                    code: error_codes::BOTH_CONTACT_NAME_AND_PHONE_REQUIRED,
                    message: "both contact_name and contact_phone are required for offline bookings",
                    status: StatusCode::BAD_REQUEST,
                });
            }
        } else if status != "Available" {
            return Err(AppError {
                code: error_codes::SLOT_INVALID_STATUS,
                message: "initial_status must be 'Available' or 'Booked'",
                status: StatusCode::BAD_REQUEST,
            });
        }

        // ── 9. Insert the slot ───────────────────────────────────────────────
        let slot = SlotRepository::insert_tx(
            &mut *tx,
            game_id,
            None,  // no settings template for custom slots
            label.as_deref(),
            start_time,
            end_time,
            price,
            status,
        )
        .await?;

        // ── 9b. If offline booking — insert the booking record atomically ─────
        if status == "Booked" {
            BookingRepository::insert_offline_tx(
                &mut *tx,
                slot.id,
                &final_contact_name,
                &final_contact_phone,
            )
            .await?;
        }

        // ── 10. Commit ──────────────────────────────────────────────────────
        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize custom slot creation",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(slot)
    }

    /// Deletes a single slot by ID. Only Available slots may be deleted.
    /// Pending/Booked slots are financial records and must never be deleted.
    pub async fn delete_slot(
        state: &SharedState,
        slot_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Fetch the slot
        let slot = match SlotRepository::find_by_id_tx(&mut *tx, slot_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError {
                    code: error_codes::SLOT_NOT_FOUND,
                    message: "Slot not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 2. Fetch parent game → turf for ownership check
        let game = match GameRepository::find_by_id_tx(&mut *tx, slot.game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError {
                    code: error_codes::GAME_NOT_FOUND,
                    message: "Parent game not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError {
                    code: error_codes::TURF_NOT_FOUND,
                    message: "Parent turf not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "Only the turf manager or an Admin can delete slots",
                status: StatusCode::FORBIDDEN,
            });
        }

        // 4. Status guard — Pending and Booked are financial records, never delete them
        if slot.status != "Available" {
            return Err(AppError {
                code: error_codes::SLOT_NOT_DELETABLE,
                message: "Only Available slots can be deleted. Pending and Booked slots are protected.",
                status: StatusCode::CONFLICT,
            });
        }

        // 5. Delete
        SlotRepository::delete_tx(&mut *tx, slot_id).await?;

        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize slot deletion",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(())
    }
}
