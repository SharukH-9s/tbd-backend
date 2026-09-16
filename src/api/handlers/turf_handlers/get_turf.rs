use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::turf_service::TurfService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn get_a_turf_by_id(
    State(database): State<SharedState>,
    claims: Claims,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let turf = TurfService::get_turf_by_id(&database, id, claims.sub, claims.role).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Turf fetched successfully".to_string(),
            data: turf,
        }),
    ))
}
