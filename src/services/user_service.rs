use crate::api::handlers::register::RegisterRequest;
use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::{Role, UserResponse};
use crate::repositories::user_repo::UserRepository;
use axum::http::StatusCode;
use bcrypt::{DEFAULT_COST, hash};

pub struct UserService;

impl UserService {
    pub async fn register_user(
        database: &SharedState,
        payload: &RegisterRequest,
    ) -> Result<UserResponse, AppError> {
        // Validation: Check length of username and email first
        if payload.username.trim().len() <= 3 {
            return Err(AppError {
                code: error_codes::INVALID_USERNAME,
                message: "Username must be at least 4 characters",
                status: StatusCode::BAD_REQUEST,
            });
        }

        let hashed_pass = hash(&payload.password, DEFAULT_COST).map_err(|_| AppError {
            code: error_codes::HASH_FAILED,
            message: "Failed to hash password",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?; // hash returns Result<String, BcryptError>, so we use map_err to convert the error to AppError and return it using ? operator 

        // atomic: validates for same username/email and inserts user in one atomic operation
        // this prevents race conditions where two concurrent requests could both pass the uniqueness checks
        let user_data = (
            payload.username.clone(),
            payload.email.clone(),
            hashed_pass,
            payload.phone.clone(),
            Role::User,
        );

        let new_user = UserRepository::insert_user_if_unique(database, user_data).await?;
        println!("New user created: {:?}", new_user);

        // return the new user as a UserResponse.
        Ok(UserResponse::from(&new_user))
    }
}
