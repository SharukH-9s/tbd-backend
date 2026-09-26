use crate::domain::errors::{error_codes, AppError};
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::services::user_service::UserService;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;
use std::sync::LazyLock;
use regex::Regex;

static USERNAME_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[a-zA-Z0-9_]+$").unwrap()
});
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap()
});
static BD_PHONE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(\+88)?01[3-9]\d{8}$").unwrap()
});

// Struct for extracting the username and passowrd taken from frontend
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: String,
    pub phone: String,
}

impl RegisterRequest {
    pub fn validate(&self) -> Result<(), AppError> {
        let username = self.username.trim();
        if username.len() < 3 || username.len() > 30 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Username must be between 3 and 30 characters",
            ));
        }
        if !USERNAME_RE.is_match(username) {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Username may only contain letters, numbers, and underscores",
            ));
        }

        let email = self.email.trim();
        if email.len() > 255 || !EMAIL_RE.is_match(email) {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "A valid email address is required (max 255 characters)",
            ));
        }

        if self.password.len() < 6 || self.password.len() > 128 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Password must be between 6 and 128 characters",
            ));
        }

        if !BD_PHONE_RE.is_match(self.phone.trim()) {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Phone must be a valid Bangladeshi mobile number (e.g. 01712345678 or +8801712345678)",
            ));
        }

        Ok(())
    }
}

pub async fn register(
    State(database): State<SharedState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    // passing the payload and database to the service layer. then store the user_response in response_data
    let user = UserService::register_user(&database, &payload).await?; // if register_request returns any AppError, the `?` operator will automatically return it

    // if no error is returned, then return the response_data in Json format with status code 201
    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Registered successfully".to_string(),
            data: user,
        }),
    )) // StatusCode and Json are both IntoResponse trait objects. So, axum will automatically convert them into a response.
}
