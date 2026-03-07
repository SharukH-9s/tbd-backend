use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::user_service::UserService;
use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Deserialize;

// Struct for extracting the username and passowrd taken from frontend
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: String,
    pub phone: String,
}

pub async fn register( State(database): State<SharedState>,Json(payload): Json<RegisterRequest>,) -> Result<impl IntoResponse, AppError> {
  
    // passing the payload and database to the service layer. then store the user_response in response_data
    let response_data = UserService::register_user(&database, &payload).await?; // if register_request returns any AppError, the `?` operator will automatically return it
    
    // if no error is returned, then return the response_data in Json format with status code 201
    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Registered successfully".to_string(),
            data: response_data,
        }),
    )) // StatusCode and Json are both IntoResponse trait objects. So, axum will automatically convert them into a response.
    
}

