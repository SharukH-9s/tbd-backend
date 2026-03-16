use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::{Role, User};
use axum::http::StatusCode;

pub struct UserRepository;

impl UserRepository {
    pub async fn find_by_email(
        database: &SharedState,
        email: &str,
    ) -> Result<Option<User>, AppError> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, email, password_hash, phone, role, location, created_at, updated_at
             FROM users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(&database.db) // Result<Option<User>, sqlx::Error>
        .await
        .map_err(|_| AppError {
            code: error_codes::DATABASE_ERROR,
            message: "A database error occurred while fetching user by email",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })
    }

    pub async fn find_by_email_tx(
        executor: &mut sqlx::PgConnection,
        email: &str,
    ) -> Result<Option<User>, AppError> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, email, password_hash, phone, role, location, created_at, updated_at
             FROM users WHERE email = $1"
        )
        .bind(email)
        .fetch_optional(executor) // Result<Option<User>, sqlx::Error>
        .await
        .map_err(|_| AppError {
            code: error_codes::DATABASE_ERROR,
            message: "A database error occurred while fetching user by email",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })
    }

    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<User>, AppError> {
        sqlx::query_as::<_, User>(
            "SELECT id, username, email, password_hash, phone, role, location, created_at, updated_at
             FROM users WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(executor) // Result<Option<User>, sqlx::Error>
        .await
        .map_err(|_| AppError {
            code: error_codes::DATABASE_ERROR,
            message: "A database error occurred while fetching user by id",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })
    }

    pub async fn insert_user_if_unique(
        database: &SharedState,
        user_data: (String, String, String, String, Role),
    ) -> Result<User, AppError> {
        let (username, email, password_hash, phone, role) = user_data;

        sqlx::query_as::<_, User>(
            "INSERT INTO users (username, email, password_hash, phone, role)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, username, email, password_hash, phone, role, location, created_at, updated_at"
        )
        .bind(&username)
        .bind(&email)
        .bind(&password_hash)
        .bind(&phone)
        .bind(&role)
        .fetch_one(&database.db)// Result<User, sqlx::Error>
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.code().as_deref() == Some("23505") {
                    let msg = db_err.message();
                    if msg.contains("users_username_key") {
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
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "A database error occurred during registration",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        }) // returns Result<User, AppError>
    }

    pub async fn insert_user_tx(
        executor: &mut sqlx::PgConnection,
        user_data: (String, String, String, String, Role),
    ) -> Result<User, AppError> {
        let (username, email, password_hash, phone, role) = user_data;

        sqlx::query_as::<_, User>(
            "INSERT INTO users (username, email, password_hash, phone, role)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, username, email, password_hash, phone, role, location, created_at, updated_at"
        )
        .bind(&username)
        .bind(&email)
        .bind(&password_hash)
        .bind(&phone)
        .bind(&role)
        .fetch_one(executor) // Result<User, sqlx::Error>
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(db_err) = &e {
                if db_err.code().as_deref() == Some("23505") {
                    let msg = db_err.message();
                    if msg.contains("users_username_key") {
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
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "A database error occurred during registration",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        }) // returns Result<User, AppError> 
    }

    pub async fn promote_to_manager_tx(
        executor: &mut sqlx::PgConnection,
        user_id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        Self::set_role_tx(executor, user_id, Role::Manager).await
    }

    pub async fn set_role_tx(
        executor: &mut sqlx::PgConnection,
        user_id: i32,
        role: Role,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("UPDATE users SET role = $1 WHERE id = $2")
            .bind(role)
            .bind(user_id)
            .execute(executor)
            .await
            .map_err(|e| {
                eprintln!("Database error in set_role_tx: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to update user role",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    // Delete ALL refresh tokens for a given user.
    // Called at the start of login so a user can only have one active session at a time.
    pub async fn delete_tokens_for_user(
        database: &SharedState,
        user_id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("DELETE FROM refresh_tokens WHERE user_id = $1")
            .bind(user_id)
            .execute(&database.db)
            .await
            .map_err(|e| {
                eprintln!("Database error in delete_tokens_for_user: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "A database error occurred while deleting the refresh tokens",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    // Store a new refresh token linked to a user_id
    pub async fn store_refresh_token(
        database: &SharedState,
        token: String,
        user_id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("INSERT INTO refresh_tokens (token, user_id) VALUES ($1, $2)")
            .bind(&token)
            .bind(user_id)
            .execute(&database.db) // Result<u64, sqlx::Error>
            .await
            .map_err(|e| {
                eprintln!("Database error in store_refresh_token: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "A database error occurred while storing the refresh token",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }

    // Delete the old refresh token and return the user_id and role it belonged to.
    pub async fn swap_refresh_token(
        database: &SharedState,
        old_token: &str,
    ) -> Result<(i32, Role), AppError> {
        let result = sqlx::query_as::<_, (i32, Role)>(
            "WITH deleted AS (
                DELETE FROM refresh_tokens WHERE token = $1 RETURNING user_id
             )
             SELECT u.id, u.role FROM users u
             INNER JOIN deleted d ON u.id = d.user_id",
        )
        .bind(old_token)
        .fetch_optional(&database.db) // fetch_optional returns: Result<Option<(i32, Role)>, sqlx::Error>
        .await
        .map_err(|e| {
            eprintln!("Database error in swap_refresh_token: {}", e);
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "A database error occurred",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })?; // ? will unwrap the Result<Option<(i32, Role)>, sqlx::Error> to Option<(i32, Role)>

        match result {
            Some(user_id_and_role) => return Ok(user_id_and_role), // ? operatior will unwrap this Ok(user_id_and_role) in the refresh_access_token function
            None => {
                return Err(AppError {
                    code: error_codes::INVALID_REFRESH_TOKEN,
                    message: "Invalid or expired refresh token",
                    status: StatusCode::UNAUTHORIZED,
                });
            }
        };
    }

    // Delete the refresh token on logout
    pub async fn remove_refresh_token(
        database: &SharedState,
        token: &str,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query("DELETE FROM refresh_tokens WHERE token = $1")
            .bind(token)
            .execute(&database.db)
            .await
            .map_err(|e| {
                eprintln!("Database error in remove_refresh_token: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "A database error occurred while removing the refresh token",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })
    }
}
