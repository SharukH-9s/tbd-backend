use crate::{db_conn_err, db_err};
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
        let mut conn = database.db.acquire().await.map_err(db_conn_err!())?;
        Self::find_by_email_tx(&mut *conn, email).await
    }

    pub async fn find_by_id(
        database: &SharedState,
        id: i32,
    ) -> Result<Option<User>, AppError> {
        let mut conn = database.db.acquire().await.map_err(db_conn_err!())?;
        Self::find_by_id_tx(&mut *conn, id).await
    }

    pub async fn find_by_email_tx(
        executor: &mut sqlx::PgConnection,
        email: &str,
    ) -> Result<Option<User>, AppError> {
        sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, phone, role as \"role: Role\",
                    location, created_at, updated_at
             FROM users WHERE email = $1",
            email,
        )
        .fetch_optional(executor)
        .await
        .map_err(db_err!("A database error occurred while fetching user by email"))
    }

    pub async fn find_by_id_tx(
        executor: &mut sqlx::PgConnection,
        id: i32,
    ) -> Result<Option<User>, AppError> {
        sqlx::query_as!(
            User,
            "SELECT id, username, email, password_hash, phone, role as \"role: Role\",
                    location, created_at, updated_at
             FROM users WHERE id = $1",
            id,
        )
        .fetch_optional(executor)
        .await
        .map_err(db_err!("A database error occurred while fetching user by id"))
    }

    pub async fn insert_user_if_unique(
        database: &SharedState,
        user_data: (String, String, String, String, Role),
    ) -> Result<User, AppError> {
        let mut conn = database.db.acquire().await.map_err(db_conn_err!())?;
        Self::insert_user_tx(&mut *conn, user_data).await
    }

    pub async fn insert_user_tx(
        executor: &mut sqlx::PgConnection,
        user_data: (String, String, String, String, Role),
    ) -> Result<User, AppError> {
        let (username, email, password_hash, phone, role) = user_data;

        sqlx::query_as!(
            User,
            "INSERT INTO users (username, email, password_hash, phone, role)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, username, email, password_hash, phone, role as \"role: Role\",
                       location, created_at, updated_at",
            username,
            email,
            password_hash,
            phone,
            role as Role, // cast the enum to its sqlx type
        )
        .fetch_one(executor)
        .await
        .map_err(|e| {
            // Unique constraint violations need specific error codes — cannot use db_err! here.
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
            tracing::error!(error = %e, "Unexpected database error during user registration");
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
        sqlx::query!(
            "UPDATE users SET role = $1 WHERE id = $2",
            role as Role, // cast the enum to its sqlx type
            user_id,
        )
        .execute(executor)
        .await
        .map_err(db_err!("Failed to update user role"))
    }

    // Delete ALL refresh tokens for a given user.
    // Called at the start of login so a user can only have one active session at a time.
    pub async fn delete_tokens_for_user(
        database: &SharedState,
        user_id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE user_id = $1", user_id)
            .execute(&database.db)
            .await
            .map_err(db_err!("A database error occurred while deleting the refresh tokens"))
    }

    // Store a new refresh token linked to a user_id
    pub async fn store_refresh_token(
        database: &SharedState,
        token: String,
        user_id: i32,
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query!(
            "INSERT INTO refresh_tokens (token, user_id) VALUES ($1, $2)",
            token,
            user_id,
        )
        .execute(&database.db)
        .await
        .map_err(db_err!("A database error occurred while storing the refresh token"))
    }

    // Delete the old refresh token and return the user_id and role it belonged to.
    pub async fn swap_refresh_token(
        database: &SharedState,
        old_token: &str,
    ) -> Result<(i32, Role), AppError> {
        // query! is used instead of query_as! because the result is a tuple (i32, Role),
        // not a named struct. The "role!: Role" column override tells sqlx to decode the
        // text column as our Role enum at compile time.
        let result = sqlx::query!(
            r#"WITH deleted AS (
                   DELETE FROM refresh_tokens WHERE token = $1 RETURNING user_id
               )
               SELECT u.id, u.role as "role!: Role" FROM users u
               INNER JOIN deleted d ON u.id = d.user_id"#,
            old_token,
        )
        .fetch_optional(&database.db)
        .await
        .map_err(db_err!("A database error occurred"))?; // ? will unwrap the Result<Option<_>, sqlx::Error> to Option<_>

        match result {
            Some(row) => Ok((row.id, row.role)),
            None => Err(AppError {
                code: error_codes::INVALID_REFRESH_TOKEN,
                message: "Invalid or expired refresh token",
                status: StatusCode::UNAUTHORIZED,
            }),
        }
    }

    // Delete the refresh token on logout
    pub async fn remove_refresh_token(
        database: &SharedState,
        token: &str, // deref coercion
    ) -> Result<sqlx::postgres::PgQueryResult, AppError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE token = $1", token)
            .execute(&database.db)
            .await
            .map_err(db_err!("A database error occurred while removing the refresh token"))
    }

    pub async fn update_profile_tx(
        executor: &mut sqlx::PgConnection,
        user_id: i32,
        username: &str,
        password_hash: &str,
        phone: &str,
        location: Option<&str>,
    ) -> Result<User, AppError> {
        sqlx::query_as!(
            User,
            "UPDATE users
             SET username = $1, password_hash = $2, phone = $3, location = $4, updated_at = NOW()
             WHERE id = $5
             RETURNING id, username, email, password_hash, phone, role as \"role: Role\",
                       location, created_at, updated_at",
            username,
            password_hash,
            phone,
            location,
            user_id,
        )
        .fetch_one(executor)
        .await
        .map_err(|e| {
            // Unique constraint on username must return a specific error — cannot use db_err! here.
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
                }
            }
            tracing::error!(error = %e, "Unexpected database error during profile update");
            AppError {
                code: error_codes::DATABASE_ERROR,
                message: "A database error occurred while updating the profile",
                status: StatusCode::INTERNAL_SERVER_ERROR,
            }
        })
    }
}
