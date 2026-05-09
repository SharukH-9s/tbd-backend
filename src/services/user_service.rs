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

    pub async fn update_profile(
        database: &SharedState,
        target_user_id: i32,
        caller_id: i32,
        caller_role: Role,
        payload: crate::domain::models::users::UpdateProfileRequest,
    ) -> Result<UserResponse, AppError> {
        let mut conn = database.db.acquire().await.map_err(|e| {
            eprintln!("Failed to acquire connection: {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "Internal server error",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?;

        // 1. Authorization
        if caller_id != target_user_id && !matches!(caller_role, Role::Admin) {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "You do not have permission to update this profile",
                status: StatusCode::FORBIDDEN,
            });
        }

        // 2. Fetch existing user
        let existing_user = match UserRepository::find_by_id_tx(&mut *conn, target_user_id).await? {
            Some(u) => u,
            None => {
                return Err(AppError {
                    code: error_codes::USER_NOT_FOUND,
                    message: "User not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 3. Merge fields & validate
        let final_username = match payload.username {
            Some(uname) => {
                if uname.trim().len() <= 3 {
                    return Err(AppError {
                        code: error_codes::INVALID_USERNAME,
                        message: "Username must be at least 4 characters",
                        status: StatusCode::BAD_REQUEST,
                    });
                }
                uname
            }
            None => existing_user.username,
        };

        let final_password_hash = match payload.password {
            Some(pass) if !pass.trim().is_empty() => {
                hash(&pass, DEFAULT_COST).map_err(|_| AppError {
                    code: error_codes::HASH_FAILED,
                    message: "Failed to hash password",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                })?
            }
            _ => existing_user.password_hash,
        };

        let final_phone = payload.phone.unwrap_or(existing_user.phone);

        let final_location = if payload.location.is_some() {
            payload.location
        } else {
            existing_user.location
        };

        // 4. Persist
        let updated_user = UserRepository::update_profile_tx(
            &mut *conn,
            target_user_id,
            &final_username,
            &final_password_hash,
            &final_phone,
            final_location.as_deref(),
        )
        .await?;

        Ok(UserResponse::from(&updated_user))
    }
}
