use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use bigdecimal::BigDecimal;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Game {
    pub id: i32,
    pub turf_id: i32,
    pub name: String,
    pub price_per_hour: BigDecimal,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateGameRequest {
    pub name: String,
    pub price_per_hour: BigDecimal,
}

#[derive(Debug, Deserialize)]
pub struct UpdateGameRequest {
    pub name: Option<String>,
    pub price_per_hour: Option<BigDecimal>,
}
