use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::games::GameSettings;
use crate::domain::models::users::Role;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::game_settings_repo::GameSettingsRepository;
use crate::repositories::turf_repo::TurfRepository;

use bigdecimal::BigDecimal;

pub struct GameSettingsService;

impl GameSettingsService {
    /// Create a new settings template for a game.
    /// Only the turf's manager or an Admin may add templates.
    pub async fn create_settings(
        state: &SharedState,
        game_id: i32,
        caller_id: i32,
        caller_role: Role,
        label: String,
        base_price: BigDecimal,
        slot_duration_mins: i32,
    ) -> Result<GameSettings, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Verify the game exists
        let game = match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
        };

        // 2. Fetch the parent turf to verify ownership
        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        // 3. Security check: only the turf's manager or Admin may add templates
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::SETTINGS_ACCESS_DENIED, "You do not have permission to add settings to this game"));
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

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for settings creation"))?;

        Ok(new_settings)
    }

    /// List all settings templates for a game.
    /// Public — no auth required (used to populate the frontend dropdown).
    pub async fn get_settings_for_game(
        state: &SharedState,
        game_id: i32,
    ) -> Result<Vec<GameSettings>, AppError> {
        // Verify the game exists first (gives the caller a clean 404 instead of an empty list)
        let mut tx = state.db.acquire().await.map_err(|_| AppError::internal(error_codes::DATABASE_ERROR, "Failed to acquire database connection"))?;

        match GameRepository::find_by_id_tx(&mut *tx, game_id).await? {
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Game not found")),
            Some(_) => {}
        }

        GameSettingsRepository::find_all_by_game_id(state, game_id).await
    }

    /// Update an existing settings template.
    /// Only the turf's manager or Admin may update templates.
    /// All fields are optional — omitted fields keep their existing value.
    pub async fn update_settings(
        state: &SharedState,
        settings_id: i32,
        caller_id: i32,
        caller_role: Role,
        label: Option<String>,
        base_price: Option<BigDecimal>,
        slot_duration_mins: Option<i32>,
    ) -> Result<GameSettings, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Fetch the settings template
        let existing = match GameSettingsRepository::find_by_id_tx(&mut *tx, settings_id).await? {
            Some(s) => s,
            None => return Err(AppError::not_found(error_codes::SETTINGS_NOT_FOUND, "Game settings template not found")),
        };

        // 2. Fetch the parent game → parent turf to check ownership
        let game = match GameRepository::find_by_id_tx(&mut *tx, existing.game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Parent game not found")),
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::SETTINGS_ACCESS_DENIED, "You do not have permission to update these settings"));
        }

        // 4. Merge fields — omitted fields fall back to existing values
        let final_label = label.unwrap_or(existing.label);
        let final_price = base_price.unwrap_or(existing.base_price);
        let final_duration = slot_duration_mins.unwrap_or(existing.slot_duration_mins);

        // 5. Update
        let updated = GameSettingsRepository::update_tx(
            &mut *tx,
            settings_id,
            &final_label,
            final_price,
            final_duration,
        )
        .await?;

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for settings update"))?;

        Ok(updated)
    }

    /// Delete a settings template by ID.
    /// Only the turf's manager or Admin may delete templates.
    pub async fn delete_settings(
        state: &SharedState,
        settings_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        // 1. Fetch the settings template
        let existing = match GameSettingsRepository::find_by_id_tx(&mut *tx, settings_id).await? {
            Some(s) => s,
            None => return Err(AppError::not_found(error_codes::SETTINGS_NOT_FOUND, "Game settings template not found")),
        };

        // 2. Fetch parent game → parent turf for ownership check
        let game = match GameRepository::find_by_id_tx(&mut *tx, existing.game_id).await? {
            Some(g) => g,
            None => return Err(AppError::not_found(error_codes::GAME_NOT_FOUND, "Parent game not found")),
        };

        let turf = match TurfRepository::find_by_id_tx(&mut *tx, game.turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Parent turf not found")),
        };

        // 3. Security check
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::SETTINGS_ACCESS_DENIED, "You do not have permission to delete these settings"));
        }

        // 4. Delete
        GameSettingsRepository::delete_tx(&mut *tx, settings_id).await?;

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for settings deletion"))
    }
}
