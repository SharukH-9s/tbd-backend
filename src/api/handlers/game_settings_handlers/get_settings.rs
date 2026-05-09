use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::game_settings_service::GameSettingsService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn get_settings_handler(
    State(database): State<SharedState>,
    Path(game_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    // Note: Fetching settings doesn't strictly need auth check if the dropdown is public,
    // but if it's admin/manager only, Claims could be added here later.
    let settings = GameSettingsService::get_settings_for_game(&database, game_id).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Game settings fetched successfully".to_string(),
            data: settings,
        }),
    ))
}
