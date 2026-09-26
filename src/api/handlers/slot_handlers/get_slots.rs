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
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use crate::domain::errors::error_codes;

/// Query parameters for filtering slots by date.
/// Example: GET /games/5/slots?date=2026-04-10
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDateQuery {
    pub date: NaiveDate, // e.g. "2026-04-10"
}

impl SlotDateQuery {
    pub fn validate(&self) -> Result<(), AppError> {
        let year = self.date.year();
        if year < 2000 || year > 2100 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Query date year must be between 2000 and 2100",
            ));
        }
        Ok(())
    }
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
    params.validate()?;

    let slots = SlotService::get_slots_for_game_on_date(&state, game_id, params.date).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Slots fetched successfully".to_string(),
            data: slots,
        }),
    ))
}
