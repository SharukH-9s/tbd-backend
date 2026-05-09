use crate::config::state::SharedState;
use crate::domain::errors::AppError;
use crate::domain::models::games::UpdateGameSettingsRequest;
use axum::{Json, extract::Path, extract::State, http::StatusCode};

pub async fn update_settings_handler(
    _state: State<SharedState>,
    _claim: crate::api::extractors::types::Claims,
    _path: Path<i32>,
    _payload: Json<UpdateGameSettingsRequest>,
) -> Result<(StatusCode, String), AppError> {
    Err(AppError {
        code: "SETTINGS_IMMUTABLE",
        message: "Settings templates cannot be edited once created. Create a new setting template instead.",
        status: StatusCode::FORBIDDEN,
    })
}
