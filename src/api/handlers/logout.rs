use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::services::auth_service::AuthService;

use axum::http::StatusCode;
use axum::{Json, extract::State, http::request::Parts, response::IntoResponse};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use time;

// Removes the refresh token from the database, clears the cookie from the browser,
// and blacklists the access token's jti in Redis for the remainder of its lifetime.
pub async fn logout_handler(
    State(database): State<SharedState>,
    parts: Parts,
    jar: CookieJar,
) -> impl IntoResponse {
    // 1. Attempt to extract and blacklist the access token if present and still valid.
    // We decode manually here (without the Claims extractor) because we want logout
    // to succeed even if the token is already expired or missing — the cookie must
    // always be cleared regardless.
    if let Some(auth_header) = parts.headers.get("Authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if let Some(token) = auth_str.strip_prefix("Bearer ") {
                let secret = &database.jwt_secret;
                if let Ok(token_data) = jsonwebtoken::decode::<crate::api::extractors::types::Claims>(
                    token,
                    &jsonwebtoken::DecodingKey::from_secret(secret.as_ref()),
                    &jsonwebtoken::Validation::default(),
                ) {
                    let claims = token_data.claims;
                    let now = chrono::Utc::now().timestamp();
                    let remaining_ttl = claims.exp - now;

                    if remaining_ttl > 0 {
                        // if the remaining ttl is less than 0, the token is already expired, so we don't need to blacklist it.
                        let blacklist_key = format!("blacklist:jti:{}", claims.jti);

                        match database.redis_pool.get().await {
                            Ok(mut redis_conn) => {
                                // SET blacklist:jti:<jti> "1" EX <ttl>
                                // The key auto-expires when the token would have expired naturally.
                                let result: Result<(), redis::RedisError> = redis::cmd("SET")
                                    .arg(&blacklist_key) // key name
                                    .arg("1") // value
                                    .arg("EX") // expire
                                    .arg(remaining_ttl) // ttl
                                    .query_async(&mut redis_conn)
                                    .await;

                                match result {
                                    Ok(_) => {
                                        tracing::info!(jti = %claims.jti, ttl = remaining_ttl, "Access token blacklisted in Redis");
                                    }
                                    Err(e) => {
                                        // Fail-Open: log the error but continue with logout.
                                        // The refresh token is still revoked, so the session is effectively ended.
                                        tracing::error!(error = %e, jti = %claims.jti, "Failed to blacklist access token in Redis — continuing logout");
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!(error = %e, jti = %claims.jti, "Failed to get Redis connection from pool for blacklisting — continuing logout");
                            }
                        }
                    }
                }
            }
        }
    }

    // 2. Remove the refresh token from the database
    if let Some(cookie) = jar.get("refresh_token") {
        if let Ok(_) = AuthService::remove_refresh_token(&database, cookie.value()).await {
            tracing::debug!("Refresh token removed from database on logout");
        }
    }

    // 3. Clear the cookie in the browser by setting it with max-age=0
    let is_production = std::env::var("PORT").is_ok();

    let dead_cookie = Cookie::build(("refresh_token", ""))
        .path("/") // must match the path the cookie was set on
        .http_only(true)
        .same_site(if is_production {
            SameSite::None
        } else {
            SameSite::Lax
        })
        .secure(is_production)
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
