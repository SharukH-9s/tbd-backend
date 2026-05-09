use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::turfs::UpdateTurfRequest;
use crate::services::turf_service::TurfService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/* pub struct UpdateTurfRequest {
    pub name: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<i32>, // Admin can use this to re-assign!
} */

pub async fn update_turf_handler(
    State(database): State<SharedState>,
    claim: Claims,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateTurfRequest>,
) -> Result<impl IntoResponse, AppError> {
    // 4. Perform the update
    let updated_turf = TurfService::update_turf(
        &database,
        id,
        claim.sub,
        claim.role,
        payload.name,
        payload.location,
        payload.description,
        payload.manager_id,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Turf updated successfully".to_string(),
            data: updated_turf,
        }),
    ))
}
