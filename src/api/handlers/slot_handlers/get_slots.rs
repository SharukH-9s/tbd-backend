use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::slot_service::SlotService;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::NaiveDate;
use serde::Deserialize;

/// Query parameters for filtering slots by date.
/// Example: GET /games/5/slots?date=2026-04-10
#[derive(Deserialize)]
pub struct SlotDateQuery {
    pub date: NaiveDate, // e.g. "2026-04-10"
}

/// GET /games/:game_id/slots?date=YYYY-MM-DD
///
/// Returns all slots for a game on the given business day.
/// Handles midnight rollover — a game open 07:00→01:00 includes late night slots under the same date.
/// No auth required — slot calendars are publicly visible.
pub async fn get_slots_handler(
    State(state): State<SharedState>,
    Path(game_id): Path<i32>,
    Query(params): Query<SlotDateQuery>,
) -> Result<impl IntoResponse, AppError> {
    let slots = SlotService::get_slots_for_game_on_date(&state, game_id, params.date).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Slots fetched successfully".to_string(),
            data: slots,
        }),
    ))
}
