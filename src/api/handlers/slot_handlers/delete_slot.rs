use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::slot_service::SlotService;
use axum::{Json, extract::{Path, State}, http::StatusCode, response::IntoResponse};

/// DELETE /slots/:slot_id
///
/// Deletes a single Available slot.
/// Pending and Booked slots are financial records and can never be deleted via this endpoint.
/// Only the turf's manager or an Admin may delete slots.
pub async fn delete_slot_handler(
    State(state): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(slot_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    SlotService::delete_slot(
        &state,
        slot_id,
        claim.sub,
        claim.role,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Slot deleted successfully".to_string(),
            data: (),
        }),
    ))
}
