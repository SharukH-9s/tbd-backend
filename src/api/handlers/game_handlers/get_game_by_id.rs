use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::game_service::GameService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn get_game_by_id_handler(
    State(database): State<SharedState>,
    Path(id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let game = GameService::get_game_by_id(&database, id).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Game fetched successfully".to_string(),
            data: game,
        }),
    ))
}
