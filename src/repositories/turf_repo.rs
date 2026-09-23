use crate::{db_conn_err, db_err};
use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::domain::models::turfs::Turf;

pub struct TurfRepository;

impl TurfRepository {
    // The TURF_SELECT constant has been removed. Each function now contains its own complete
    // SQL string literal so that sqlx::query_as! can verify the query at compile time.
    // query_as! requires a string literal — runtime format!() strings are incompatible.
    // The INNER JOIN is changed to LEFT JOIN so that manager_name/email/phone are correctly
    // inferred as nullable (Option<String>) by the macro, matching the Turf struct fields.

    pub async fn find_all(state: &SharedState) -> Result<Vec<Turf>, AppError> {
        sqlx::query_as::<sqlx::Postgres, Turf>(
            r#"SELECT t.id, t.manager_id, t.name, t.location, t.description,
                    t.created_at, t.updated_at,
                    t.google_maps_url, t.latitude, t.longitude,
                    u.username AS manager_name, u.email AS manager_email, u.phone AS manager_phone,
                    COALESCE(ARRAY_AGG(DISTINCT g.name) FILTER (WHERE g.name IS NOT NULL), ARRAY[]::text[]) AS sport_types
             FROM turfs t
             LEFT JOIN users u ON t.manager_id = u.id
             LEFT JOIN games g ON g.turf_id = t.id
             GROUP BY t.id, u.username, u.email, u.phone
             ORDER BY t.id ASC"#,
        )
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
        sqlx::query_as::<sqlx::Postgres, Turf>(
            r#"SELECT t.id, t.manager_id, t.name, t.location, t.description,
                    t.created_at, t.updated_at,
                    t.google_maps_url, t.latitude, t.longitude,
                    u.username AS manager_name, u.email AS manager_email, u.phone AS manager_phone,
                    COALESCE(ARRAY_AGG(DISTINCT g.name) FILTER (WHERE g.name IS NOT NULL), ARRAY[]::text[]) AS sport_types
             FROM turfs t
             LEFT JOIN users u ON t.manager_id = u.id
             LEFT JOIN games g ON g.turf_id = t.id
             WHERE t.id = $1
             GROUP BY t.id, u.username, u.email, u.phone"#,
        )
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
        sqlx::query_as::<sqlx::Postgres, Turf>(
            r#"SELECT t.id, t.manager_id, t.name, t.location, t.description,
                    t.created_at, t.updated_at,
                    t.google_maps_url, t.latitude, t.longitude,
                    u.username AS manager_name, u.email AS manager_email, u.phone AS manager_phone,
                    COALESCE(ARRAY_AGG(DISTINCT g.name) FILTER (WHERE g.name IS NOT NULL), ARRAY[]::text[]) AS sport_types
             FROM turfs t
             LEFT JOIN users u ON t.manager_id = u.id
             LEFT JOIN games g ON g.turf_id = t.id
             WHERE t.manager_id = $1
             GROUP BY t.id, u.username, u.email, u.phone
             ORDER BY t.id ASC"#,
        )
        .bind(manager_id)
        .fetch_all(executor)
        .await
        .map_err(db_err!("Failed to fetch manager turfs from database"))
    }

    pub async fn find_by_manager_id_tx(
        executor: &mut sqlx::PgConnection,
        manager_id: i32,
    ) -> Result<Option<Turf>, AppError> {
        sqlx::query_as::<sqlx::Postgres, Turf>(
            r#"SELECT t.id, t.manager_id, t.name, t.location, t.description,
                    t.created_at, t.updated_at,
                    t.google_maps_url, t.latitude, t.longitude,
                    u.username AS manager_name, u.email AS manager_email, u.phone AS manager_phone,
                    COALESCE(ARRAY_AGG(DISTINCT g.name) FILTER (WHERE g.name IS NOT NULL), ARRAY[]::text[]) AS sport_types
             FROM turfs t
             LEFT JOIN users u ON t.manager_id = u.id
             LEFT JOIN games g ON g.turf_id = t.id
             WHERE t.manager_id = $1
             GROUP BY t.id, u.username, u.email, u.phone"#,
        )
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
        google_maps_url: Option<&str>,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> Result<Turf, AppError> {
        use crate::domain::errors::error_codes;
        // First insert the turf and get its ID back via query_scalar!
        let turf_id = sqlx::query_scalar!(
            "INSERT INTO turfs (manager_id, name, location, description, google_maps_url, latitude, longitude)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id",
            manager_id,
            name,
            location,
            description,
            google_maps_url,
            latitude,
            longitude,
        )
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
        google_maps_url: Option<&str>,
        latitude: Option<f64>,
        longitude: Option<f64>,
    ) -> Result<Turf, AppError> {
        use crate::domain::errors::error_codes;
        sqlx::query!(
            "UPDATE turfs SET name = $1, location = $2, description = $3, manager_id = $4,
                              google_maps_url = $5, latitude = $6, longitude = $7,
                              updated_at = NOW()
             WHERE id = $8",
            name,
            location,
            description,
            manager_id,
            google_maps_url,
            latitude,
            longitude,
            turf_id,
        )
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
        sqlx::query!("DELETE FROM turfs WHERE id = $1", id)
            .execute(executor)
            .await
            .map_err(db_err!("Failed to delete turf from database"))
    }
}
