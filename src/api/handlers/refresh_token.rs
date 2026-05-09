use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::{AppError, error_codes};
use crate::services::auth_service::AuthService;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use axum_extra::extract::{CookieJar, cookie::Cookie};
use time;

pub async fn refresh_handler(
    State(database): State<SharedState>,
    jar: CookieJar,
) -> Result<impl IntoResponse, AppError> {
    let old_cookie = jar.get("refresh_token");
    let old_token = match old_cookie {
        Some(cookie) => cookie.value().to_string(),
        None => {
            return Err(AppError {
                code: error_codes::NO_REFRESH_TOKEN,
                message: "No token found in the Cookie",
                status: StatusCode::UNAUTHORIZED,
            });
        }
    };

    let (new_response_body, new_refresh_token) =
        AuthService::refresh_access_token(&database, &old_token).await?;

    let is_production = std::env::var("PORT").is_ok();

    let new_cookie = Cookie::build(("refresh_token", new_refresh_token))
        .path("/")
        .http_only(true)
        .same_site(axum_extra::extract::cookie::SameSite::None) // Always use None for cross-origin/cross-port requests
        .secure(is_production) // Must be true for SameSite::None (Production/HTTPS), false for HTTP (Local Dev)
        .max_age(time::Duration::days(7))
        .build();

    Ok((
        StatusCode::OK,
        jar.add(new_cookie),
        Json(SuccessResponse {
            message: "Token refreshed successfully".to_string(),
            data: new_response_body,
        }),
    ))
}
