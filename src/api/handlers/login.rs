use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::{error_codes, AppError};
use crate::services::auth_service::AuthService;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Deserialize;
use std::sync::LazyLock;
use regex::Regex;
use time;

static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[^@\s]+@[^@\s]+\.[^@\s]+$").unwrap()
});

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
}

impl LoginReq {
    pub fn validate(&self) -> Result<(), AppError> {
        let email = self.email.trim();
        if email.is_empty() || email.len() > 255 || !EMAIL_RE.is_match(email) {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "A valid email address is required",
            ));
        }
        if self.password.is_empty() || self.password.len() > 128 {
            return Err(AppError::bad_request(
                error_codes::INVALID_INPUT,
                "Password must be between 1 and 128 characters",
            ));
        }
        Ok(())
    }
}

#[axum::debug_handler]
pub async fn login_handler(
    State(database): State<SharedState>,
    jar: CookieJar,
    Json(payload): Json<LoginReq>,
) -> Result<impl IntoResponse, AppError> {
    let (response_body, refresh_token) = AuthService::authenticate(&database, &payload).await?;

    let is_production = std::env::var("PORT").is_ok(); // Railway sets PORT in production

    let cookie = Cookie::build(("refresh_token", refresh_token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::None) // Always use None for cross-origin/cross-port requests
        .secure(is_production)
        .max_age(time::Duration::days(7))
        .build();

    Ok((
        StatusCode::OK,
        jar.add(cookie),
        Json(SuccessResponse {
            message: "Login successful".to_string(),
            data: response_body,
        }),
    ))
}
