use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::booking_service::BookingService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/// POST /slots/:id/pay/full
/// Mock payment gateway callback that simulates a successful 100% full payment.
pub async fn pay_full_handler(
    State(state): State<SharedState>,
    claim: Claims,
    Path(slot_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let booking = BookingService::pay_full(&state, slot_id, claim.sub).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Full payment successful! Slot is now completely Booked.".to_string(),
            data: booking,
        }),
    ))
}
