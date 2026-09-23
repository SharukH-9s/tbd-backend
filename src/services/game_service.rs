use crate::config::state::SharedState;
use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::games::Game;
use crate::domain::models::users::Role;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::slot_repo::SlotRepository;
use crate::repositories::turf_repo::TurfRepository;
use chrono::NaiveTime;
use std::sync::Arc;

pub struct GameService;

impl GameService {
    // -----------------------------------------------------------------------
    // Cache invalidation helper
    // -----------------------------------------------------------------------

    /// Invalidates L1 and L2 cache keys for a specific game and its parent turf's game list.
    async fn invalidate_game_cache(state: &SharedState, game_id: i32, turf_id: i32) {
        // Per-game key
        let game_key = format!("cache:game:{}", game_id);
        state.l2_cache.delete(&game_key).await;
        state.cache.invalidate(&game_key).await;

        // Turf's full game list
        let turf_games_key = format!("cache:game:turf:{}", turf_id);
        state.l2_cache.delete(&turf_games_key).await;
        state.cache.invalidate(&turf_games_key).await;
    }

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    #[tracing::instrument(skip(state), fields(turf_id), name = "game.get_for_turf")]
    pub async fn get_games_for_turf(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<Game>, AppError> {
        // Verify turf exists first (clean 404 before touching the cache)
        let turf = TurfRepository::find_by_id(state, turf_id).await?;
        let _turf = match turf {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Turf not found",
                ));
            }
        };

        let cache_key = format!("cache:game:turf:{}", turf_id);

        if let Some(json_arc) = state.cache.get(&cache_key).await {
            tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
            return serde_json::from_str(&json_arc)
                .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"));
        }

        let json_arc = state
            .cache
            .try_get_with(&cache_key, async {
                if let Some(json) = state.l2_cache.get(&cache_key).await {
                    tracing::debug!(key = %cache_key, source = "l2", "Cache hit, warming L1");
                    return Ok::<Arc<str>, AppError>(json.into());
                }
                tracing::debug!(key = %cache_key, source = "db", "Cache miss, querying database");
                let games = GameRepository::find_all_games_by_turf_id(state, turf_id).await?;
                let json = serde_json::to_string(&games)
                    .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed"))?;
                state.l2_cache.set(&cache_key, &json, 300).await;
                Ok(json.into())
            })
            .await
            .map_err(|e| {
                if e.status == axum::http::StatusCode::NOT_FOUND {
                    return AppError::not_found(e.code, e.message);
                }
                AppError::internal("CACHE_ERROR", "Cache fetch failed")
            })?;

        let games = serde_json::from_str(&json_arc)
            .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"))?;
        Ok(games)
    }

    #[tracing::instrument(skip(state), fields(game_id), name = "game.get_by_id")]
    pub async fn get_game_by_id(state: &SharedState, game_id: i32) -> Result<Game, AppError> {
        let cache_key = format!("cache:game:{}", game_id);

        if let Some(json_arc) = state.cache.get(&cache_key).await {
            tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
            return serde_json::from_str(&json_arc)
                .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"));
        }

        let json_arc = state
            .cache
            .try_get_with(&cache_key, async {
                if let Some(json) = state.l2_cache.get(&cache_key).await {
                    tracing::debug!(key = %cache_key, source = "l2", "Cache hit, warming L1");
                    return Ok::<Arc<str>, AppError>(json.into());
                }
                tracing::debug!(key = %cache_key, source = "db", "Cache miss, querying database");
                let mut conn = state.db.acquire().await.map_err(|_| {
                    AppError::internal(error_codes::DATABASE_ERROR, "Failed to acquire database connection")
                })?;
                let game = match GameRepository::find_by_id_tx(&mut *conn, game_id).await? {
                    Some(g) => g,
                    None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
                };
                let json = serde_json::to_string(&game)
                    .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed"))?;
                state.l2_cache.set(&cache_key, &json, 300).await;
                Ok(json.into())
            })
            .await
            .map_err(|e| {
                if e.status == axum::http::StatusCode::NOT_FOUND {
                    return AppError::not_found(e.code, e.message);
                }
                AppError::internal("CACHE_ERROR", "Cache fetch failed")
            })?;

        let game = serde_json::from_str(&json_arc)
            .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"))?;
        Ok(game)
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

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
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        if game_start_time == game_end_time {
            return Err(AppError::bad_request(error_codes::BAD_REQUEST, "Game start time and end time cannot be identical. 24-hour operations are not supported."));
        }

        // 1. Verify turf exists
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Turf not found",
                ))
            }
        };

        // 2. Verify permission (Admin or Turf Manager)
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to add games to this turf",
            ));
        }

        // 3. Insert game
        let newly_created_game =
            GameRepository::insert_tx(&mut *tx, turf_id, &name, game_start_time, game_end_time)
                .await?;

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for game creation",
            )
        })?;

        // Invalidate the turf's game list (a new game was added)
        let turf_games_key = format!("cache:game:turf:{}", turf_id);
        state.l2_cache.delete(&turf_games_key).await;
        state.cache.invalidate(&turf_games_key).await;

        Ok(newly_created_game)
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
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Game not found",
                ))
            }
        };

        // 2. Fetch the turf to check ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, existing_game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Parent turf not found",
                ))
            }
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to update this game",
            ));
        }

        // 4. Merge fields (keep existing value if caller omitted the field)
        let final_name = name.unwrap_or(existing_game.name);
        let final_start = game_start_time.unwrap_or(existing_game.game_start_time);
        let final_end = game_end_time.unwrap_or(existing_game.game_end_time);

        if final_start == final_end {
            return Err(AppError::bad_request(error_codes::BAD_REQUEST, "Game start time and end time cannot be identical. 24-hour operations are not supported."));
        }

        // 5. Window-conflict guard
        if final_start != existing_game.game_start_time || final_end != existing_game.game_end_time
        {
            let orphaned_count = SlotRepository::count_active_slots_outside_window_tx(
                &mut *tx,
                game_id,
                final_start,
                final_end,
            )
            .await?;

            if orphaned_count > 0 {
                return Err(AppError::conflict(
                    error_codes::GAME_WINDOW_CONFLICT,
                    "Cannot narrow the game window: slots exist outside the proposed operating hours",
                ));
            }
        }

        // 6. Update
        let updated_game =
            GameRepository::update_tx(&mut *tx, game_id, &final_name, final_start, final_end)
                .await?;

        let turf_id = existing_game.turf_id;

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for game update",
            )
        })?;

        // Invalidate per-game key and turf's game list
        Self::invalidate_game_cache(state, game_id, turf_id).await;

        Ok(updated_game)
    }

    #[tracing::instrument(skip(state), fields(game_id, caller_id), name = "game.delete")]
    pub async fn delete_game(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Game not found",
                ))
            }
        };

        // 2. Fetch the turf to check ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, existing_game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Parent turf not found",
                ))
            }
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to delete this game",
            ));
        }

        // 4. Protection: Prevent deletion if active bookings exist
        let active_count = SlotRepository::count_active_slots_by_game_tx(&mut *tx, game_id).await?;
        if active_count > 0 {
            return Err(AppError::conflict(
                error_codes::GAME_HAS_ACTIVE_BOOKINGS,
                "Cannot delete game: active bookings (Booked or Pending) exist for this game",
            ));
        }

        let turf_id = existing_game.turf_id;

        // 5. Delete
        GameRepository::delete_tx(&mut *tx, game_id).await?;

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for game deletion",
            )
        })?;

        // Invalidate per-game key and turf's game list
        Self::invalidate_game_cache(state, game_id, turf_id).await;

        Ok(())
    }
}
