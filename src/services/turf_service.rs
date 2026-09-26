use crate::config::state::SharedState;
use crate::domain::errors::{error_codes, AppError};
use crate::domain::models::turfs::Turf;
use crate::repositories::turf_repo::TurfRepository;
use crate::repositories::user_repo::UserRepository;
use std::sync::Arc;

/// Allowed domains for Google Maps URLs (SSRF guard).
const ALLOWED_MAP_DOMAINS: &[&str] = &[
    "maps.google.com",
    "www.google.com",
    "maps.app.goo.gl",
    "goo.gl",
];

/// Coordinate regex pattern: matches /@lat,lng or ?q=lat,lng
fn extract_coords(url: &str) -> (Option<f64>, Option<f64>) {
    // Pattern 1: /@lat,lng (viewport anchor)
    let re_at = regex::Regex::new(r"/@(-?\d+\.\d+),(-?\d+\.\d+)").unwrap();
    if let Some(caps) = re_at.captures(url) {
        if let (Ok(lat), Ok(lng)) = (caps[1].parse::<f64>(), caps[2].parse::<f64>()) {
            return (Some(lat), Some(lng));
        }
    }
    // Pattern 2: ?q=lat,lng or &query=lat,lng
    let re_q = regex::Regex::new(r"[?&](?:q|query)=(-?\d+\.\d+),(-?\d+\.\d+)").unwrap();
    if let Some(caps) = re_q.captures(url) {
        if let (Ok(lat), Ok(lng)) = (caps[1].parse::<f64>(), caps[2].parse::<f64>()) {
            return (Some(lat), Some(lng));
        }
    }
    (None, None)
}

/// Validates the URL is a safe Google Maps link and attempts to resolve short links.
/// Returns `(canonical_url, lat, lng)`. Never fails — falls back to the raw input.
async fn parse_google_maps_url(raw: &str) -> (String, Option<f64>, Option<f64>) {
    // Basic scheme check
    if !raw.starts_with("https://") {
        return (raw.to_string(), None, None);
    }

    // Domain whitelist
    let url_str = raw.trim();
    let is_allowed = ALLOWED_MAP_DOMAINS
        .iter()
        .any(|d| url_str.contains(&format!("https://{}", d)));
    if !is_allowed {
        return (url_str.to_string(), None, None);
    }

    // If it's already a full URL with coords, extract immediately
    let (lat, lng) = extract_coords(url_str);
    if lat.is_some() {
        return (url_str.to_string(), lat, lng);
    }

    // Attempt redirect resolution only for known short-link domains
    let is_short = url_str.contains("maps.app.goo.gl") || url_str.contains("goo.gl/maps");
    if is_short {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(5))
            .timeout(std::time::Duration::from_secs(4))
            .user_agent("Mozilla/5.0 TurfBD-LocationResolver/1.0")
            .build()
            .unwrap_or_default();

        if let Ok(resp) = client.get(url_str).send().await {
            let final_url = resp.url().as_str().to_string();
            // Only accept final URL if it resolved to a known Google Maps domain
            if final_url.contains("google.com/maps") {
                let (rlat, rlng) = extract_coords(&final_url);
                tracing::debug!(short = url_str, resolved = %final_url, lat = ?rlat, lng = ?rlng, "Resolved short Google Maps URL");
                return (url_str.to_string(), rlat, rlng); // store original short link, save coords
            }
        }
    }

    // Fall through: store as-is, no coordinates
    (url_str.to_string(), None, None)
}

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
    async fn invalidate_turf_cache(state: &SharedState, turf_id: Option<i32>, manager_ids: &[i32]) {
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

    #[tracing::instrument(skip(state), name = "turf.get_all")]
    pub async fn get_all_turfs(state: &SharedState) -> Result<Vec<Turf>, AppError> {
        let cache_key = "cache:turf:all";

        if let Some(json_arc) = state.cache.get(cache_key).await {
            tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
            return serde_json::from_str(&json_arc).map_err(|_| {
                AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed")
            });
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
                let json = serde_json::to_string(&turfs).map_err(|_| {
                    AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed")
                })?;
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

        let turfs = serde_json::from_str(&json_arc).map_err(|_| {
            AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed")
        })?;
        Ok(turfs)
    }

    #[tracing::instrument(skip(state), fields(turf_id = id), name = "turf.get_by_id")]
    pub async fn get_turf_by_id(state: &SharedState, id: i32) -> Result<Turf, AppError> {
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

        let turf: Turf = serde_json::from_str(&json_arc).map_err(|_| {
            AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed")
        })?;

        Ok(turf)
    }

    #[tracing::instrument(skip(state), fields(manager_id), name = "turf.get_by_manager")]
    pub async fn get_turfs_by_manager(
        state: &SharedState,
        manager_id: i32,
    ) -> Result<Vec<Turf>, AppError> {
        let cache_key = format!("cache:turf:manager:{}", manager_id);

        if let Some(json_arc) = state.cache.get(&cache_key).await {
            tracing::debug!(key = %cache_key, source = "l1", "Cache hit");
            return serde_json::from_str(&json_arc).map_err(|_| {
                AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed")
            });
        }

        let json_arc = state
            .cache
            .try_get_with(&cache_key, async {
                if let Some(json) = state.l2_cache.get(&cache_key).await {
                    tracing::debug!(key = %cache_key, source = "l2", "Cache hit, warming L1");
                    return Ok::<Arc<str>, AppError>(json.into());
                }
                tracing::debug!(key = %cache_key, source = "db", "Cache miss, querying database");
                let turfs = TurfRepository::find_by_manager_id(state, manager_id).await?;
                let json = serde_json::to_string(&turfs).map_err(|_| {
                    AppError::internal(error_codes::INTERNAL_ERROR, "Serialization failed")
                })?;
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

        let turfs = serde_json::from_str(&json_arc).map_err(|_| {
            AppError::internal(error_codes::INTERNAL_ERROR, "Deserialization failed")
        })?;
        Ok(turfs)
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
        raw_google_maps_url: Option<String>,
    ) -> Result<Turf, AppError> {
        // Resolve Google Maps URL outside the transaction (async network call)
        let (maps_url, lat, lng) = match raw_google_maps_url.as_deref() {
            Some(u) if !u.is_empty() => {
                let (u, lat, lng) = parse_google_maps_url(u).await;
                (Some(u), lat, lng)
            }
            _ => (None, None, None),
        };

        let mut tx = state.db.begin().await.map_err(|_e| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

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
                        return Err(AppError::conflict(
                            error_codes::MANAGER_ALREADY_ASSIGNED,
                            "This user is already managing another turf.",
                        ));
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
                        bcrypt::hash(pass, bcrypt::DEFAULT_COST).map_err(|_| {
                            AppError::internal(
                                error_codes::HASH_FAILED,
                                "Failed to hash manager password",
                            )
                        })?;

                    let user_data = (
                        uname,
                        email.to_string(),
                        hashed_password,
                        "0000000000".to_string(),
                        crate::domain::models::users::Role::Manager,
                    );

                    let new_user = UserRepository::insert_user_tx(&mut *tx, user_data).await?;
                    tracing::info!(
                        user_id = new_user.id,
                        "New Manager user created during turf creation"
                    );
                    final_manager_id = new_user.id;
                } else {
                    return Err(AppError::bad_request(error_codes::USER_DETAILS_REQUIRED, "This email is not registered. Please provide manager_password and manager_username to onboard them."));
                }
            }
        }

        let newly_created_turf = TurfRepository::insert_tx(
            &mut *tx,
            final_manager_id,
            name,
            location,
            description,
            maps_url.as_deref(),
            lat,
            lng,
        )
        .await?;

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes",
            )
        })?;

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
        raw_google_maps_url: Option<String>,
    ) -> Result<Turf, AppError> {
        // Resolve Google Maps URL before the transaction
        // None = not provided (no change); Some("") = user cleared it
        let (maps_url, lat, lng): (
            Option<Option<String>>,
            Option<Option<f64>>,
            Option<Option<f64>>,
        ) = match raw_google_maps_url.as_deref() {
            None => (None, None, None), // not in request, don't touch
            Some("") => (Some(None), Some(None), Some(None)), // explicit clear
            Some(u) => {
                let (resolved, rlat, rlng) = parse_google_maps_url(u).await;
                (Some(Some(resolved)), Some(rlat), Some(rlng))
            }
        };

        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        let existing_turf = match TurfRepository::find_by_id_tx(&mut *tx, turf_id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Turf not found",
                ))
            }
        };

        let is_admin = matches!(caller_role, crate::domain::models::users::Role::Admin);
        let is_owner = existing_turf.manager_id == caller_id;

        if !is_admin && !is_owner {
            return Err(AppError::forbidden(
                error_codes::FORBIDDEN,
                "You do not have permission to update this turf",
            ));
        }

        let final_manager_id = if is_admin {
            requested_manager_id.unwrap_or(existing_turf.manager_id)
        } else {
            existing_turf.manager_id
        };

        if final_manager_id != existing_turf.manager_id {
            let user = match UserRepository::find_by_id_tx(&mut *tx, final_manager_id).await? {
                Some(u) => u,
                None => {
                    return Err(AppError::bad_request(
                        error_codes::USER_NOT_FOUND,
                        "The assigned manager ID does not exist",
                    ))
                }
            };

            if !matches!(user.role, crate::domain::models::users::Role::Admin) {
                if let Some(_) =
                    TurfRepository::find_by_manager_id_tx(&mut *tx, final_manager_id).await?
                {
                    return Err(AppError::conflict(
                        error_codes::MANAGER_ALREADY_ASSIGNED,
                        "The requested manager is already assigned to another turf.",
                    ));
                }
            }

            if !matches!(user.role, crate::domain::models::users::Role::Manager)
                && !matches!(user.role, crate::domain::models::users::Role::Admin)
            {
                UserRepository::promote_to_manager_tx(&mut *tx, final_manager_id).await?;
                tracing::info!(
                    user_id = final_manager_id,
                    "User promoted to Manager role during turf update"
                );
            }
        }

        let final_name = name.unwrap_or(existing_turf.name);
        let final_location = location.unwrap_or(existing_turf.location);
        let final_description = description.unwrap_or(existing_turf.description);
        // For map fields: use incoming value if provided, otherwise keep existing
        let final_maps_url = maps_url.unwrap_or(existing_turf.google_maps_url);
        let final_lat = lat.unwrap_or(existing_turf.latitude);
        let final_lng = lng.unwrap_or(existing_turf.longitude);

        let updated_turf = TurfRepository::update_tx(
            &mut *tx,
            turf_id,
            &final_name,
            &final_location,
            &final_description,
            final_manager_id,
            final_maps_url.as_deref(),
            final_lat,
            final_lng,
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
                        tracing::info!(
                            user_id = old_manager_id,
                            "Old Manager demoted to User after turf reassignment"
                        );
                    }
                }
            }
        }

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for turf update",
            )
        })?;

        // Invalidate: per-turf key, global list, old manager list, new manager list.
        // Both old and new manager IDs are passed so both per-manager caches are flushed.
        Self::invalidate_turf_cache(state, Some(turf_id), &[old_manager_id, final_manager_id])
            .await;

        Ok(updated_turf)
    }

    #[tracing::instrument(skip(state), fields(turf_id = id), name = "turf.delete")]
    pub async fn delete_turf(state: &SharedState, id: i32) -> Result<(), AppError> {
        let mut tx = state.db.begin().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_START_FAILED,
                "Failed to start database transaction",
            )
        })?;

        let existing_turf = match TurfRepository::find_by_id_tx(&mut *tx, id).await? {
            Some(t) => t,
            None => {
                return Err(AppError::not_found(
                    error_codes::TURF_NOT_FOUND,
                    "Turf not found",
                ))
            }
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
                    tracing::info!(
                        user_id = manager_id,
                        "Manager demoted to User after turf deletion"
                    );
                }
            }
        }

        tx.commit().await.map_err(|_| {
            AppError::internal(
                error_codes::TRANSACTION_COMMIT_FAILED,
                "Failed to finalize database changes for turf deletion",
            )
        })?;

        // Invalidate: per-turf key, global list, manager's list
        Self::invalidate_turf_cache(state, Some(id), &[manager_id]).await;

        Ok(())
    }
}
