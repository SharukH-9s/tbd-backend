use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::LazyLock;
use regex::Regex;
use crate::domain::errors::{error_codes, AppError};

static USERNAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9_]+$").unwrap()
});
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap()
});

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Turf {
    pub id: i32,
    pub manager_id: i32, // The user managing/owning the turf
    pub name: String,
    pub location: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Joined from users table (not stored in turfs)
    pub manager_name: Option<String>,
    pub manager_email: Option<String>,
    pub manager_phone: Option<String>,

    // Google Maps location
    pub google_maps_url: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,

    // Aggregated sport types from games table
    #[sqlx(default)]
    #[serde(default)]
    pub sport_types: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct CreateTurfRequest {
    pub name: String,
    pub location: String,
    pub description: String,

    // Linked Manager Creation (Concierge Onboarding)
    pub manager_email: Option<String>,
    pub manager_password: Option<String>,
    pub manager_username: Option<String>,

    // Google Maps location (optional at creation)
    pub google_maps_url: Option<String>,
}

impl CreateTurfRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        let name = self.name.trim();
        if name.len() < 2 || name.len() > 100 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Turf name must be between 2 and 100 characters",
            ));
        }

        let location = self.location.trim();
        if location.len() < 2 || location.len() > 200 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Location must be between 2 and 200 characters",
            ));
        }

        if self.description.len() > 1000 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Description must be at most 1000 characters",
            ));
        }

        if let Some(url) = &self.google_maps_url {
            if !url.is_empty() && !url.starts_with("https://") {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Google Maps URL must start with https://",
                ));
            }
        }

        // Validate linked manager fields if any are provided
        if let Some(email) = &self.manager_email {
            let email = email.trim();
            if !EMAIL_RE.is_match(email) {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Manager email must be a valid email address",
                ));
            }
        }
        if let Some(uname) = &self.manager_username {
            let uname = uname.trim();
            if uname.len() < 3 || uname.len() > 30 || !USERNAME_RE.is_match(uname) {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Manager username must be 3-30 alphanumeric/underscore characters",
                ));
            }
        }
        if let Some(pass) = &self.manager_password {
            if pass.len() < 6 || pass.len() > 128 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Manager password must be between 6 and 128 characters",
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateTurfRequest {
    pub name: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<i32>, // Admin can use this to re-assign!

    // Google Maps location (None = no change; Some("") = clear)
    pub google_maps_url: Option<String>,
}

impl UpdateTurfRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(name) = &self.name {
            let name = name.trim();
            if name.len() < 2 || name.len() > 100 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Turf name must be between 2 and 100 characters",
                ));
            }
        }

        if let Some(location) = &self.location {
            let location = location.trim();
            if location.len() < 2 || location.len() > 200 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Location must be between 2 and 200 characters",
                ));
            }
        }

        if let Some(description) = &self.description {
            if description.len() > 1000 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Description must be at most 1000 characters",
                ));
            }
        }

        if let Some(mid) = self.manager_id {
            if mid <= 0 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "manager_id must be a positive integer",
                ));
            }
        }

        if let Some(url) = &self.google_maps_url {
            if !url.is_empty() && !url.starts_with("https://") {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Google Maps URL must start with https://",
                ));
            }
        }

        Ok(())
    }
}
