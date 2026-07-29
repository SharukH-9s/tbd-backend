use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::booking_service::BookingService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/// PATCH /bookings/:id/payment
///
/// Manager-only endpoint. Marks any non-fully-paid booking as Fully_Paid.
/// This can be used to:
/// - Mark Unpaid bookings as Fully_Paid (when manager collects 100% in cash)
/// - Mark Partially_Paid bookings as Fully_Paid (when manager collects remaining 70% at the turf)
/// No payment gateway involved — this is a manual action by the manager.
pub async fn update_payment_handler(
    State(state): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(booking_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let booking =
        BookingService::update_payment(&state, booking_id, claim.sub, claim.role).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Payment marked as Fully_Paid. Booking is fully settled.".to_string(),
            data: booking,
        }),
    ))
}
