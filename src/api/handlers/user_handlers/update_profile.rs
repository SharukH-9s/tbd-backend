use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::users::UpdateProfileRequest;
use crate::services::user_service::UserService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn update_profile_handler(
    State(database): State<SharedState>,
    claim: Claims,
    Path(id): Path<i32>,
    Json(payload): Json<UpdateProfileRequest>,
) -> Result<impl IntoResponse, AppError> {
    let updated_user = UserService::update_profile(
        &database,
        id,
        claim.sub,
        claim.role,
        payload,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Profile updated successfully".to_string(),
            data: updated_user,
        }),
    ))
}
