use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use crate::domain::errors::{error_codes, AppError};

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MonthlyAnalyticsFilter {
    pub year: Option<i32>,
    pub month: Option<i32>,
    pub turf_id: Option<i32>,
}

impl MonthlyAnalyticsFilter {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(year) = self.year {
            if !(2000..=2100).contains(&year) {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Year must be between 2000 and 2100",
                ));
            }
        }
        if let Some(month) = self.month {
            if !(1..=12).contains(&month) {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Month must be between 1 and 12",
                ));
            }
        }
        if let Some(turf_id) = self.turf_id {
            if turf_id <= 0 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "turf_id must be a positive integer",
                ));
            }
        }
        Ok(())
    }
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
