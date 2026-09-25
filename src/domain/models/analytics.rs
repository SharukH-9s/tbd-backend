use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Default)]
pub struct MonthlyAnalyticsFilter {
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub turf_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GameAnalytics {
    pub game_id: i32,
    pub game_name: String,
    pub total_bookings: i64,
    pub fully_paid_bookings: i64,
    pub income: BigDecimal,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TurfAnalytics {
    pub turf_id: i32,
    pub turf_name: String,
    pub games: Vec<GameAnalytics>,
    pub turf_total_income: BigDecimal,
    pub turf_total_bookings: i64,
    pub turf_fully_paid_bookings: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MonthlyAnalyticsResponse {
    pub year: i32,
    pub month: Option<i32>,
    pub data: Vec<TurfAnalytics>,
}
