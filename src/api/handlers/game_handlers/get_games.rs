use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::game_service::GameService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn get_games_handler(
    State(database): State<SharedState>,
    Path(turf_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let games = GameService::get_games_for_turf(&database, turf_id).await?;

    Ok((
        StatusCode::OK,
        Json(SuccessResponse {
            message: "Games fetched successfully".to_string(),
            data: games,
        }),
    ))
}
