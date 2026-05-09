use crate::api::extractors::types::Claims;
use crate::domain::dtos::responses::SuccessResponse;
use crate::services::turf_service::TurfService;
use crate::{config::state::SharedState, domain::errors::AppError};
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

pub async fn get_all_turfs(
    State(state): State<SharedState>,
    claims: Claims,
) -> Result<impl IntoResponse, AppError> {
    let turfs = TurfService::get_all_turfs(&state, claims.sub, claims.role).await?;
    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Turfs fetched successfully".to_string(),
            data: turfs,
        }),
    ))
}
