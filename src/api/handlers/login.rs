use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::auth_service::AuthService;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Deserialize;
use time;

#[derive(Deserialize)]
pub struct LoginReq {
    pub email: String,
    pub password: String,
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
        .secure(is_production) // Must be true for SameSite::None (Production/HTTPS), false for HTTP (Local Dev)
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
