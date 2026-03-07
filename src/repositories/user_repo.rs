use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::{Role, User};
use axum::http::StatusCode;

pub struct UserRepository;

impl UserRepository {
    // Find a user by email address. Returns None if no user exists with that email.
    pub async fn find_by_email(database: &SharedState, email: &str) -> Option<User> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, email, password_hash, phone, role, location, created_at, updated_at
             FROM users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(&database.db) // returns Result<Option<User>, sqlx::Error>
        .await
        .ok()                         // convert Result<Option<User>, sqlx::Error> -> Option<Option<User>> (None on error)
        .flatten()                    // flatten Option<Option<User>> -> Option<User>
    }

    // Insert a new user into the database.
    // The UNIQUE constraints on `username` and `email` columns in Postgres
    // enforce uniqueness at the DB level — this handles race conditions automatically.
    pub async fn insert_user_if_unique(database: &SharedState, user_data: (String, String, String, String, Role)) -> Result<User, AppError> { // username, email, hashed_pass, phone, role
        let (username, email, password_hash, phone, role) = user_data;

        sqlx::query_as::<_, User>( // sqlx::query_as is used to map the database row to the User struct automatically
                                     "INSERT INTO users (username, email, password_hash, phone, role)
                                      VALUES ($1, $2, $3, $4, $5)
                                      RETURNING id, username, email, password_hash, phone, role, location, created_at, updated_at"
                                 )
        .bind(&username)
        .bind(&email)
        .bind(&password_hash)
        .bind(&phone)
        .bind(&role)
        .fetch_one(&database.db) // expects one row, then maps it to User struct. returns Result<User, sqlx::Error>
        .await
        .map_err(|e| { // e is sqlx::Error
            
            // Check if this is a unique constraint violation (postgres error code 23505)
            if let sqlx::Error::Database(db_err) = &e {         // & is used to borrow the db_err from e
                if db_err.code().as_deref() == Some("23505") {  // as_deref() is used to convert &sqlx::Error::Database to &str
                    let msg = db_err.message();                 // message() returns the error message as a String
                    
                    // Postgres auto-names UNIQUE constraints as {table}_{column}_key
                    if msg.contains("users_username_key") {     // contains() is used to check if the message contains the string "users_username_key"
                        return AppError {
                            code: error_codes::USERNAME_EXISTS,
                            message: "Username already exists",
                            status: StatusCode::BAD_REQUEST,
                        };
                    }
                    if msg.contains("users_email_key") {
                        return AppError {
                            code: error_codes::EMAIL_EXISTS,
                            message: "An account with this email already exists",
                            status: StatusCode::BAD_REQUEST,
                        };
                    }
                }
            }
            // Fallback for any other unexpected DB error
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "A database error occurred during registration",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })
    }

    // Delete ALL refresh tokens for a given user.
    // Called at the start of login so a user can only have one active session at a time.
    pub async fn delete_tokens_for_user(database: &SharedState, user_id: i32) {
        sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&database.db)
            .await
            .ok();
    }

    // Store a new refresh token linked to a user_id
    pub async fn store_refresh_token(database: &SharedState, token: String, user_id: i32) {

        sqlx::query("INSERT INTO refresh_tokens (token, user_id) VALUES ($1, $2)")
            .bind(&token)
            .bind(user_id)
            .execute(&database.db)
            .await
            .ok(); // silently ignore errors — token storage failure is non-fatal
    }

    // Delete the old refresh token and return the user_id it belonged to.
    // RETURNING gives us the user_id in the same query — one round trip to the DB.
    pub async fn swap_refresh_token(database: &SharedState, old_token: &str) -> Result<i32, AppError> {
        let result = sqlx::query_scalar::<_, i32>(
            "DELETE FROM refresh_tokens WHERE token = $1 RETURNING user_id"
        )
        .bind(old_token)
        .fetch_optional(&database.db) // fetch_optional returns: Result<Option<i32>, sqlx::Error>
        .await
        .map_err(|_| AppError {
            code: error_codes::DATABASE_ERROR,
            message: "A database error occurred",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        match result {
            Some(user_id) => return Ok(user_id), // ? operatior will unwrap this Ok(user_id) in the refresh_access_token function
            None => return Err(AppError {
                code: error_codes::INVALID_REFRESH_TOKEN,
                message: "Invalid or expired refresh token",
                status: StatusCode::UNAUTHORIZED,
            })
        };
    }

    // Delete the refresh token on logout
    pub async fn remove_refresh_token(database: &SharedState, token: &str) {
        sqlx::query("DELETE FROM refresh_tokens WHERE token = $1")
            .bind(token)
            .execute(&database.db)
            .await
            .ok();
    }
}
