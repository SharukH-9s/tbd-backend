use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::turfs::Turf;
use axum::http::StatusCode;
use sqlx::query_as;

pub struct TurfRepository;

impl TurfRepository {
    pub async fn find_all(state: &SharedState) -> Result<Vec<Turf>, AppError> {
        query_as::<_, Turf>("SELECT * FROM turfs ORDER BY id ASC")
            .fetch_all(&state.db) // Result<Vec<Turf>, sqlx::Error>
            .await
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch turfs from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    pub async fn find_by_id(state: &SharedState, id: i32) -> Result<Option<Turf>, AppError> {
        query_as::<_, Turf>("SELECT * FROM turfs WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.db) // Result<Option<Turf>, sqlx::Error>
            .await
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch turf from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<Turf>, AppError> {
        query_as::<_, Turf>("SELECT * FROM turfs WHERE id = $1")
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch turf from database (tx)",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    pub async fn find_by_manager_id_tx(
        executor: &mut sqlx::PgConnection,
        manager_id: i32,
    ) -> Result<Option<Turf>, AppError> {
        query_as::<_, Turf>("SELECT * FROM turfs WHERE manager_id = $1")
            .bind(manager_id)
            .fetch_optional(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to fetch turf by manager ID from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        manager_id: i32,
        name: &str,
        location: &str,
        description: &str,
    ) -> Result<Turf, AppError> {
        query_as::<_, Turf>(
            "INSERT INTO turfs (manager_id, name, location, description)
             VALUES ($1, $2, $3, $4)
             RETURNING *",
        )
        .bind(manager_id)
        .bind(name)
        .bind(location)
        .bind(description)
        .fetch_one(executor) // Ok(Turf) or Err(sqlx::Error)
        .await
        .map_err(|e| {
            eprintln!("Database error: {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to insert turf into database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        }) // Result<Turf, AppError>
    }

    pub async fn update(
        state: &SharedState,
        id: i32,
        name: &str,
        location: &str,
        description: &str,
        manager_id: i32,
    ) -> Result<Turf, AppError> {
        query_as::<_, Turf>(
            "UPDATE turfs SET name = $1, location = $2, description = $3, manager_id = $4, updated_at = NOW()
             WHERE id = $5
             RETURNING *"
        )
        .bind(name)
        .bind(location)
        .bind(description)
        .bind(manager_id)
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            eprintln!("Database error: {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to update turf in database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })
    }

    pub async fn update_tx(
        executor: &mut sqlx::PgConnection,
        turf_id: i32,
        name: &str,
        location: &str,
        description: &str,
        manager_id: i32,
    ) -> Result<Turf, AppError> {
        query_as::<_, Turf>(
            "UPDATE turfs SET name = $1, location = $2, description = $3, manager_id = $4, updated_at = NOW()
             WHERE id = $5
             RETURNING *"
        )
        .bind(name)
        .bind(location)
        .bind(description)
        .bind(manager_id)
        .bind(turf_id)
        .fetch_one(executor) // Ok(Turf) or Err(sqlx::Error)
        .await
        .map_err(|e| {
            eprintln!("Database error: {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to update turf in database",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })
    }

    pub async fn delete_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("DELETE FROM turfs WHERE id = $1")
            .bind(id)
            .execute(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to delete turf from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }
}
