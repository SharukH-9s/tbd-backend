use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::turf_photos::{TurfPhotoRaw, TurfPhotoResponse};
use crate::domain::models::users::Role;
use crate::repositories::turf_photo_repo::TurfPhotoRepository;
use crate::repositories::turf_repo::TurfRepository;

pub const MAX_PHOTOS_PER_CATEGORY: i64 = 6;
pub const MAX_PHOTO_SIZE_BYTES: usize = 1_048_576; // 1MB

pub struct TurfPhotoService;

impl TurfPhotoService {
    /// Retrieve all photos for a turf, mapped with streaming URLs
    #[tracing::instrument(skip(state), fields(turf_id), name = "photo.get_by_turf")]
    pub async fn get_turf_photos(
        state: &SharedState,
        turf_id: i32,
    ) -> Result<Vec<TurfPhotoResponse>, AppError> {
        // Ensure turf exists
        let turf = TurfRepository::find_by_id(state, turf_id).await?;
        if turf.is_none() {
            return Err(AppError::not_found(
                error_codes::TURF_NOT_FOUND,
                "Turf not found",
            ));
        }

        let metadata_list = TurfPhotoRepository::find_by_turf_id(state, turf_id).await?;
        let responses = metadata_list
            .into_iter()
            .map(TurfPhotoResponse::from_metadata)
            .collect();

        Ok(responses)
    }

    /// Retrieve raw binary image data and MIME type for streaming
    #[tracing::instrument(skip(state), fields(photo_id), name = "photo.get_raw")]
    pub async fn get_raw_photo(
        state: &SharedState,
        photo_id: i32,
    ) -> Result<TurfPhotoRaw, AppError> {
        match TurfPhotoRepository::find_raw_by_id(state, photo_id).await? {
            Some(raw) => Ok(raw),
            None => Err(AppError::not_found(
                error_codes::PHOTO_NOT_FOUND,
                "Photo not found",
            )),
        }
    }

    /// Upload one or more photos into PostgreSQL BYTEA column
    #[tracing::instrument(
        skip(state, files),
        fields(turf_id, caller_id, role = ?caller_role, files_count = files.len()),
        name = "photo.upload"
    )]
    pub async fn upload_photos(
        state: &SharedState,
        turf_id: i32,
        caller_id: i32,
        caller_role: Role,
        game_id: Option<i32>,
        category: String,
        files: Vec<(String, String, Vec<u8>)>, // (filename, mime_type, bytes)
    ) -> Result<Vec<TurfPhotoResponse>, AppError> {
        let turf = match TurfRepository::find_by_id(state, turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
        };

        // Permissions: Admin or assigned Turf Manager
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;
        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to manage photos for this turf",
            ));
        }

        if files.is_empty() {
            return Err(AppError::bad_request(
                error_codes::NO_PHOTOS_PROVIDED,
                "At least one photo must be provided",
            ));
        }

        // Validate count limit
        let current_count =
            TurfPhotoRepository::count_by_turf_and_category(state, turf_id, &category).await?;
        if current_count + (files.len() as i64) > MAX_PHOTOS_PER_CATEGORY {
            return Err(AppError::bad_request(
                error_codes::PHOTO_LIMIT_EXCEEDED,
                "Maximum 6 photos allowed per category",
            ));
        }

        // Validate each file before insertion
        for (filename, mime_type, bytes) in &files {
            if bytes.len() > MAX_PHOTO_SIZE_BYTES {
                return Err(AppError::bad_request(
                    error_codes::PHOTO_TOO_LARGE,
                    "Each photo must be under 1MB",
                ));
            }

            let valid_mime = mime_type.starts_with("image/")
                || matches!(
                    mime_type.as_str(),
                    "image/jpeg" | "image/png" | "image/webp" | "image/jpg"
                );

            if !valid_mime {
                tracing::warn!(file = %filename, mime = %mime_type, "Invalid photo MIME type rejected");
                return Err(AppError::bad_request(
                    error_codes::INVALID_PHOTO_FORMAT,
                    "Only image files (JPEG, PNG, WebP) are allowed",
                ));
            }
        }

        let mut responses = Vec::with_capacity(files.len());
        for (filename, mime_type, bytes) in files {
            let meta = TurfPhotoRepository::insert(
                state,
                turf_id,
                game_id,
                &category,
                &filename,
                &mime_type,
                bytes.len() as i32,
                &bytes,
            )
            .await?;

            responses.push(TurfPhotoResponse::from_metadata(meta));
        }

        Ok(responses)
    }

    /// Delete a photo from PostgreSQL
    #[tracing::instrument(skip(state), fields(turf_id, photo_id, caller_id, role = ?caller_role), name = "photo.delete")]
    pub async fn delete_photo(
        state: &SharedState,
        turf_id: i32,
        photo_id: i32,
        caller_id: i32,
        caller_role: Role,
    ) -> Result<(), AppError> {
        let turf = match TurfRepository::find_by_id(state, turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
        };

        // Permissions: Admin or assigned Turf Manager
        let is_admin = matches!(caller_role, Role::Admin);
        let is_owner = turf.manager_id == caller_id;
        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to manage photos for this turf",
            ));
        }

        // Verify photo exists and belongs to this turf
        let photo = TurfPhotoRepository::find_by_id(state, photo_id).await?;
        match photo {
            Some(p) if p.turf_id == turf_id => {}
            _ => {
                return Err(AppError::not_found(
                    error_codes::PHOTO_NOT_FOUND,
                    "Photo not found on this turf",
                ));
            }
        }

        let deleted = TurfPhotoRepository::delete_by_id(state, photo_id, turf_id).await?;
        if !deleted {
            return Err(AppError::not_found(
                error_codes::PHOTO_NOT_FOUND,
                "Photo not found",
            ));
        }

        Ok(())
    }
}
