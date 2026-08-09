use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ---------------------------------------------------------------------------
// DB Entity
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "PascalCase")]
#[serde(rename_all = "PascalCase")]
pub enum SlotStatus {
    Available,
    Pending,
    Booked,
}

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
    pub status: SlotStatus, // 'Available' | 'Pending' | 'Booked'
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
    pub initial_status: Option<SlotStatus>, // None → "Available" (default). "Booked" for offline.
    pub contact_name: Option<String>, // required if initial_status == "Booked"
    pub contact_phone: Option<String>, // required if initial_status == "Booked"
}

/// Used by the manager to create a fully-custom slot with explicit times, price, and label.
/// Nothing is inherited from any settings template.
/// Times are sent as local Dhaka times — the backend resolves UTC, just like normal slots.
#[derive(Debug, Deserialize)]
pub struct CreateCustomSlotRequest {
    pub label: Option<String>,           // optional display label
    pub price: BigDecimal,               // explicit price, always required
    pub slot_date: chrono::NaiveDate,    // the business day (same as normal slot flow)
    pub start_time: NaiveTime,           // local Dhaka time (HH:MM)
    pub end_time: NaiveTime,             // local Dhaka time (HH:MM)
    pub initial_status: Option<SlotStatus>,
    pub contact_name: Option<String>,
    pub contact_phone: Option<String>,
}
