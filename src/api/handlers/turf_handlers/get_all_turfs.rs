use crate::domain::dtos::responses::SuccessResponse;
use crate::services::turf_service::TurfService;
use crate::{config::state::SharedState, domain::errors::AppError};
use axum::{
    Json,
    extract::State,
    http::{HeaderValue, StatusCode, header},
    response::IntoResponse,
};

pub async fn get_all_turfs(
    State(state): State<SharedState>,
) -> Result<impl IntoResponse, AppError> {
    let turfs = TurfService::get_all_turfs(&state).await?;
    Ok((
        StatusCode::OK,
        [(
            header::CACHE_CONTROL,
            // Browsers and CDN edges cache for 60s.
            // stale-while-revalidate lets the edge serve stale content instantly
            // while fetching a fresh copy in the background (no user-visible lag on expiry).
            HeaderValue::from_static("public, max-age=60, stale-while-revalidate=300"),
        )],
        Json(SuccessResponse {
            message: "Turfs fetched successfully".to_string(),
            data: turfs,
        }),
    ))
}
