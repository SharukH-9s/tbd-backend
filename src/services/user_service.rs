use crate::api::handlers::register::RegisterRequest;
use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::{Role, UserResponse};
use crate::repositories::user_repo::UserRepository;
use bcrypt::{DEFAULT_COST, hash};

pub struct UserService;

impl UserService {
    #[tracing::instrument(skip(database, payload), fields(email = %payload.email), name = "user.register")]
    pub async fn register_user(
        database: &SharedState,
        payload: &RegisterRequest,
    ) -> Result<UserResponse, AppError> {
        // Validate all fields first (username, email, password, phone)
        payload.validate()?;

        let hashed_pass = hash(&payload.password, DEFAULT_COST).map_err(|_| AppError::internal(error_codes::HASH_FAILED, "Failed to hash password"))?; // hash returns Result<String, BcryptError>, so we use map_err to convert the error to AppError and return it using ? operator 

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
        tracing::info!(user_id = new_user.id, email = %new_user.email, "New user registered successfully");

        // return the new user as a UserResponse.
        Ok(UserResponse::from(&new_user))
    }

    #[tracing::instrument(skip(database, payload), fields(target_user_id, caller_id), name = "user.update_profile")]
    pub async fn update_profile(
        database: &SharedState,
        target_user_id: i32,
        caller_id: i32,
        caller_role: Role,
        payload: crate::domain::models::users::UpdateProfileRequest,
    ) -> Result<UserResponse, AppError> {
        let mut conn = database.db.acquire().await.map_err(|e| {
            tracing::error!(error = %e, "Failed to acquire DB connection for profile update");
            AppError::internal(error_codes::DATABASE_ERROR, "Internal server error")
        })?;

        // 1. Authorization
        if caller_id != target_user_id && !matches!(caller_role, Role::Admin) {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "You do not have permission to update this profile"));
        }

        // 1a. Validate the incoming fields before any DB work
        payload.validate()?;

        // 2. Fetch existing user
        let existing_user = match UserRepository::find_by_id_tx(&mut *conn, target_user_id).await? {
            Some(u) => u,
            None => return Err(AppError::not_found(error_codes::USER_NOT_FOUND, "User not found")),
        };

        // 3. Merge fields (validate() already checked bounds; just merge with existing)
        let final_username = match payload.username {
            Some(uname) => uname,
            None => existing_user.username,
        };

        let final_password_hash = match payload.password {
            Some(pass) if !pass.trim().is_empty() => {
                hash(&pass, DEFAULT_COST).map_err(|_| AppError::internal(error_codes::HASH_FAILED, "Failed to hash password"))?
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
