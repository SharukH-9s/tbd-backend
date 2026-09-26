use crate::domain::errors::{error_codes, AppError};
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Mirrors the `games` DB row.
/// Owns the operating window (start/end time). Pricing lives in `game_settings`.
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Game {
    pub id: i32,
    pub turf_id: i32,
    pub name: String,
    pub game_start_time: NaiveTime, // e.g. 08:00:00
    pub game_end_time: NaiveTime,   // e.g. 01:00:00 (may cross midnight)
    pub created_at: DateTime<Utc>,
}

/// Mirrors the `game_settings` DB row.
/// A game can have multiple named slot templates.
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct GameSettings {
    pub id: i32,
    pub game_id: i32,
    pub label: String,
    pub base_price: BigDecimal,
    pub slot_duration_mins: i32,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGameRequest {
    pub name: String,
    pub game_start_time: NaiveTime,
    pub game_end_time: NaiveTime,
}

impl CreateGameRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        let name = self.name.trim();
        if name.len() < 2 || name.len() > 50 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Game name must be between 2 and 50 characters",
            ));
        }
        if self.game_start_time == self.game_end_time {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Game start time and end time cannot be identical. 24-hour operations are not supported.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateGameRequest {
    pub name: Option<String>,
    pub game_start_time: Option<NaiveTime>,
    pub game_end_time: Option<NaiveTime>,
}

impl UpdateGameRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            let name = name.trim();
            if name.len() < 2 || name.len() > 50 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Game name must be between 2 and 50 characters",
                ));
            }
        }
        // Only check equality if BOTH times are provided in this request
        if let (Some(start), Some(end)) = (self.game_start_time, self.game_end_time) {
            if start == end {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Game start time and end time cannot be identical. 24-hour operations are not supported.",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGameSettingsRequest {
    pub label: String,
    pub base_price: BigDecimal,
    pub slot_duration_mins: i32,
}

impl CreateGameSettingsRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        let label = self.label.trim();
        if label.len() < 2 || label.len() > 50 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Label must be between 2 and 50 characters",
            ));
        }
        if self.base_price <= BigDecimal::from(0) || self.base_price > BigDecimal::from(1_000_000) {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Base price must be greater than 0 and at most 1,000,000",
            ));
        }
        if self.slot_duration_mins < 15 || self.slot_duration_mins > 360 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Slot duration must be between 15 and 360 minutes",
            ));
        }
        Ok(())
    }
}
