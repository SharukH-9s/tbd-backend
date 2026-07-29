use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::slots::CreateSlotRequest;
use crate::services::slot_service::SlotService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/// POST /games/:game_id/slots
///
/// Creates the next sequential slot for a game on a given date.
/// If `initial_status` is "Booked", `contact_name` and `contact_phone` must be provided.
/// The booking record is automatically created inside the same transaction.
///  /*
/// pub struct CreateSlotRequest { // will be given by frontend
///     pub settings_id: i32,
///     pub slot_date: chrono::NaiveDate, // the business day to generate the slot for
///     pub slot_interval_mins: Option<i32>, // gap before this slot (default: 10 mins)
///     pub initial_status: Option<String>, // None → "Available" (default). "Booked" for offline.
///     pub contact_name: Option<String>, // required if initial_status == "Booked"
///     pub contact_phone: Option<String>, // required if initial_status == "Booked"
/// }
///  */
pub async fn create_slot_handler(
    State(state): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(game_id): Path<i32>,
    Json(payload): Json<CreateSlotRequest>,
) -> Result<impl IntoResponse, AppError> {
    let slot = SlotService::create_slot(
        &state,
        game_id,
        payload.settings_id,
        payload.slot_date,
        payload.slot_interval_mins,
        payload.initial_status,
        payload.contact_name,
        payload.contact_phone,
        claim.sub,
        claim.role,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Slot created successfully".to_string(),
            data: slot,
        }),
    ))
}
