use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::turfs::Turf;
use crate::repositories::turf_repo::TurfRepository;
use crate::repositories::user_repo::UserRepository;

pub struct TurfService;

impl TurfService {
    pub async fn get_all_turfs(
        state: &SharedState,
        user_id: i32,
        user_role: crate::domain::models::users::Role,
    ) -> Result<Vec<Turf>, AppError> {
        match user_role {
            crate::domain::models::users::Role::Admin => {
                // Admins see all turfs
                TurfRepository::find_all(state).await
            }
            crate::domain::models::users::Role::Manager => {
                // Managers see only their own turfs
                TurfRepository::find_by_manager_id(state, user_id).await
            }
            crate::domain::models::users::Role::User => {
                // Normal users see all turfs
                TurfRepository::find_all(state).await
            }
        }
    }

    pub async fn get_turf_by_id(state: &SharedState, id: i32) -> Result<Turf, AppError> {
        let turf = TurfRepository::find_by_id(state, id).await?;
        match turf {
            Some(t) => Ok(t),
            None => Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
        }
    }

    pub async fn create_turf(
        state: &SharedState,
        creator_id: i32,
        name: &str,
        location: &str,
        description: &str,
        manager_email: Option<String>,
        manager_password: Option<String>,
        manager_username: Option<String>,
    ) -> Result<Turf, AppError> {
        let mut tx = state.db.begin().await.map_err(|_e| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        let mut final_manager_id = creator_id;

        if let Some(email) = &manager_email {
            let existing_user = UserRepository::find_by_email_tx(&mut *tx, &email).await?;

            if let Some(user) = existing_user {
                if manager_password.is_some() || manager_username.is_some() {
                    return Err(AppError::conflict(error_codes::EMAIL_EXISTS, "This email is already registered. To link this existing user, please omit the manager_password and manager_username."));
                }

                final_manager_id = user.id;

                if !matches!(user.role, crate::domain::models::users::Role::Admin) {
                    if let Some(_) =
                        TurfRepository::find_by_manager_id_tx(&mut *tx, final_manager_id).await?
                    {
                        return Err(AppError::conflict(error_codes::MANAGER_ALREADY_ASSIGNED, "This user is already managing another turf."));
                    }
                }

                if !matches!(user.role, crate::domain::models::users::Role::Manager)
                    && !matches!(user.role, crate::domain::models::users::Role::Admin)
                {
                    UserRepository::promote_to_manager_tx(&mut *tx, user.id).await?;
                    println!("User {} got promoted to Manager", user.id);
                }
            } else {
                if let (Some(pass), Some(uname)) = (manager_password, manager_username) {
                    let hashed_password =
                        bcrypt::hash(pass, bcrypt::DEFAULT_COST).map_err(|_| AppError::internal(error_codes::HASH_FAILED, "Failed to hash manager password"))?;

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
                    return Err(AppError::bad_request(error_codes::USER_DETAILS_REQUIRED, "This email is not registered. Please provide manager_password and manager_username to onboard them."));
                }
            }
        }

        let newly_created_turf =
            TurfRepository::insert_tx(&mut *tx, final_manager_id, name, location, description)
                .await?;

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes"))?;

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
        requested_manager_id: Option<i32>,
    ) -> Result<Turf, AppError> {
        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        let existing_turf = match TurfRepository::find_by_id_tx(&mut *tx, turf_id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
        };

        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = existing_turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "You do not have permission to update this turf"));
        }

        let final_manager_id = if is_admin {
            requested_manager_id.unwrap_or(existing_turf.manager_id)
        } else {
            existing_turf.manager_id
        };

        if final_manager_id != existing_turf.manager_id {
            let user = match UserRepository::find_by_id_tx(&mut *tx, final_manager_id).await? {
                Some(u) => u,
                None => return Err(AppError::bad_request(error_codes::USER_NOT_FOUND, "The assigned manager ID does not exist")),
            };

            if !matches!(user.role, crate::domain::models::users::Role::Admin) {
                if let Some(_) =
                    TurfRepository::find_by_manager_id_tx(&mut *tx, final_manager_id).await?
                {
                    return Err(AppError::conflict(error_codes::MANAGER_ALREADY_ASSIGNED, "The requested manager is already assigned to another turf."));
                }
            }

            if !matches!(user.role, crate::domain::models::users::Role::Manager)
                && !matches!(user.role, crate::domain::models::users::Role::Admin)
            {
                UserRepository::promote_to_manager_tx(&mut *tx, final_manager_id).await?;
                println!("User {} got promoted to Manager", final_manager_id);
            }
        }

        let final_name = name.unwrap_or(existing_turf.name);
        let final_location = location.unwrap_or(existing_turf.location);
        let final_description = description.unwrap_or(existing_turf.description);

        let updated_turf = TurfRepository::update_tx(
            &mut *tx,
            turf_id,
            &final_name,
            &final_location,
            &final_description,
            final_manager_id,
        )
        .await?;

        let old_manager_id = existing_turf.manager_id;
        if final_manager_id != old_manager_id {
            if let None = TurfRepository::find_by_manager_id_tx(&mut *tx, old_manager_id).await? {
                if let Some(user) = UserRepository::find_by_id_tx(&mut *tx, old_manager_id).await? {
                    if matches!(user.role, crate::domain::models::users::Role::Manager) {
                        UserRepository::set_role_tx(
                            &mut *tx,
                            old_manager_id,
                            crate::domain::models::users::Role::User,
                        )
                        .await?;
                        println!(
                            "Old Manager {} demoted to User role after turf reassignment",
                            old_manager_id
                        );
                    }
                }
            }
        }

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for turf update"))?;

        Ok(updated_turf)
    }

    pub async fn delete_turf(
        state: &SharedState,
        id: i32,
        caller_role: crate::domain::models::users::Role,
    ) -> Result<(), AppError> {
        // Only Admin can delete turfs
        if !matches!(caller_role, crate::domain::models::users::Role::Admin) {
            return Err(AppError::forbidden(error_codes::FORBIDDEN, "Only administrators can delete turfs"));
        }

        let mut tx = state.db.begin().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_START_FAILED, "Failed to start database transaction"))?;

        let existing_turf = match TurfRepository::find_by_id_tx(&mut *tx, id).await? {
            Some(t) => t,
            None => return Err(AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found")),
        };

        let manager_id = existing_turf.manager_id;

        TurfRepository::delete_tx(&mut *tx, id).await?;

        if let None = TurfRepository::find_by_manager_id_tx(&mut *tx, manager_id).await? {
            // Get user to check current role
            if let Some(user) = UserRepository::find_by_id_tx(&mut *tx, manager_id).await? {
                // Only demote if they are currently a Manager (don't touch Admins)
                if matches!(user.role, crate::domain::models::users::Role::Manager) {
                    UserRepository::set_role_tx(
                        &mut *tx,
                        manager_id,
                        crate::domain::models::users::Role::User,
                    )
                    .await?;
                    println!(
                        "User {} demoted to User role after turf deletion",
                        manager_id
                    );
                }
            }
        }

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for turf deletion")) // returns Result<(), AppError>
    }
}
