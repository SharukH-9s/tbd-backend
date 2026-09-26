use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use regex::Regex;
use crate::domain::errors::{error_codes, AppError};

static USERNAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9_]+$").unwrap()
});
static BD_PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\+88)?01[3-9]\d{8}$").unwrap()
});

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type)] // sqlx::Type is used to map the enum to the database type automatically
#[sqlx(type_name = "text", rename_all = "PascalCase")] // enum variants will be stored as text in the database, and the variants will be renamed to PascalCase (Admin, User)
pub enum Role {
    Admin,
    Manager,
    User,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)] // sqlx::FromRow is used to map the database row to the User struct automatically
pub struct User {
    // Core Auth Info (Required)
    pub id: i32,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub phone: String,
    pub role: Role,
    // Profile Info (Optional - added later)
    pub location: Option<String>,
    // Metadata (Professional Standard)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// DTO (Data Transfer Object) to send to the frontend
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub phone: String,
    pub role: Role,
    pub location: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// to convert a User struct to a UserResponse struct
impl From<&User> for UserResponse {
    /*pub trait From<T> {
              fn from(value: T) -> Self;
    } */
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
            phone: user.phone.clone(),
            role: user.role.clone(),
            location: user.location.clone(),
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateProfileRequest {
    pub username: Option<String>,
    pub password: Option<String>,
    pub phone: Option<String>,
    pub location: Option<String>,
}

impl UpdateProfileRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        if let Some(uname) = &self.username {
            let uname = uname.trim();
            if uname.len() < 3 || uname.len() > 30 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Username must be between 3 and 30 characters",
                ));
            }
            if !USERNAME_RE.is_match(uname) {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Username may only contain letters, numbers, and underscores",
                ));
            }
        }

        if let Some(pass) = &self.password {
            if pass.trim().is_empty() {
                // empty string treated as no-op (existing behaviour), skip
            } else if pass.len() < 6 || pass.len() > 128 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Password must be between 6 and 128 characters",
                ));
            }
        }

        if let Some(phone) = &self.phone {
            if !BD_PHONE_RE.is_match(phone.trim()) {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Phone must be a valid Bangladeshi mobile number (e.g. 01712345678 or +8801712345678)",
                ));
            }
        }

        if let Some(loc) = &self.location {
            if loc.len() > 100 {
                return Err(AppError::bad_request(
                    error_codes::INVALID_INPUT,
                    "Location must be at most 100 characters",
                ));
            }
        }

        Ok(())
    }
}
