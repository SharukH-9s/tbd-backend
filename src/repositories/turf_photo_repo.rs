use crate::config::state::SharedState;
use crate::db_err;
use crate::domain::errors::AppError;
use crate::domain::models::turf_photos::{TurfPhotoMetadata, TurfPhotoRaw};

pub struct TurfPhotoRepository;

impl TurfPhotoRepository {
    /// Fetch all photo metadata for a turf (without heavy binary data)
    pub async fn find_by_turf_id(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<TurfPhotoMetadata>, AppError> {
        sqlx::query_as::<_, TurfPhotoMetadata>(
            "SELECT id, turf_id, game_id, category, filename, mime_type, file_size, created_at
             FROM turf_photos
             WHERE turf_id = $1
             ORDER BY created_at ASC",
        )
        .bind(turf_id)
        .fetch_all(&state.db)
        .await
        .map_err(db_err!("Failed to fetch turf photos from database"))
    }

    /// Fetch single photo metadata by id
    pub async fn find_by_id(
        state: &SharedState,
        photo_id: i32,
    ) -> Result<Option<TurfPhotoMetadata>, AppError> {
        sqlx::query_as::<_, TurfPhotoMetadata>(
            "SELECT id, turf_id, game_id, category, filename, mime_type, file_size, created_at
             FROM turf_photos
             WHERE id = $1",
        )
        .bind(photo_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err!("Failed to fetch photo metadata from database"))
    }

    /// Fetch raw binary image data for streaming to client
    pub async fn find_raw_by_id(
        state: &SharedState,
        photo_id: i32,
    ) -> Result<Option<TurfPhotoRaw>, AppError> {
        sqlx::query_as::<_, TurfPhotoRaw>(
            "SELECT id, mime_type, photo_data
             FROM turf_photos
             WHERE id = $1",
        )
        .bind(photo_id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err!("Failed to fetch raw photo from database"))
    }

    /// Count photos in a specific category for a turf (enforce <= 6 limit)
    pub async fn count_by_turf_and_category(
        state: &SharedState,
        turf_id: i32,
        category: &str,
    ) -> Result<i64, AppError> {
        let count: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM turf_photos WHERE turf_id = $1 AND category = $2",
        )
        .bind(turf_id)
        .bind(category)
        .fetch_one(&state.db)
        .await
        .map_err(db_err!("Failed to count photos for category"))?;

        Ok(count.0)
    }

    /// Insert new photo with binary data into PostgreSQL BYTEA column
    pub async fn insert(
        state: &SharedState,
        turf_id: i32,
        game_id: Option<i32>,
        category: &str,
        filename: &str,
        mime_type: &str,
        file_size: i32,
        photo_data: &[u8],
    ) -> Result<TurfPhotoMetadata, AppError> {
        sqlx::query_as::<_, TurfPhotoMetadata>(
            "INSERT INTO turf_photos (turf_id, game_id, category, filename, mime_type, file_size, photo_data)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, turf_id, game_id, category, filename, mime_type, file_size, created_at",
        )
        .bind(turf_id)
        .bind(game_id)
        .bind(category)
        .bind(filename)
        .bind(mime_type)
        .bind(file_size)
        .bind(photo_data)
        .fetch_one(&state.db)
        .await
        .map_err(db_err!("Failed to insert turf photo into database"))
    }

    /// Delete photo by id scoped to turf_id
    pub async fn delete_by_id(
        state: &SharedState,
        photo_id: i32,
        turf_id: i32,
    ) -> Result<bool, AppError> {
        let result = sqlx::query(
            "DELETE FROM turf_photos WHERE id = $1 AND turf_id = $2",
        )
        .bind(photo_id)
        .bind(turf_id)
        .execute(&state.db)
        .await
        .map_err(db_err!("Failed to delete turf photo from database"))?;

        Ok(result.rows_affected() > 0)
    }
}
