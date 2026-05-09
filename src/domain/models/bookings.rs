use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
    pub payment_status: String,        // 'Unpaid' | 'Partially_Paid' | 'Fully_Paid'
    pub booking_time: DateTime<Utc>,
}
