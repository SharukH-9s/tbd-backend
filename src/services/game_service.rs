use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::games::Game;
use crate::domain::models::users::Role;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::slot_repo::SlotRepository;
use crate::repositories::turf_repo::TurfRepository;
use axum::http::StatusCode;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Asia::Dhaka;

pub struct GameService;

impl GameService {
    pub async fn create_game(
        state: &SharedState,
        turf_id: i32,
        caller_id: i32,
        caller_role: Role,
        name: String,
        game_start_time: NaiveTime,
        game_end_time: NaiveTime,
    ) -> Result<Game, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Verify turf exists
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError {
                    code: error_codes::TURF_NOT_FOUND,
                    message: "Turf not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 2. Verify permission (Admin or Turf Manager)
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError {
                code: error_codes::GAME_ACCESS_DENIED,
                message: "You do not have permission to add games to this turf",
                status: StatusCode::FORBIDDEN,
            });
        }

        // 3. Insert game
        let newly_created_game =
            GameRepository::insert_tx(&mut *tx, turf_id, &name, game_start_time, game_end_time)
                .await?;

        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes for game creation",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(newly_created_game)
    }

    pub async fn get_games_for_turf(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<Game>, AppError> {
        // Verify turf exists first
        let turf = TurfRepository::find_by_id(state, turf_id).await?;
        if turf.is_none() {
            return Err(AppError {
                code: error_codes::TURF_NOT_FOUND,
                message: "Turf not found",
                status: StatusCode::NOT_FOUND,
            });
        }

        GameRepository::find_all_games_by_turf_id(state, turf_id).await
    }

    pub async fn get_game_by_id(state: &SharedState, game_id: i32) -> Result<Game, AppError> {
        let mut conn = state.db.acquire().await.map_err(|_| AppError {
            code: error_codes::DATABASE_ERROR,
            message: "Failed to acquire database connection",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        match GameRepository::find_by_id_tx(&mut *conn, game_id).await? {
            Some(g) => Ok(g),
            None => Err(AppError {
                code: error_codes::GAME_NOT_FOUND,
                message: "Game not found",
                status: StatusCode::NOT_FOUND,
            }),
        }
    }

    pub async fn update_game(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
        name: Option<String>,
        game_start_time: Option<NaiveTime>,
        game_end_time: Option<NaiveTime>,
    ) -> Result<Game, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError {
                    code: error_codes::GAME_NOT_FOUND,
                    message: "Game not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 2. Fetch the turf to check ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, existing_game.turf_id).await? {
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
                code: error_codes::GAME_ACCESS_DENIED,
                message: "You do not have permission to update this game",
                status: StatusCode::FORBIDDEN,
            });
        }

        // 4. Merge fields (keep existing value if caller omitted the field)
        let final_name = name.unwrap_or(existing_game.name);
        let final_start = game_start_time.unwrap_or(existing_game.game_start_time);
        let final_end = game_end_time.unwrap_or(existing_game.game_end_time);

        // 5. Window-conflict guard ────────────────────────────────────────────
        // If either boundary changed, check whether any Booked/Pending slots
        // would fall outside the new window.  We use an arbitrary representative
        // date (today) because the window is purely time-based — the same
        // start/end times apply to every business day.
        if final_start != existing_game.game_start_time || final_end != existing_game.game_end_time
        {
            let today: NaiveDate = Utc::now().with_timezone(&Dhaka).date_naive();

            // Build proposed window start
            let new_window_start = Dhaka
                .from_local_datetime(&NaiveDateTime::new(today, final_start))
                .single()
                .ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute proposed business day start time",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
                .with_timezone(&Utc);

            // Build proposed window end (handles midnight rollover)
            let end_date = if final_end < final_start {
                today.succ_opt().ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute next business day for conflict check",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
            } else {
                today
            };
            let new_window_end = Dhaka
                .from_local_datetime(&NaiveDateTime::new(end_date, final_end))
                .single()
                .ok_or_else(|| AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to compute proposed business day end time",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
                .with_timezone(&Utc);

            let orphaned_count = SlotRepository::count_active_slots_outside_window_tx(
                &mut *tx,
                game_id,
                new_window_start,
                new_window_end,
            )
            .await?;

            if orphaned_count > 0 {
                return Err(AppError {
                    code: error_codes::GAME_WINDOW_CONFLICT,
                    message: "Cannot narrow the game window: active (Booked or Pending) slots exist outside the proposed operating hours",
                    status: StatusCode::CONFLICT,
                });
            }
        }

        // 6. Update
        let updated_game =
            GameRepository::update_tx(&mut *tx, game_id, &final_name, final_start, final_end)
                .await?;

        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes for game update",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(updated_game)
    }

    pub async fn delete_game(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError {
                    code: error_codes::GAME_NOT_FOUND,
                    message: "Game not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 2. Fetch the turf to check ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, existing_game.turf_id).await? {
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
                code: error_codes::GAME_ACCESS_DENIED,
                message: "You do not have permission to delete this game",
                status: StatusCode::FORBIDDEN,
            });
        }

        // 4. Protection: Prevent deletion if active bookings exist
        let active_count = SlotRepository::count_active_slots_by_game_tx(&mut *tx, game_id).await?;
        if active_count > 0 {
            return Err(AppError {
                code: error_codes::GAME_HAS_ACTIVE_BOOKINGS,
                message: "Cannot delete game: active bookings (Booked or Pending) exist for this game",
                status: StatusCode::CONFLICT,
            });
        }

        // 5. Delete
        GameRepository::delete_tx(&mut *tx, game_id).await?;

        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes for game deletion",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })
    }
}
