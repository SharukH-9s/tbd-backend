use bigdecimal::BigDecimal;
use chrono::{DateTime, Datelike, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::LazyLock;
use regex::Regex;
use crate::domain::errors::{error_codes, AppError};

static BD_PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\+88)?01[3-9]\d{8}$").unwrap()
});

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
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CreateSlotRequest {
    pub settings_id: i32,
    pub slot_date: chrono::NaiveDate, // the business day to generate the slot for
    pub slot_interval_mins: Option<i32>, // gap before this slot (default: 10 mins)
    pub initial_status: Option<SlotStatus>, // None → "Available" (default). "Booked" for offline.
    pub contact_name: Option<String>, // required if initial_status == "Booked"
    pub contact_phone: Option<String>, // required if initial_status == "Booked"
}

impl CreateSlotRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if self.settings_id <= 0 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "settings_id must be a positive integer",
            ));
        }

        let year = self.slot_date.year();
        if year < 2000 || year > 2100 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "slot_date year must be between 2000 and 2100",
            ));
        }

        if let Some(interval) = self.slot_interval_mins {
            if interval < 0 || interval > 180 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "slot_interval_mins must be between 0 and 180 (up to 3 hours)",
                ));
            }
        }

        if self.initial_status == Some(SlotStatus::Booked) {
            match &self.contact_name {
                Some(name) => {
                    let trimmed = name.trim();
                    if trimmed.len() < 2 || trimmed.len() > 50 {
                        return Err(AppError::bad_request(
                            error_codes::INVALID_INPUT,
                            "contact_name must be between 2 and 50 characters",
                        ));
                    }
                }
                None => {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_name is required when initial_status is Booked",
                    ));
                }
            }

            match &self.contact_phone {
                Some(phone) => {
                    let trimmed = phone.trim();
                    if !BD_PHONE_RE.is_match(trimmed) {
                        return Err(AppError::bad_request(
                            error_codes::INVALID_INPUT,
                            "contact_phone must be a valid Bangladeshi number (e.g., 01712345678 or +8801712345678)",
                        ));
                    }
                }
                None => {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_phone is required when initial_status is Booked",
                    ));
                }
            }
        } else {
            if let Some(name) = &self.contact_name {
                let trimmed = name.trim();
                if !trimmed.is_empty() && (trimmed.len() < 2 || trimmed.len() > 50) {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_name must be between 2 and 50 characters",
                    ));
                }
            }
            if let Some(phone) = &self.contact_phone {
                let trimmed = phone.trim();
                if !trimmed.is_empty() && !BD_PHONE_RE.is_match(trimmed) {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_phone must be a valid Bangladeshi number",
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Used by the manager to create a fully-custom slot with explicit times, price, and label.
/// Nothing is inherited from any settings template.
/// Times are sent as local Dhaka times — the backend resolves UTC, just like normal slots.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
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

impl CreateCustomSlotRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(label) = &self.label {
            let trimmed = label.trim();
            if trimmed.is_empty() || trimmed.len() > 50 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "label must be between 1 and 50 characters",
                ));
            }
        }

        if self.price <= BigDecimal::from(0) || self.price > BigDecimal::from(1_000_000) {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "price must be greater than 0 and at most 1,000,000",
            ));
        }

        let year = self.slot_date.year();
        if year < 2000 || year > 2100 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "slot_date year must be between 2000 and 2100",
            ));
        }

        if self.start_time == self.end_time {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Slot start time and end time cannot be identical",
            ));
        }

        if self.initial_status == Some(SlotStatus::Booked) {
            match &self.contact_name {
                Some(name) => {
                    let trimmed = name.trim();
                    if trimmed.len() < 2 || trimmed.len() > 50 {
                        return Err(AppError::bad_request(
                            error_codes::INVALID_INPUT,
                            "contact_name must be between 2 and 50 characters",
                        ));
                    }
                }
                None => {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_name is required when initial_status is Booked",
                    ));
                }
            }

            match &self.contact_phone {
                Some(phone) => {
                    let trimmed = phone.trim();
                    if !BD_PHONE_RE.is_match(trimmed) {
                        return Err(AppError::bad_request(
                            error_codes::INVALID_INPUT,
                            "contact_phone must be a valid Bangladeshi number (e.g., 01712345678 or +8801712345678)",
                        ));
                    }
                }
                None => {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_phone is required when initial_status is Booked",
                    ));
                }
            }
        } else {
            if let Some(name) = &self.contact_name {
                let trimmed = name.trim();
                if !trimmed.is_empty() && (trimmed.len() < 2 || trimmed.len() > 50) {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_name must be between 2 and 50 characters",
                    ));
                }
            }
            if let Some(phone) = &self.contact_phone {
                let trimmed = phone.trim();
                if !trimmed.is_empty() && !BD_PHONE_RE.is_match(trimmed) {
                    return Err(AppError::bad_request(
                        error_codes::INVALID_INPUT,
                        "contact_phone must be a valid Bangladeshi number",
                    ));
                }
            }
        }

        Ok(())
    }
}
