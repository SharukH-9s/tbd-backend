use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::services::booking_service::BookingService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};
use serde_json::json;

/// DELETE /bookings/:id
///
/// Manager-only endpoint. Cancels a booking and automatically resets
/// the linked slot back to 'Available' in a single atomic transaction.
/// Cannot cancel a Fully_Paid booking.
pub async fn cancel_booking_handler(
    State(state): State<SharedState>,
    claim: Claims,
    Path(booking_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    BookingService::cancel_booking(&state, booking_id, claim.sub, claim.role).await?;

    Ok((
        StatusCode::OK,
        Json(json!({
            "message": "Booking cancelled and slot is now Available again."
        })),
    ))
}
