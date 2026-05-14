use crate::domain::errors::{AppError, error_codes};
use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
    response::{IntoResponse, Response},
};

use super::types::Claims;
use crate::config::state::SharedState;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};

// the Extractor Implementation
#[axum::async_trait]
impl<S> FromRequestParts<S> for Claims
// S is the entire AppState
where
    S: Send + Sync, // S state must be safe to share between different CPU threads.
    SharedState: FromRef<S>, // We need the SharedState from the AppState...so we define a way to get the SharedState out of whole AppState which is S
{
    type Rejection = Response; // If extraction fails, return this response

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let state = crate::config::state::SharedState::from_ref(_state);

        // Get the Authorization header
        let auth_header = parts.headers.get("Authorization");
        let auth_header = match auth_header {
            Some(value) => match value.to_str() {
                Ok(value) => value,
                Err(_) => {
                    return Err(AppError {
                        code: error_codes::MISSING_TOKEN,
                        message: "Missing authorization header",
                        status: StatusCode::BAD_REQUEST,
                    }
                    .into_response());
                }
            },
            None => {
                return Err(AppError {
                    code: error_codes::MISSING_TOKEN,
                    message: "Missing authorization header",
                    status: StatusCode::BAD_REQUEST,
                }
                .into_response());
            }
        };

        // Check if it's a Bearer token and cut the prefix
        if !auth_header.starts_with("Bearer ") {
            return Err(AppError {
                code: error_codes::INVALID_TOKEN,
                message: "Invalid token",
                status: StatusCode::UNAUTHORIZED,
            }
            .into_response());
        }
        let token = &auth_header[7..];

        let secret = &state.jwt_secret;

        let token_data = match decode(
            &token,
            &DecodingKey::from_secret(secret.as_ref()),
            &Validation::default(),
        ) {
            Ok(token_data) => token_data,
            Err(e) => {
                if e.kind() == &jsonwebtoken::errors::ErrorKind::ExpiredSignature {
                    return Err(AppError {
                        code: error_codes::EXPIRED_TOKEN,
                        message: "Token has expired",
                        status: StatusCode::UNAUTHORIZED,
                    }
                    .into_response());
                } else {
                    return Err(AppError {
                        code: error_codes::JWT_DECODE_FAILED,
                        message: "Failed to decode the JWT!",
                        status: StatusCode::UNAUTHORIZED,
                    }
                    .into_response());
                }
            }
        };
        Ok(token_data.claims)
    }
}

/// The lifetime of an access token in seconds (15 minutes).
pub const ACCESS_TOKEN_LIFETIME_SECS: i64 = 900;

pub fn make_access_token(
    user_id: i32,
    role: crate::domain::models::users::Role,
    secret: &[u8],
) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = Claims {
        sub: user_id,
        role,
        iat: now.timestamp(),
        exp: (now + Duration::minutes(15)).timestamp(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret),
    )
    .map_err(|_| AppError {
        code: error_codes::JWT_ENCODE_FAIL,
        message: "Couldn't encode the JWT!",
        status: axum::http::StatusCode::SERVICE_UNAVAILABLE,
    })
}
