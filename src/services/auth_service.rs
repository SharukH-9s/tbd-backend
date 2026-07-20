use crate::api::extractors::auth::{ACCESS_TOKEN_LIFETIME_SECS, make_access_token};
use crate::api::extractors::types::LoginResponse;
use crate::api::handlers::login::LoginReq;
use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::repositories::user_repo::UserRepository;
use uuid::Uuid;

pub struct AuthService;

// upon login, both access and refresh tokens are created.
impl AuthService {
    #[tracing::instrument(skip(database, payload), fields(email = %payload.email), name = "auth.authenticate")]
    pub async fn authenticate(
        database: &SharedState,
        payload: &LoginReq,
    ) -> Result<(LoginResponse, String), AppError> {
        let user = match UserRepository::find_by_email(database, &payload.email).await? {
            Some(u) => u,
            None => return Err(AppError::unauthorized(error_codes::INVALID_CREDENTIALS, "Invalid email or password")),
        };

        match bcrypt::verify(&payload.password, &user.password_hash) {
            Ok(true) => {}
            Ok(false) => return Err(AppError::unauthorized(error_codes::INVALID_CREDENTIALS, "Invalid email or password")),
            Err(_) => return Err(AppError::service_unavailable(error_codes::UN_HASH_FAILED, "Couldnt unhash the password!")),
        };

        let user_id = user.id;
        let access_token = make_access_token(user_id, user.role.clone(), &database.jwt_secret)?;

        let refresh_token = Uuid::new_v4().to_string();

        // Delete all existing tokens for this user before issuing a new one.
        // This ensures only one active session at a time.
        UserRepository::delete_tokens_for_user(database, user_id).await?;
        tracing::debug!(user_id, "Previous session tokens revoked");
        UserRepository::store_refresh_token(database, refresh_token.clone(), user_id).await?;
        tracing::debug!(user_id, "New refresh token stored");

        let response = LoginResponse {
            access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_LIFETIME_SECS,
            user_id,
        };

        Ok((response, refresh_token))
    }

    //upon refresh, old refresh token is deleted and new access token and refresh token are created.
    #[tracing::instrument(skip(database, old_token), name = "auth.refresh_access_token")]
    pub async fn refresh_access_token(
        database: &SharedState,
        old_token: &str,
    ) -> Result<(LoginResponse, String), AppError> {
        let (user_id, role) = UserRepository::swap_refresh_token(database, old_token).await?;

        let new_refresh_token = Uuid::new_v4().to_string();
        UserRepository::store_refresh_token(database, new_refresh_token.clone(), user_id).await?;

        let new_access_token = make_access_token(user_id, role, &database.jwt_secret)?;

        let response = LoginResponse {
            access_token: new_access_token,
            token_type: "Bearer".to_string(),
            expires_in: ACCESS_TOKEN_LIFETIME_SECS,
            user_id,
        };

        Ok((response, new_refresh_token))
    }

    pub async fn remove_refresh_token(
        database: &SharedState,
        token: &str,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        UserRepository::remove_refresh_token(database, token).await
    }
}
