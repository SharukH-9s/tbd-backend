use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use bigdecimal::BigDecimal;

/// Mirrors the `games` DB row.
/// Owns the operating window (start/end time). Pricing lives in `game_settings`.
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Game {
    pub id: i32,
    pub turf_id: i32,
    pub name: String,
    pub game_start_time: NaiveTime,  // e.g. 08:00:00
    pub game_end_time: NaiveTime,    // e.g. 01:00:00 (may cross midnight)
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
pub struct CreateGameRequest {
    pub name: String,
    pub game_start_time: NaiveTime,
    pub game_end_time: NaiveTime,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGameRequest {
    pub name: Option<String>,
    pub game_start_time: Option<NaiveTime>,
    pub game_end_time: Option<NaiveTime>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGameSettingsRequest {
    pub label: String,
    pub base_price: BigDecimal,
    pub slot_duration_mins: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGameSettingsRequest {
    pub label: Option<String>,
    pub base_price: Option<BigDecimal>,
    pub slot_duration_mins: Option<i32>,
}
