use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::game_service::GameService;
use axum::{
    Json,
    extract::Path,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};

pub async fn get_games_handler(
    State(database): State<SharedState>,
    Path(turf_id): Path<i32>,
) -> Result<impl IntoResponse, AppError> {
    let games = GameService::get_games_for_turf(&database, turf_id).await?;

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-store, must-revalidate, max-age=0"),
    );
    headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(header::EXPIRES, HeaderValue::from_static("0"));

    Ok((
        StatusCode::OK,
        headers,
        Json(SuccessResponse {
            message: "Games fetched successfully".to_string(),
            data: games,
        }),
    ))
}
