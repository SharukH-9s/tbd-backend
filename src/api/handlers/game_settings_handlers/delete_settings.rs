use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::game_settings_service::GameSettingsService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn delete_settings_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(settings_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    GameSettingsService::delete_settings(&database, settings_id, claim.sub, claim.role).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Game settings template deleted successfully".to_string(),
            data: (),
        }),
    ))
}
