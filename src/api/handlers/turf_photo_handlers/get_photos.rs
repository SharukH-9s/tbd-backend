use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::turf_photo_service::TurfPhotoService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn get_photos_handler(
    State(database): State<SharedState>,
    Path(turf_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let photos = TurfPhotoService::get_turf_photos(&database, turf_id).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Turf photos fetched successfully".to_string(),
            data: photos,
        }),
    ))
}
