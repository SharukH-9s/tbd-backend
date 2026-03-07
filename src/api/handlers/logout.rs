use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::services::auth_service::AuthService;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use time;

// this will remove the refresh token from the database and the cookie from the browser
pub async fn logout_handler(
    State(database): State<SharedState>,
    jar: CookieJar,
) -> impl IntoResponse {
    if let Some(cookie) = jar.get("refresh_token") {
        AuthService::remove_refresh_token(&database, cookie.value()).await;
    }

    let dead_cookie = Cookie::build(("refresh_token", ""))
        .path("/") // must match the path the cookie was set on
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build();

    (
        StatusCode::OK,
        jar.add(dead_cookie),
        Json(SuccessResponse {
            message: "Log out is successfull!".to_string(),
            data: (),
        }),
    )
        .into_response()
}
