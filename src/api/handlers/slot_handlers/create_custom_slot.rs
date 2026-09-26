use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::slots::CreateCustomSlotRequest;
use crate::services::slot_service::SlotService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/// POST /slots/custom
///
/// Creates a custom slot at explicit start_time and end_time.
/// Supply either `settings_id` (uses its base_price) or an explicit `price`.
/// Optional `label` overrides the display name shown in the UI.
/// If `initial_status` is "Booked", `contact_name` and `contact_phone` are required.
pub async fn create_custom_slot_handler(
    State(state): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(game_id): Path<i32>,
    Json(payload): Json<CreateCustomSlotRequest>,
) -> Result<impl IntoResponse, AppError> {
    payload.validate()?;

    let slot = SlotService::create_custom_slot(
        &state,
        game_id,
        payload.label,
        payload.price,
        payload.slot_date,
        payload.start_time,
        payload.end_time,
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
            message: "Custom slot created successfully".to_string(),
            data: slot,
        }),
    ))
}
