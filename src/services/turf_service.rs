use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::turfs::Turf;
use crate::repositories::turf_repo::TurfRepository;
use crate::repositories::user_repo::UserRepository;
use axum::http::StatusCode;

pub struct TurfService;

impl TurfService {
    pub async fn get_all_turfs(state: &SharedState) -> Result<Vec<Turf>, AppError> {
        TurfRepository::find_all(state).await
    }

    pub async fn create_turf(
        state: &SharedState,
        creator_id: i32,
        name: &str,
        location: &str,
        description: &str,
        price: bigdecimal::BigDecimal,
        manager_email: Option<String>,
        manager_password: Option<String>,
        manager_username: Option<String>,
    ) -> Result<Turf, AppError> {
        // --- START TRANSACTION ---
        let mut tx = state.db.begin().await.map_err(|_e| AppError {
            // state.db.begin() returns a sqlx::Transaction object
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        let mut final_manager_id = creator_id;

        // 1. Linked Manager Logic
        if let Some(email) = &manager_email {
            let existing_user = UserRepository::find_by_email_tx(&mut *tx, &email).await?; // Some(User) or None // *tx is a sqlx::PgConnection // &mut *tx is a mutable reference to that sqlx::PgConnection

            if let Some(user) = existing_user {
                // --- CONFLICT CHECK ---
                if manager_password.is_some() || manager_username.is_some() {
                    return Err(AppError {
                        code: error_codes::EMAIL_EXISTS,
                        message: "This email is already registered. To link this existing user, please omit the manager_password and manager_username.",
                        status: StatusCode::CONFLICT,
                    });
                }

                final_manager_id = user.id;

                // --- ONE TURF PER MANAGER CHECK ---
                if let Some(_) =
                    TurfRepository::find_by_manager_id_tx(&mut *tx, final_manager_id).await?
                {
                    return Err(AppError {
                        code: error_codes::MANAGER_ALREADY_ASSIGNED,
                        message: "This user is already managing another turf.",
                        status: StatusCode::CONFLICT,
                    });
                }

                // --- AUTO-PROMOTION LOGIC ---
                if !matches!(user.role, crate::domain::models::users::Role::Manager)
                    && !matches!(user.role, crate::domain::models::users::Role::Admin)
                {
                    UserRepository::promote_to_manager_tx(&mut *tx, user.id).await?;
                    println!("User {} got promoted to Manager", user.id);
                }
            } else {
                // CASE 2: New Partner
                if let (Some(pass), Some(uname)) = (manager_password, manager_username) {
                    let hashed_password =
                        bcrypt::hash(pass, bcrypt::DEFAULT_COST).map_err(|_| AppError {
                            code: error_codes::HASH_FAILED,
                            message: "Failed to hash manager password",
                            status: StatusCode::INTERNAL_SERVER_ERROR,
                        })?;

                    let user_data = (
                        uname,
                        email.to_string(), // email is &String here
                        hashed_password,
                        "0000000000".to_string(),
                        crate::domain::models::users::Role::Manager,
                    );

                    let new_user = UserRepository::insert_user_tx(&mut *tx, user_data).await?;
                    println!("New user created: {:?}", new_user);
                    final_manager_id = new_user.id;
                } else {
                    return Err(AppError {
                        code: error_codes::USER_DETAILS_REQUIRED,
                        message: "This email is not registered. Please provide manager_password and manager_username to onboard them.",
                        status: StatusCode::BAD_REQUEST,
                    });
                }
            }
        }

        // 2. CHECK FOR CREATOR (If no email was provided, we still check the creator)
        if manager_email.is_none() {
            if let Some(_) =
                TurfRepository::find_by_manager_id_tx(&mut *tx, final_manager_id).await?
            {
                return Err(AppError {
                    code: error_codes::MANAGER_ALREADY_ASSIGNED,
                    message: "You are already managing another turf. One manager per turf allowed.",
                    status: StatusCode::CONFLICT,
                });
            }
        }

        // 3. Create the turf
        let newly_created_turf = TurfRepository::insert_tx(
            &mut *tx,
            final_manager_id,
            name,
            location,
            description,
            price,
        )
        .await?;

        // --- COMMIT TRANSACTION ---
        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(newly_created_turf)
    }

    pub async fn update_turf(
        state: &SharedState,
        turf_id: i32,
        caller_id: i32,
        caller_role: crate::domain::models::users::Role,
        name: Option<String>,
        location: Option<String>,
        description: Option<String>,
        price_per_hour: Option<bigdecimal::BigDecimal>,
        requested_manager_id: Option<i32>,
    ) -> Result<Turf, AppError> {
        // --- START TRANSACTION ---
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Fetch the existing turf to check ownership and existence
        let existing_turf = match TurfRepository::find_by_id_tx(&mut *tx, turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError {
                    code: error_codes::TURF_NOT_FOUND,
                    message: "Turf not found",
                    status: StatusCode::NOT_FOUND,
                });
            }
        };

        // 2. SECURITY CHECK (RBAC)
        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = existing_turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "You do not have permission to update this turf",
                status: StatusCode::FORBIDDEN,
            });
        }

        // 3. Determine the final manager_id
        // Admins can re-assign the turf, Managers cannot change the owner.
        let final_manager_id = if is_admin {
            // if admin is updating the turf without providing a new manager_id, keep the existing manager_id
            requested_manager_id.unwrap_or(existing_turf.manager_id)
        } else {
            // if manager is updating the turf, they cannot change the owner
            existing_turf.manager_id
        };

        if final_manager_id != existing_turf.manager_id {
            // 4. Admin provided a new manager_id, so verify and auto-promote if needed
            let user = match UserRepository::find_by_id_tx(&mut *tx, final_manager_id).await? {
                Some(u) => u,
                None => {
                    return Err(AppError {
                        code: error_codes::USER_NOT_FOUND,
                        message: "The assigned manager ID does not exist",
                        status: StatusCode::BAD_REQUEST,
                    });
                }
            };

            // --- ONE TURF PER MANAGER CHECK ---
            if let Some(_) =
                TurfRepository::find_by_manager_id_tx(&mut *tx, final_manager_id).await?
            {
                return Err(AppError {
                    code: error_codes::MANAGER_ALREADY_ASSIGNED,
                    message: "The requested manager is already assigned to another turf.",
                    status: StatusCode::CONFLICT,
                });
            }

            // 4.1 Auto-Promote the manager
            if !matches!(user.role, crate::domain::models::users::Role::Manager)
                && !matches!(user.role, crate::domain::models::users::Role::Admin)
            {
                UserRepository::promote_to_manager_tx(&mut *tx, final_manager_id).await?;
                println!("User {} got promoted to Manager", final_manager_id);
            }
        }

        // 5. Merge fields
        let final_name = name.unwrap_or(existing_turf.name);
        let final_location = location.unwrap_or(existing_turf.location);
        let final_description = description.unwrap_or(existing_turf.description);
        let final_price = price_per_hour.unwrap_or(existing_turf.price_per_hour);

        // 6. Perform the actual Turf update
        let updated_turf = TurfRepository::update_tx(
            &mut *tx,
            turf_id,
            &final_name,
            &final_location,
            &final_description,
            final_price,
            final_manager_id,
        )
        .await?;

        // --- COMMIT TRANSACTION ---
        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes for turf update",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        Ok(updated_turf)
    }

    pub async fn delete_turf(
        state: &SharedState,
        id: i32,
        caller_role: crate::domain::models::users::Role,
    ) -> Result<(), AppError> {
        // Only Admin can delete turfs
        if !matches!(caller_role, crate::domain::models::users::Role::Admin) {
            return Err(AppError {
                code: error_codes::FORBIDDEN,
                message: "Only administrators can delete turfs",
                status: StatusCode::FORBIDDEN,
            });
        }

        // Start transaction for atomic delete
        let mut tx = state.db.begin().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_START_FAILED,
            message: "Failed to start database transaction",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        })?;

        // 1. Verify existence inside transaction
        let existing = TurfRepository::find_by_id_tx(&mut *tx, id).await?;
        if existing.is_none() {
            return Err(AppError {
                code: error_codes::TURF_NOT_FOUND,
                message: "Turf not found",
                status: StatusCode::NOT_FOUND,
            });
        }

        // 2. Perform delete
        sqlx::query("DELETE FROM turfs WHERE id = $1")
            .bind(id)
            .execute(&mut *tx) // Result<PgQueryResult, sqlx::Error>
            .await
            .map_err(|e| {
                eprintln!("Database error: {}", e);
                AppError {
                    code: error_codes::DATABASE_ERROR,
                    message: "Failed to delete turf from database",
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                }
            })?;

        tx.commit().await.map_err(|_| AppError {
            code: error_codes::TRANSACTION_COMMIT_FAILED,
            message: "Failed to finalize database changes for turf deletion",
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }) // returns Result<(), AppError>
    }
}
