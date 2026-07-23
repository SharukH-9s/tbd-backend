use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::games::Game;
use crate::domain::models::users::Role;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::slot_repo::SlotRepository;
use crate::repositories::turf_repo::TurfRepository;
use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use chrono_tz::Asia::Dhaka;

pub struct GameService;

impl GameService {
    #[tracing::instrument(skip(state), fields(turf_id, caller_id, game_name = %name), name = "game.create")]
    pub async fn create_game(
        state: &SharedState,
        turf_id: i32,
        caller_id: i32,
        caller_role: Role,
        name: String,
        game_start_time: NaiveTime,
        game_end_time: NaiveTime,
    ) -> Result<Game, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Verify turf exists
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
        };

        // 2. Verify permission (Admin or Turf Manager)
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "You do not have permission to add games to this turf"));
        }

        // 3. Insert game
        let newly_created_game =
            GameRepository::insert_tx(&mut *tx, turf_id, &name, game_start_time, game_end_time)
                .await?;

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for game creation"))?;

        Ok(newly_created_game)
    }

    #[tracing::instrument(skip(state), fields(turf_id), name = "game.get_for_turf")]
    pub async fn get_games_for_turf(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<Game>, AppError> {
        // Verify turf exists first
        let turf = TurfRepository::find_by_id(state, turf_id).await?;
        if turf.is_none() {
            return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found"));
        }

        GameRepository::find_all_games_by_turf_id(state, turf_id).await
    }

    #[tracing::instrument(skip(state), fields(game_id), name = "game.get_by_id")]
    pub async fn get_game_by_id(state: &SharedState, game_id: i32) -> Result<Game, AppError> {
        let mut conn = state.db.acquire().await.map_err(|_| AppError::internal(error_codes::DATABASE_ERROR, "Failed to acquire database connection"))?;

        match GameRepository::find_by_id_tx(&mut *conn, game_id).await? {
            Some(g) => Ok(g),
            None => Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
        }
    }

    #[tracing::instrument(skip(state), fields(game_id, caller_id), name = "game.update")]
    pub async fn update_game(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
        name: Option<String>,
        game_start_time: Option<NaiveTime>,
        game_end_time: Option<NaiveTime>,
    ) -> Result<Game, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
        };

        // 2. Fetch the turf to check ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, existing_game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "You do not have permission to update this game"));
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
                .ok_or_else(|| AppError::internal(error_codes::DATABASE_ERROR, "Failed to compute proposed business day start time"))?
                .with_timezone(&Utc);

            // Build proposed window end (handles midnight rollover)
            let end_date = if final_end < final_start {
                today.succ_opt().ok_or_else(|| AppError::internal(error_codes::DATABASE_ERROR, "Failed to compute next business day for conflict check"))?
            } else {
                today
            };
            let new_window_end = Dhaka
                .from_local_datetime(&NaiveDateTime::new(end_date, final_end))
                .single()
                .ok_or_else(|| AppError::internal(error_codes::DATABASE_ERROR, "Failed to compute proposed business day end time"))?
                .with_timezone(&Utc);

            let orphaned_count = SlotRepository::count_active_slots_outside_window_tx(
                &mut *tx,
                game_id,
                new_window_start,
                new_window_end,
            )
            .await?;

            if orphaned_count > 0 {
                return Err(AppError::conflict(error_codes::GAME_WINDOW_CONFLICT, "Cannot narrow the game window: active (Booked or Pending) slots exist outside the proposed operating hours"));
            }
        }

        // 6. Update
        let updated_game =
            GameRepository::update_tx(&mut *tx, game_id, &final_name, final_start, final_end)
                .await?;

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for game update"))?;

        Ok(updated_game)
    }

    #[tracing::instrument(skip(state), fields(game_id, caller_id), name = "game.delete")]
    pub async fn delete_game(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
        };

        // 2. Fetch the turf to check ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, existing_game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "You do not have permission to delete this game"));
        }

        // 4. Protection: Prevent deletion if active bookings exist
        let active_count = SlotRepository::count_active_slots_by_game_tx(&mut *tx, game_id).await?;
        if active_count > 0 {
            return Err(AppError::conflict(error_codes::GAME_HAS_ACTIVE_BOOKINGS, "Cannot delete game: active bookings (Booked or Pending) exist for this game"));
        }

        // 5. Delete
        GameRepository::delete_tx(&mut *tx, game_id).await?;

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for game deletion"))
    }
}
