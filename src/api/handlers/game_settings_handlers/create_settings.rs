use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::games::CreateGameSettingsRequest;
// NOTE: We rely on the GameSettingsService which will be implemented later
use crate::services::game_settings_service::GameSettingsService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

/* pub struct CreateGameSettingsRequest {
    pub label: String,
    pub base_price: BigDecimal,
    pub slot_duration_mins: i32,
} */

pub async fn create_settings_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(game_id): Path<i32>,
    Json(payload): Json<CreateGameSettingsRequest>,
) -> Result<impl IntoResponse, AppError> {
    let new_settings = GameSettingsService::create_settings(
        &database,
        game_id,
        claim.sub,
        claim.role,
        payload.label,
        payload.base_price,
        payload.slot_duration_mins,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Game settings template created successfully".to_string(),
            data: new_settings,
        }),
    ))
}
