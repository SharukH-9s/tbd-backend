use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::users::Role;
use crate::services::turf_service::TurfService;
use axum::extract::State;
use axum::{Json, response::IntoResponse};

pub async fn get_managed_turfs_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = claim.sub;
    let user_role = claim.role;

    // Only managers and admins can have managed turfs.
    // Route through TurfService so the result is served from cache:turf:manager:{id}
    // (L1 Moka → L2 Redis → DB), and the cache key is properly invalidated on writes.
    let turfs = if matches!(user_role, Role::Manager | Role::Admin) {
        TurfService::get_turfs_by_manager(&database, user_id).await?
    } else {
        // Regular users don't manage turfs
        vec![]
    };

    Ok(Json(SuccessResponse {
        message: "Managed turfs fetched successfully".to_string(),
        data: turfs,
    }))
}
