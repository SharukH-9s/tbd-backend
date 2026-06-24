use crate::{db_conn_err, db_err};
use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::domain::models::turfs::Turf;
use sqlx::query_as;

pub struct TurfRepository;

impl TurfRepository {
    const TURF_SELECT: &'static str =
        "SELECT t.*, u.username AS manager_name, u.email AS manager_email, u.phone AS manager_phone
         FROM turfs t
         JOIN users u ON t.manager_id = u.id";

    pub async fn find_all(state: &SharedState) -> Result<Vec<Turf>, AppError> {
        query_as::<_, Turf>(&format!("{} ORDER BY t.id ASC", Self::TURF_SELECT))
            .fetch_all(&state.db)
            .await
            .map_err(db_err!("Failed to fetch turfs from database"))
    }

    pub async fn find_by_id(state: &SharedState, id: i32) -> Result<Option<Turf>, AppError> {
        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;
        Self::find_by_id_tx(&mut *conn, id).await
    }

    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<Turf>, AppError> {
        query_as::<_, Turf>(&format!("{} WHERE t.id = $1", Self::TURF_SELECT))
            .bind(id)
            .fetch_optional(executor)
            .await
            .map_err(db_err!("Failed to fetch turf from database (tx)"))
    }

    pub async fn find_by_manager_id(
        state: &SharedState,
        manager_id: i32,
    ) -> Result<Vec<Turf>, AppError> {
        // We acquire the connection so we can pass it to the _tx version
        let mut conn = state.db.acquire().await.map_err(db_conn_err!())?;

        // Now we just call the transactional version which holds the SQL
        Self::find_all_by_manager_id_tx(&mut *conn, manager_id).await
    }

    pub async fn find_all_by_manager_id_tx(
        executor: &mut sqlx::PgConnection,
        manager_id: i32,
    ) -> Result<Vec<Turf>, AppError> {
        query_as::<_, Turf>(&format!(
            "{} WHERE t.manager_id = $1 ORDER BY t.id ASC",
            Self::TURF_SELECT
        ))
        .bind(manager_id)
        .fetch_all(executor)
        .await
        .map_err(db_err!("Failed to fetch manager turfs from database"))
    }

    pub async fn find_by_manager_id_tx(
        executor: &mut sqlx::PgConnection,
        manager_id: i32,
    ) -> Result<Option<Turf>, AppError> {
        query_as::<_, Turf>(&format!("{} WHERE t.manager_id = $1", Self::TURF_SELECT))
            .bind(manager_id)
            .fetch_optional(executor)
            .await
            .map_err(db_err!("Failed to fetch turf by manager ID from database"))
    }

    pub async fn insert_tx(
        executor: &mut sqlx::PgConnection,
        manager_id: i32,
        name: &str,        // deref coercion
        location: &str,    // deref coercion
        description: &str, // deref coercion
    ) -> Result<Turf, AppError> {
        use crate::domain::errors::error_codes;
        // first insert turf and get id
        let turf_id = sqlx::query_scalar::<_, i32>(
            "INSERT INTO turfs (manager_id, name, location, description)
             VALUES ($1, $2, $3, $4)
             RETURNING id",
        )
        .bind(manager_id)
        .bind(name)
        .bind(location)
        .bind(description)
        .fetch_one(&mut *executor)
        .await
        .map_err(db_err!("Failed to insert turf into database"))?;

        // Re-fetch with JOIN to populate manager fields
        Self::find_by_id_tx(executor, turf_id)
            .await?
            .ok_or_else(|| AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to fetch turf after insert",
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            })
    }

    pub async fn update_tx(
        executor: &mut sqlx::PgConnection,
        turf_id: i32,
        name: &str,        // deref coercion
        location: &str,    // deref coercion
        description: &str, // deref coercion
        manager_id: i32,
    ) -> Result<Turf, AppError> {
        use crate::domain::errors::error_codes;
        sqlx::query(
            "UPDATE turfs SET name = $1, location = $2, description = $3, manager_id = $4, updated_at = NOW()
             WHERE id = $5",
        )
        .bind(name)
        .bind(location)
        .bind(description)
        .bind(manager_id)
        .bind(turf_id)
        .execute(&mut *executor)
        .await
        .map_err(db_err!("Failed to update turf in database"))?;

        // Re-fetch with JOIN to populate manager fields
        Self::find_by_id_tx(executor, turf_id)
            .await?
            .ok_or_else(|| AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Failed to fetch turf after update",
                status: axum::http::StatusCode::INTERNAL_SERVER_ERROR,
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
            .map_err(db_err!("Failed to delete turf from database"))
    }
}
