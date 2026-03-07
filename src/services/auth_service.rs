use crate::api::extractors::auth::{ACCESS_TOKEN_LIFETIME_SECS, make_access_token};
use crate::api::extractors::types::LoginResponse;
use crate::api::handlers::login::LoginReq;
use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::repositories::user_repo::UserRepository;
use axum::http::StatusCode;
use uuid::Uuid;

pub struct AuthService;

// upon login, both access and refresh tokens are created.
impl AuthService {
    pub async fn authenticate(
        database: &SharedState,
        payload: &LoginReq,
    ) -> Result<(LoginResponse, String), AppError> {
        let user = match UserRepository::find_by_email(database, &payload.email).await {
            Some(u) => u,
            None => {
                return Err(AppError {
                    code: error_codes::INVALID_CREDENTIALS,
                    message: "Invalid email or password",
                    status: StatusCode::UNAUTHORIZED,
                });
            }
        };

        match bcrypt::verify(&payload.password, &user.password_hash) {
            Ok(true) => {}
            Ok(false) => {
                return Err(AppError {
                    code: error_codes::INVALID_CREDENTIALS,
                    message: "Invalid email or password",
                    status: StatusCode::UNAUTHORIZED,
                });
            }
            Err(_) => {
                return Err(AppError {
                    code: error_codes::UN_HASH_FAILED,
                    message: "Couldnt unhash the password!",
                    status: StatusCode::SERVICE_UNAVAILABLE,
                });
            }
        };

        let user_id = user.id;
        let access_token = make_access_token(user_id, &database.jwt_secret)?;

        let refresh_token = Uuid::new_v4().to_string();

        // Delete all existing tokens for this user before issuing a new one.
        // This ensures only one active session at a time.
        UserRepository::delete_tokens_for_user(database, user_id).await;
        UserRepository::store_refresh_token(database, refresh_token.clone(), user_id).await;

        let response = LoginResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_LIFETIME_SECS,
            user_id,
        };

        Ok((response, refresh_token))
    }

    //upon refresh, old refresh token is deleted and new access token and refresh token are created.
    pub async fn refresh_access_token(
        database: &SharedState,
        old_token: &str,
    ) -> Result<(LoginResponse, String), AppError> {
        let user_id = UserRepository::swap_refresh_token(database, old_token).await?;

        let new_refresh_token = Uuid::new_v4().to_string();
        UserRepository::store_refresh_token(database, new_refresh_token.clone(), user_id).await;

        let new_access_token = make_access_token(user_id, &database.jwt_secret)?;

        let response = LoginResponse {
            access_token: new_access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_LIFETIME_SECS,
            user_id,
        };

        Ok((response, new_refresh_token))
    }

    pub async fn remove_refresh_token(database: &SharedState, token: &str) {
        UserRepository::remove_refresh_token(database, token).await;
    }
}
