use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::Role;
use crate::services::booking_service::BookingService;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

pub async fn get_game_bookings_handler(
    State(database): State<SharedState>,
    Path(game_id): Path<i32>,
    claim: Claims,
) -> Result<impl IntoResponse, AppError> {
    let caller_id = claim.sub;
    let role = &claim.role;

    // Only managers and admins can view game bookings
    // Regular users cannot access this endpoint
    match role {
        Role::User => {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "Regular users cannot view game bookings",
                status: StatusCode::FORBIDDEN,
            });
        }
        _ => {}
    }

    let is_manager = matches!(role, Role::Manager);
    let booking_details =
        BookingService::get_game_bookings(&database, game_id, caller_id, is_manager).await?;

    Ok(Json(SuccessResponse {
        message: "Game bookings fetched successfully".to_string(),
        data: booking_details,
    }))
}
