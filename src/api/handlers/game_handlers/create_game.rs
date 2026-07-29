use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::games::CreateGameRequest;
use crate::services::game_service::GameService;
use axum::{Json, extract::Path, extract::State, http::StatusCode, response::IntoResponse};

pub async fn create_game_handler(
    State(database): State<SharedState>,
    axum::Extension(claim): axum::Extension<Claims>,
    Path(turf_id): Path<i32>,
    Json(payload): Json<CreateGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let newly_created_game = GameService::create_game(
        &database,
        turf_id,
        claim.sub,
        claim.role,
        payload.name,
        payload.game_start_time,
        payload.game_end_time,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Game created successfully".to_string(),
            data: newly_created_game,
        }),
    ))
}
