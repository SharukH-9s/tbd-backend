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

    let cookie = Cookie::build(("refresh_token", refresh_token))
        .path("/")
        .http_only(true)
        .same_site(SameSite::None) // This allows the cookie to be sent even when the request originates from a different domain (Vercel).
        .secure(true) //Required by modern browsers whenever SameSite::None is used. This ensures the cookie is only sent over HTTPS (which Railway uses).
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
