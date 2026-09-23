use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::bookings::BookingFilter;
use crate::services::booking_service::BookingService;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Extension, Json};

/// GET /bookings
///
/// Consolidated query endpoint for fetching bookings:
/// - `?user_id=12`             --> Customer's bookings (requires self or admin)
/// - `?game_id=5`              --> Manager/Admin bookings for a specific pitch game
/// - `?today=true`             --> Manager's today schedule across their turfs (or all turfs if admin)
/// - (no params)              --> Defaults to caller's own bookings or manager's turfs
pub async fn get_bookings_handler(
    State(database): State<SharedState>,
    Query(filter): Query<BookingFilter>,
    Extension(claim): Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let bookings = BookingService::get_bookings(&database, claim.sub, &claim.role, filter).await?;

    Ok(Json(SuccessResponse {
        message: "Bookings fetched successfully".to_string(),
        data: bookings,
    }))
}
