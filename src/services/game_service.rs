use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::games::Game;
use crate::domain::models::users::Role;
use crate::repositories::game_repo::GameRepository;
use crate::repositories::turf_repo::TurfRepository;
use axum::http::StatusCode;
use bigdecimal::BigDecimal;

pub struct GameService;

impl GameService {
    pub async fn create_game(
        state: &SharedState,
        turf_id: i32,
        caller_id: i32,
        caller_role: Role,
        name: String,
        price: BigDecimal,
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
        let newly_created_game = GameRepository::insert_tx(&mut *tx, turf_id, &name, price).await?;

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
        // First verify turf exists
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

        let game = GameRepository::find_a_game_by_id_tx(&mut *conn, game_id).await?;
        match game {
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
        price: Option<BigDecimal>,
    ) -> Result<Game, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Fetch the game
        let existing_game = match GameRepository::find_a_game_by_id_tx(&mut *tx, game_id).await? {
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
                // This shouldn't happen due to FK, but safe to check
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

        // 4. Merge fields
        let final_name = name.unwrap_or(existing_game.name);
        let final_price = price.unwrap_or(existing_game.price_per_hour);

        // 5. Update
        let updated_game =
            GameRepository::update_tx(&mut *tx, game_id, &final_name, final_price).await?;

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
        let existing_game = match GameRepository::find_a_game_by_id_tx(&mut *tx, game_id).await? {
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

        // 4. Delete
        GameRepository::delete_tx(&mut *tx, game_id).await?;

        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes for game deletion",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(())
    }
}
