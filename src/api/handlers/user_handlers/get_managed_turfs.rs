use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::users::Role;
use crate::repositories::turf_repo::TurfRepository;
use axum::extract::State;
use axum::{Json, response::IntoResponse};

pub async fn get_managed_turfs_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    // Extract user_id and role from JWT token
    let user_id = claim.sub;
    let user_role = claim.role;

    // Only managers and admins can have managed turfs
    // For now, fetch turfs for the current user's manager_id
    // This will be the user_id if they are a manager
    let turfs = if matches!(user_role, Role::Manager | Role::Admin) {
        TurfRepository::find_by_manager_id(&database, user_id).await?
    } else {
        // Regular users don't manage turfs
        vec![]
    };

    Ok(Json(SuccessResponse {
        message: "Managed turfs fetched successfully".to_string(),
        data: turfs,
    }))
}
