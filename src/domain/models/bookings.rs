use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, sqlx::Type)] // These are SQLx macros used to handle how the enum is stored in the database.
#[sqlx(type_name = "text", rename_all = "PascalCase")] // variants will be saved as raw text/string in DB and will be converted to enum in Rust
#[serde(rename_all = "PascalCase")] // variants will be converted to raw text/string when converted to JSON
pub enum PaymentStatus {
    Unpaid,
    #[sqlx(rename = "Partially_Paid")]
    #[serde(rename = "Partially_Paid")]
    PartiallyPaid,
    #[sqlx(rename = "Fully_Paid")]
    #[serde(rename = "Fully_Paid")]
    FullyPaid,
}

// ---------------------------------------------------------------------------
// DB Entity
// ---------------------------------------------------------------------------

/// Mirrors the `bookings` DB row exactly.
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Booking {
    pub id: i32,
    pub slot_id: i32,
    pub user_id: Option<i32>,          // NULL for offline/phone bookings
    pub contact_name: Option<String>,  // offline only
    pub contact_phone: Option<String>, // offline only
    pub payment_status: PaymentStatus, // 'Unpaid' | 'Partially_Paid' | 'Fully_Paid'
    pub booking_time: DateTime<Utc>,
}
