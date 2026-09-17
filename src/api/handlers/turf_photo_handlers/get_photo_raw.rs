use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::services::turf_photo_service::TurfPhotoService;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};

pub async fn get_photo_raw_handler(
    State(database): State<SharedState>,
    Path(photo_id): Path<i32>,
) -> Result<Response, AppError> {
    let photo = TurfPhotoService::get_raw_photo(&database, photo_id).await?;

    let mut headers = HeaderMap::new();
    if let Ok(mime) = HeaderValue::from_str(&photo.mime_type) {
        headers.insert(header::CONTENT_TYPE, mime);
    } else {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"));
    }

    // Cache immutable for 1 year so browsers cache aggressively
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    Ok((StatusCode::OK, headers, Body::from(photo.photo_data)).into_response())
}
