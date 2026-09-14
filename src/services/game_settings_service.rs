use crate::config::state::SharedState;
use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::games::GameSettings;
use crate::domain::models::users::Role;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::game_settings_repo::GameSettingsRepository;
use crate::repositories::turf_repo::TurfRepository;
use std::sync::Arc;

use bigdecimal::BigDecimal;

pub struct GameSettingsService;

impl GameSettingsService {
    // -----------------------------------------------------------------------
    // Cache invalidation helper
    // -----------------------------------------------------------------------

    /// Invalidates L1 and L2 cache for a game's settings list.
    async fn invalidate_settings_cache(state: &SharedState, game_id: i32) {
        let key = format!("cache:settings:game:{}", game_id);
        state.l2_cache.delete(&key).await;
        state.cache.invalidate(&key).await;
    }

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    /// List all settings templates for a game.
    /// Public — no auth required (used to populate the frontend dropdown).
    #[tracing::instrument(skip(state), fields(game_id), name = "settings.get_for_game")]
    pub async fn get_settings_for_game(
        state: &SharedState,
        game_id: i32,
    ) -> Result<Vec<GameSettings>, AppError> {
        // Verify the game exists first (gives the caller a clean 404 instead of an empty list)
        let mut conn = state.db.acquire().await.map_err(|_| {
            AppError::internal(
                error_codes::DATABASE_ERROR,
                "Failed to acquire database connection",
            )
        })?;

        match GameRepository::find_by_id_tx(&mut *conn, game_id).await? {
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Game not found",
                ))
            }
            Some(_) => {}
        }
        drop(conn); // release the connection back to the pool before the cache path

        let cache_key = format!("cache:settings:game:{}", game_id);

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
                let settings = GameSettingsRepository::find_all_by_game_id(state, game_id).await?;
                let json = serde_json::to_string(&settings)
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

        let settings = serde_json::from_str(&json_arc)
            .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"))?;
        Ok(settings)
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

    /// Create a new settings template for a game.
    /// Only the turf's manager or an Admin may add templates.
    #[tracing::instrument(skip(state), fields(game_id, caller_id, label = %label), name = "settings.create")]
    pub async fn create_settings(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
        label: String,
        base_price: BigDecimal,
        slot_duration_mins: i32,
    ) -> Result<GameSettings, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // 1. Verify the game exists
        let game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => {
                return Err(AppError::not_found(
                    error_codes::GAME_NOT_FOUND,
                    "Game not found",
                ))
            }
        };

        // 2. Fetch the parent turf to verify ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Parent turf not found",
                ))
            }
        };

        // 3. Security check: only the turf's manager or Admin may add templates
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to add settings to this game",
            ));
        }

        // 4. Insert the template
        let new_settings = GameSettingsRepository::insert_tx(
            &mut *tx,
            game_id,
            &label,
            base_price,
            slot_duration_mins,
        )
        .await?;

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for settings creation",
            )
        })?;

        // Invalidate the game's settings list
        Self::invalidate_settings_cache(state, game_id).await;

        Ok(new_settings)
    }

    /// Delete a settings template by ID.
    /// Only the turf's manager or Admin may delete templates.
    #[tracing::instrument(skip(state), fields(settings_id, caller_id), name = "settings.delete")]
    pub async fn delete_settings(
        state: &SharedState,
        settings_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        // 1. Fetch the settings template
        let existing = match GameSettingsRepository::find_by_id_tx(&mut *tx, settings_id).await? {
            Some(s) => s,
            None => {
                return Err(AppError::not_found(
                    error_codes::SETTINGS_NOT_FOUND,
                    "Game settings template not found",
                ))
            }
        };

        // 2. Fetch parent game → parent turf for ownership check
        let game = match GameRepository::find_by_id_tx(&mut *tx, existing.game_id).await? {
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

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to delete these settings",
            ));
        }

        let game_id = existing.game_id;

        // 4. Delete
        GameSettingsRepository::delete_tx(&mut *tx, settings_id).await?;

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for settings deletion",
            )
        })?;

        // Invalidate the game's settings list
        Self::invalidate_settings_cache(state, game_id).await;

        Ok(())
    }
}
