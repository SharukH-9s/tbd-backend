use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::turf_service::TurfService;
use axum::{Extension, Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn delete_turf_handler(
    State(database): State<SharedState>,
    Extension(_claim): Extension<Claims>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    TurfService::delete_turf(&database, id).await?;

    Ok((
        StatusCode::NO_CONTENT,
        Json(SuccessResponse {
            message: "Turf deleted successfully".to_string(),
            data: (),
        }),
    ))
}
