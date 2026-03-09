use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, sqlx::Type)]
#[sqlx(type_name = "text", rename_all = "PascalCase")]
pub enum PaymentStatus {
    Unpaid,
    Paid,
}

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Booking {
    pub id: i32,
    pub slot_id: i32,
    pub user_id: i32,
    pub payment_status: PaymentStatus,
    pub booking_time: DateTime<Utc>,
}
