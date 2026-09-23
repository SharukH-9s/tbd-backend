use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::booking_service::BookingService;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

/// POST /slots/:id/book
///
/// Initiates an online booking for an Available slot.
/// Locks the slot as Pending for 15 minutes and creates an Unpaid booking record.
/// The user must proceed to pay (advance or full) before the timer expires.
pub async fn initiate_booking_handler(
    State(state): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(slot_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let booking = BookingService::initiate_booking(&state, slot_id, claim.sub, &claim.role).await?;

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Slot locked for 15 minutes. Proceed to payment.".to_string(),
            data: booking,
        }),
    ))
}
