use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::booking_service::BookingService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/// POST /slots/:id/pay/advance
/// Mock payment gateway callback that simulates a successful 30% advance payment.
pub async fn pay_advance_handler(
    State(state): State<SharedState>,
    claim: Claims,
    Path(slot_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let booking = BookingService::pay_advance(&state, slot_id, claim.sub).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Advance payment successful! Slot is now Booked (Partially Paid).".to_string(),
            data: booking,
        }),
    ))
}
