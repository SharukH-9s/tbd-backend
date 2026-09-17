use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::turf_photo_service::TurfPhotoService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn delete_photo_handler(
    State(database): State<SharedState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path((turf_id, photo_id)): Path<(i32, i32)>,
) -> Result<impl IntoResponse, AppError> {
    TurfPhotoService::delete_photo(
        &database,
        turf_id,
        photo_id,
        claims.sub,
        claims.role,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Photo deleted successfully".to_string(),
            data: (),
        }),
    ))
}
