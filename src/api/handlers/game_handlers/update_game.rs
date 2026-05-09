use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::games::UpdateGameRequest;
use crate::services::game_service::GameService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn update_game_handler(
    State(database): State<SharedState>,
    claim: Claims,
    Path(game_id): Path<i32>,
    Json(payload): Json<UpdateGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let updated_game = GameService::update_game(
        &database,
        game_id,
        claim.sub,
        claim.role,
        payload.name,
        payload.game_start_time,
        payload.game_end_time,
    )
    .await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Game updated successfully".to_string(),
            data: updated_game,
        }),
    ))
}
