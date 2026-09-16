use crate::config::state::SharedState;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::turfs::Turf;
use crate::repositories::turf_repo::TurfRepository;
use crate::repositories::user_repo::UserRepository;
use std::sync::Arc;

pub struct TurfService;

impl TurfService {
    // -----------------------------------------------------------------------
    // Cache invalidation helper
    // -----------------------------------------------------------------------

    /// Invalidates all L1 and L2 cache keys affected by a turf write.
    ///
    /// - `turf_id`: `Some(id)` to also invalidate the per-turf key; `None` on creates.
    /// - `manager_ids`: every manager whose `cache:turf:manager:{id}` list must be
    ///   flushed (pass both old and new on a manager reassignment).
    async fn invalidate_turf_cache(
        state: &SharedState,
        turf_id: Option<i32>,
        manager_ids: &[i32],
    ) {
        // 1. Per-turf key (only meaningful on update / delete)
        if let Some(id) = turf_id {
            let specific_key = format!("cache:turf:{}", id);
            state.l2_cache.delete(&specific_key).await;
            state.cache.invalidate(&specific_key).await;
        }

        // 2. Global list (any write changes the full collection)
        let all_key = "cache:turf:all";
        state.l2_cache.delete(all_key).await;
        state.cache.invalidate(all_key).await;

        // 3. Per-manager lists
        // On update, pass both [old_manager_id, new_manager_id] so both lists are flushed.
        for mgr_id in manager_ids {
            let mgr_key = format!("cache:turf:manager:{}", mgr_id);
            state.l2_cache.delete(&mgr_key).await;
            state.cache.invalidate(&mgr_key).await;
        }
    }

    // -----------------------------------------------------------------------
    // Read operations
    // -----------------------------------------------------------------------

    #[tracing::instrument(skip(state), fields(user_id, role = ?user_role), name = "turf.get_all")]
    pub async fn get_all_turfs(
        state: &SharedState,
        user_id: i32,
        user_role: crate::domain::models::users::Role,
    ) -> Result<Vec<Turf>, AppError> {
        match user_role {
            crate::domain::models::users::Role::Manager => {
                // Managers see only their own turfs
                let cache_key = format!("cache:turf:manager:{}", user_id);

                if let Some(json_arc) = state.cache.get(&cache_key).await {
                    tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
                    return serde_json::from_str(&json_arc)
                        .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"));
                }

                let json_arc = state
                    .cache
                    .try_get_with(&cache_key, async {
                        // L2 check
                        if let Some(json) = state.l2_cache.get(&cache_key).await {
                            tracing::debug!(key = %cache_key, source = "l2", "Cache hit, warming L1");
                            return Ok::<Arc<str>, AppError>(json.into());
                        }
                        // DB fallback
                        tracing::debug!(key = %cache_key, source = "db", "Cache miss, querying database");
                        let turfs = TurfRepository::find_by_manager_id(state, user_id).await?;
                        let json = serde_json::to_string(&turfs)
                            .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed"))?;
                        state.l2_cache.set(&cache_key, &json, 300).await;
                        Ok(json.into())
                    })
                    .await
                    .map_err(|e| {
                        if e.status == axum::http::StatusCode::NOT_FOUND {
                            return AppError::not_found(e.code, e.message);
                        }
                        AppError::internal("CACHE_ERROR", "Cache fetch failed")
                    })?;

                let turfs = serde_json::from_str(&json_arc)
                    .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"))?;
                Ok(turfs)
            }

            crate::domain::models::users::Role::Admin
            | crate::domain::models::users::Role::User => {
                // Admins and normal users see all turfs
                let cache_key = "cache:turf:all";

                if let Some(json_arc) = state.cache.get(cache_key).await {
                    tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
                    return serde_json::from_str(&json_arc)
                        .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"));
                }

                let json_arc = state
                    .cache
                    .try_get_with(cache_key, async {
                        if let Some(json) = state.l2_cache.get(cache_key).await {
                            tracing::debug!(key = %cache_key, source = "l2", "Cache hit, warming L1");
                            return Ok::<Arc<str>, AppError>(json.into());
                        }
                        tracing::debug!(key = %cache_key, source = "db", "Cache miss, querying database");
                        let turfs = TurfRepository::find_all(state).await?;
                        let json = serde_json::to_string(&turfs)
                            .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed"))?;
                        state.l2_cache.set(cache_key, &json, 300).await;
                        Ok(json.into())
                    })
                    .await
                    .map_err(|e| {
                        if e.status == axum::http::StatusCode::NOT_FOUND {
                            return AppError::not_found(e.code, e.message);
                        }
                        AppError::internal("CACHE_ERROR", "Cache fetch failed")
                    })?;

                let turfs = serde_json::from_str(&json_arc)
                    .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"))?;
                Ok(turfs)
            }
        }
    }

    #[tracing::instrument(skip(state), fields(turf_id = id, caller_id, role = ?caller_role), name = "turf.get_by_id")]
    pub async fn get_turf_by_id(
        state: &SharedState,
        id: i32,
        caller_id: i32,
        caller_role: crate::domain::models::users::Role,
    ) -> Result<Turf, AppError> {
        let cache_key = format!("cache:turf:{}", id);

        let json_arc = if let Some(cached) = state.cache.get(&cache_key).await {
            tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
            cached
        } else {
            state
                .cache
                .try_get_with(&cache_key, async {
                    if let Some(json) = state.l2_cache.get(&cache_key).await {
                        tracing::debug!(key = %cache_key, source = "l2", "Cache hit, warming L1");
                        return Ok::<Arc<str>, AppError>(json.into());
                    }
                    tracing::debug!(key = %cache_key, source = "db", "Cache miss, querying database");
                    let turf = TurfRepository::find_by_id(state, id)
                        .await?
                        .ok_or_else(|| AppError::not_found(error_codes::TURF_NOT_FOUND, "Turf not found"))?;
                    let json = serde_json::to_string(&turf)
                        .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed"))?;
                    state.l2_cache.set(&cache_key, &json, 300).await;
                    Ok(json.into())
                })
                .await
                .map_err(|e| {
                    if e.status == axum::http::StatusCode::NOT_FOUND {
                        return AppError::not_found(e.code, e.message);
                    }
                    AppError::internal("CACHE_ERROR", "Cache fetch failed")
                })?
        };

        let turf: Turf = serde_json::from_str(&json_arc)
            .map_err(|_| AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed"))?;

        if matches!(caller_role, crate::domain::models::users::Role::Manager) && turf.manager_id != caller_id {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to view this turf",
            ));
        }

        Ok(turf)
    }

    // -----------------------------------------------------------------------
    // Write operations
    // -----------------------------------------------------------------------

    #[tracing::instrument(skip(state, manager_password), fields(creator_id, turf_name = name, manager_email = ?manager_email), name = "turf.create")]
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
                    tracing::info!(user_id = user.id, "User promoted to Manager role");
                }
            } else {
                if let (Some(pass), Some(uname)) = (manager_password, manager_username) {
                    let hashed_password =
                        bcrypt::hash(pass, bcrypt::DEFAULT_COST).map_err(|_| AppError::internal(error_codes::HASH_FAILED, "Failed to hash manager password"))?;

                    let user_data = (
                        uname,
                        email.to_string(),
                        hashed_password,
                        "0000000000".to_string(),
                        crate::domain::models::users::Role::Manager,
                    );

                    let new_user = UserRepository::insert_user_tx(&mut *tx, user_data).await?;
                    tracing::info!(user_id = new_user.id, "New Manager user created during turf creation");
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

        // Invalidate: global list + new manager's list
        Self::invalidate_turf_cache(state, None, &[final_manager_id]).await;

        Ok(newly_created_turf)
    }

    #[tracing::instrument(skip(state), fields(turf_id, caller_id), name = "turf.update")]
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
                tracing::info!(user_id = final_manager_id, "User promoted to Manager role during turf update");
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
                        tracing::info!(user_id = old_manager_id, "Old Manager demoted to User after turf reassignment");
                    }
                }
            }
        }

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for turf update"))?;

        // Invalidate: per-turf key, global list, old manager list, new manager list.
        // Both old and new manager IDs are passed so both per-manager caches are flushed.
        Self::invalidate_turf_cache(
            state,
            Some(turf_id),
            &[old_manager_id, final_manager_id],
        )
        .await;

        Ok(updated_turf)
    }

    #[tracing::instrument(skip(state), fields(turf_id = id), name = "turf.delete")]
    pub async fn delete_turf(
        state: &SharedState,
        id: i32,
    ) -> Result<(), AppError> {

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
                    tracing::info!(user_id = manager_id, "Manager demoted to User after turf deletion");
                }
            }
        }

        tx.commit().await.map_err(|_| AppError::internal(error_codes::TRANSACTION_COMMIT_FAILED, "Failed to finalize database changes for turf deletion"))?;

        // Invalidate: per-turf key, global list, manager's list
        Self::invalidate_turf_cache(state, Some(id), &[manager_id]).await;

        Ok(())
    }
}
