use crate::api::extractors::types::Claims;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::domain::models::turfs::CreateTurfRequest;
use crate::services::turf_service::TurfService;
use axum::{Extension, Json, extract::State, http::StatusCode, response::IntoResponse};

/*pub struct CreateTurfRequest {
    pub name: String,
    pub location: String,
    pub description: Option<String>,
    pub price_per_hour: bigdecimal::BigDecimal,
    // Linked Manager Creation (Concierge Onboarding)
    pub manager_email: Option<String>,
    pub manager_password: Option<String>,
    pub manager_username: Option<String>,
} */

pub async fn create_turf(
    State(database): State<SharedState>,
    Extension(claim): Extension<Claims>, // Read pre-decoded claims from middleware!
    Json(payload): Json<CreateTurfRequest>,
) -> Result<impl IntoResponse, AppError> {

    // We pass the parsed data to the service layer.
    // Notice how we use `claim.sub` as the creator_id: the user who is logged in!
    let newly_created_turf = TurfService::create_turf(
        &database,
        claim.sub,
        &payload.name,
        &payload.location,
        &payload.description, // &String -> &str
        payload.manager_email.clone(),
        payload.manager_password.clone(),
        payload.manager_username.clone(),
        payload.google_maps_url.clone(),
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Turf created successfully".to_string(),
            data: newly_created_turf,
        }),
    ))
}
