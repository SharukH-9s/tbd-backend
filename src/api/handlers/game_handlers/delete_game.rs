use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::game_service::GameService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn delete_game_handler(
    State(database): State<SharedState>,
    claim: Claims,
    Path(game_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    GameService::delete_game(&database, game_id, claim.sub, claim.role).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Game deleted successfully".to_string(),
            data: (),
        }),
    ))
}
