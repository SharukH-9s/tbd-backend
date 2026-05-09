use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::games::GameSettings;
use axum::http::StatusCode;
use bigdecimal::BigDecimal;
use sqlx::query_as;

pub struct GameSettingsRepository;

impl GameSettingsRepository {
    /// Insert a new settings template for a game (inside a transaction).
    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        game_id: i32,
        label: &str, // deref coercion
        base_price: BigDecimal,
        slot_duration_mins: i32,
    ) -> Result<GameSettings, AppError> {
        query_as::<_, GameSettings>(
            "INSERT INTO game_settings (game_id, label, base_price, slot_duration_mins)
             VALUES ($1, $2, $3, $4)
             RETURNING *",
        )
        .bind(game_id)
        .bind(label)
        .bind(base_price)
        .bind(slot_duration_mins)
        .fetch_one(executor)
        .await
        .map_err(|e| {
            eprintln!("Database error in insert_tx (game_settings): {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to insert game settings into database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        }) // Result<GameSettings, AppError>
    }

    /// Fetch all settings templates for a given game (no transaction needed — read-only).
    pub async fn find_all_by_game_id(
        state: &SharedState,
        game_id: i32,
    ) -> Result<Vec<GameSettings>, AppError> {
        query_as::<_, GameSettings>(
            "SELECT * FROM game_settings WHERE game_id = $1 ORDER BY id ASC",
        )
        .bind(game_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| {
            eprintln!(
                "Database error in find_all_by_game_id (game_settings): {}",
                e
            );
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to fetch game settings from database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        }) // Result<Vec<GameSettings>, AppError>
    }

    /// Fetch a single settings template by its own ID (inside a transaction).
    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<GameSettings>, AppError> {
        query_as::<_, GameSettings>("SELECT * FROM game_settings WHERE id = $1")
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error in find_by_id_tx (game_settings): {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch game settings from database (tx)",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            }) // Result<Option<GameSettings>, AppError>
    }

    /// Update an existing settings template (inside a transaction).
    pub async fn update_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
        label: &str, // deref coercion
        base_price: BigDecimal,
        slot_duration_mins: i32,
    ) -> Result<GameSettings, AppError> {
        query_as::<_, GameSettings>(
            "UPDATE game_settings
             SET label = $1, base_price = $2, slot_duration_mins = $3
             WHERE id = $4
             RETURNING *",
        )
        .bind(label)
        .bind(base_price)
        .bind(slot_duration_mins)
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(|e| {
            eprintln!("Database error in update_tx (game_settings): {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to update game settings in database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        }) // Result<GameSettings, AppError>
    }

    /// Delete a settings template by ID (inside a transaction).
    pub async fn delete_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("DELETE FROM game_settings WHERE id = $1")
            .bind(id)
            .execute(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error in delete_tx (game_settings): {}", e);
                let message = if e.to_string().contains("fk") || e.to_string().contains("foreign key") {
                    "Failed to delete game settings from database. There are slots made of this setting."
                } else {
                    "Failed to delete game settings from database"
                };
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message,
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            }) // Result<sqlx::postgres::PgQueryResult, AppError>
    }
}
