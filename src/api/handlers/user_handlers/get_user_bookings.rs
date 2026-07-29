use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::booking_service::BookingService;
use axum::Json;
use axum::extract::State;
use axum::response::IntoResponse;
use bigdecimal::BigDecimal;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct BookingDetail {
    pub id: i32,
    pub slot_id: i32,
    pub booking_number: i32,
    pub user_name: Option<String>,
    pub user_phone: Option<String>,
    pub turf_name: String,
    pub game_name: String,
    pub total_price: BigDecimal,
    pub advance_paid: BigDecimal,
    pub due_amount: BigDecimal,
    pub payment_status: String,
    pub booking_time: chrono::DateTime<chrono::Utc>,
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
}

pub async fn get_user_bookings_handler(
    State(database): State<SharedState>,
    axum::extract::Path(target_user_id): axum::extract::Path<i32>,
    axum::Extension(claim): axum::Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let caller_id = claim.sub;
    let caller_role = &claim.role;

    // Authorization: A user can see only their own bookings,
    // unless they are an Admin.
    if target_user_id != caller_id
        && !matches!(caller_role, crate::domain::models::users::Role::Admin)
    {
        return Err(AppError {
            code: crate::domain::errors::error_codes::FORBIDDEN,
            message: "You can only view your own bookings",
            status: axum::http::StatusCode::FORBIDDEN,
        });
    }

    let booking_details = BookingService::get_user_bookings(&database, target_user_id).await?;

    Ok(Json(SuccessResponse {
        message: "Bookings fetched successfully".to_string(),
        data: booking_details,
    }))
}
