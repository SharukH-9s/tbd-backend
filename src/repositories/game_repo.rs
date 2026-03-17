use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::games::Game;
use axum::http::StatusCode;
use bigdecimal::BigDecimal;
use sqlx::query_as;

pub struct GameRepository;

impl GameRepository {
    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        turf_id: i32,
        name: &str,
        price: BigDecimal,
    ) -> Result<Game, AppError> {
        query_as::<_, Game>(
            "INSERT INTO games (turf_id, name, price_per_hour)
             VALUES ($1, $2, $3)
             RETURNING *",
        )
        .bind(turf_id)
        .bind(name)
        .bind(price)
        .fetch_one(executor)
        .await
        .map_err(|e| {
            eprintln!("Database error in insert_tx (game): {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to insert game into database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })
    }

    pub async fn find_all_games_by_turf_id(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<Game>, AppError> {
        query_as::<_, Game>("SELECT * FROM games WHERE turf_id = $1 ORDER BY id ASC")
            .bind(turf_id)
            .fetch_all(&state.db)
            .await
            .map_err(|e| {
                eprintln!("Database error in find_by_turf_id (game): {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch games from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    pub async fn find_a_game_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<Game>, AppError> {
        query_as::<_, Game>("SELECT * FROM games WHERE id = $1")
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error in find_by_id_tx (game): {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch game from database (tx)",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    pub async fn update_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
        name: &str,
        price: BigDecimal,
    ) -> Result<Game, AppError> {
        query_as::<_, Game>(
            "UPDATE games SET name = $1, price_per_hour = $2
             WHERE id = $3
             RETURNING *",
        )
        .bind(name)
        .bind(price)
        .bind(id)
        .fetch_one(executor)
        .await
        .map_err(|e| {
            eprintln!("Database error in update_tx (game): {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to update game in database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })
    }

    pub async fn delete_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("DELETE FROM games WHERE id = $1")
            .bind(id)
            .execute(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error in delete_tx (game): {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to delete game from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }
}
