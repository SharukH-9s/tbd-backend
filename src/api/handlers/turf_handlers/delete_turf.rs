use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::turf_service::TurfService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn delete_turf_handler(
    State(database): State<SharedState>,
    claim: Claims,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    TurfService::delete_turf(&database, id, claim.role).await?;

    Ok((
        StatusCode::NO_CONTENT,
        Json(SuccessResponse {
            message: "Turf deleted successfully".to_string(),
            data: (),
        }),
    ))
}
