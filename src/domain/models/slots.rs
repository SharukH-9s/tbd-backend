use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ---------------------------------------------------------------------------
// DB Entity
// ---------------------------------------------------------------------------

/// Mirrors the `slots` DB row.
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Slot {
    pub id: i32,
    pub game_id: i32,
    pub settings_id: Option<i32>, // None for fully-custom slots
    pub label: Option<String>,    // custom display label
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub final_price: BigDecimal,
    pub status: String, // 'Available' | 'Pending' | 'Booked'
    pub pending_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Used by the manager to create the next sequential slot for a game on a
/// specific date using a chosen settings template.
/// If `initial_status` is "Booked", `contact_name` and `contact_phone` are required.
#[derive(Debug, Deserialize)]
pub struct CreateSlotRequest {
    pub settings_id: i32,
    pub slot_date: chrono::NaiveDate, // the business day to generate the slot for
    pub slot_interval_mins: Option<i32>, // gap before this slot (default: 10 mins)
    pub initial_status: Option<String>, // None → "Available" (default). "Booked" for offline.
    pub contact_name: Option<String>, // required if initial_status == "Booked"
    pub contact_phone: Option<String>, // required if initial_status == "Booked"
}

/// Used by the manager to create a fully-custom slot with explicit times, price, and label.
/// Nothing is inherited from any settings template.
#[derive(Debug, Deserialize)]
pub struct CreateCustomSlotRequest {
    pub label: Option<String>, // optional display label
    pub price: BigDecimal,     // explicit price, always required
    pub start_time: chrono::DateTime<chrono::Utc>,
    pub end_time: chrono::DateTime<chrono::Utc>,
    pub initial_status: Option<String>,
    pub contact_name: Option<String>,
    pub contact_phone: Option<String>,
}
