use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::analytics::MonthlyAnalyticsFilter;
use crate::services::analytics_service::AnalyticsService;
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::{Extension, Json};

/// GET /analytics/monthly
///
/// Fetches monthly or yearly analytics for turfs and games.
/// Admins can see all turfs or filter by specific turf.
/// Managers can only view their own turfs.
pub async fn get_monthly_analytics_handler(
    State(database): State<SharedState>,
    Query(filter): Query<MonthlyAnalyticsFilter>,
    Extension(claim): Extension<Claims>,
) -> Result<impl IntoResponse, AppError> {
    let analytics = AnalyticsService::get_monthly_analytics(
        &database,
        claim.sub,
        &claim.role,
        filter,
    )
    .await?;

    Ok(Json(SuccessResponse {
        message: "Monthly analytics fetched successfully".to_string(),
        data: analytics,
    }))
}
