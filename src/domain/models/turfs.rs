use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Turf {
    pub id: i32,
    pub manager_id: i32,
    pub name: String,
    pub location: String,
    pub description: Option<String>,
    pub price_per_hour: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTurfRequest {
    pub name: String,
    pub location: String,
    pub description: Option<String>,
    pub price_per_hour: f64,
}
