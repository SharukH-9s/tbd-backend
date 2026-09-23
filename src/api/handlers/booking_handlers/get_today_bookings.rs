use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::users::Role;
use crate::services::booking_service::BookingService;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

pub async fn get_today_bookings_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let caller_id = claim.sub;
    let role = &claim.role;

    // Only managers and admins can view today's bookings
    match role {
        Role::User => {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "Regular users cannot access manager bookings",
                status: StatusCode::FORBIDDEN,
            });
        }
        _ => {}
    }

    let is_admin = matches!(role, Role::Admin);
    let bookings =
        BookingService::get_today_bookings_for_manager(&database, caller_id, is_admin).await?;

    Ok(Json(SuccessResponse {
        message: "Today's bookings fetched successfully".to_string(),
        data: bookings,
    }))
}
