use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::{AppError, error_codes};
use crate::domain::models::users::{Role, UserResponse};
use crate::repositories::user_repo::UserRepository;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

pub async fn get_profile_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let caller_id = claim.sub;

    // Fetch caller to check role
    let caller = UserRepository::find_by_id(&database, caller_id)
        .await?
        .ok_or_else(|| AppError {
            code: error_codes::USER_NOT_FOUND,
            message: "Caller not found",
            status: StatusCode::NOT_FOUND,
        })?;

    // Only admins or managers can view other users' profiles
    if caller_id != id
        && !matches!(caller.role, Role::Admin)
        && !matches!(caller.role, Role::Manager)
    {
        return Err(AppError {
            code: error_codes::FORBIDDEN,
            message: "You are not authorized to view this profile",
            status: StatusCode::FORBIDDEN,
        });
    }

    // Fetch the requested user
    let user = UserRepository::find_by_id(&database, id)
        .await?
        .ok_or_else(|| AppError {
            code: error_codes::USER_NOT_FOUND,
            message: "User not found",
            status: StatusCode::NOT_FOUND,
        })?;

    let user_response = UserResponse::from(&user);

    Ok(Json(SuccessResponse {
        message: "User profile fetched successfully".to_string(),
        data: user_response,
    }))
}
