use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "PascalCase")]
pub enum SlotStatus {
    Available,
    Pending,
    Booked,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Slot {
    pub id: i32,
    pub turf_id: i32,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub status: SlotStatus,
    pub pending_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SlotCreateRequest {
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
}
