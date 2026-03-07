use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: ErrorBody // error is the root key for the error response
}

#[derive(Serialize)]
pub struct ErrorBody { // 
    pub code: String,
    pub message: String,
}

pub struct AppError { // struct to hold the error code, message, and status code
    pub code: &'static str,
    pub message: &'static str,
    pub status: StatusCode,
}

pub mod error_codes {
    pub const INVALID_USERNAME: &str = "INVALID_USERNAME";
    pub const USERNAME_EXISTS: &str = "USERNAME_EXISTS";
    pub const EMAIL_EXISTS: &str = "EMAIL_EXISTS";
    pub const HASH_FAILED: &str = "HASH_FAILED";
    pub const UN_HASH_FAILED: &str = "UNHASH_FAILED";
    pub const INVALID_CREDENTIALS: &str = "INVALID_CREDENTIALS"; // generic — covers both unknown email and wrong password
    pub const JWT_ENCODE_FAIL: &str = "JWT_ENCODE_FAILED";
    pub const NO_REFRESH_TOKEN: &str = "NO_REFRESH_TOKEN";
    pub const INVALID_REFRESH_TOKEN: &str = "INVALID_REFRESH_TOKEN";

    pub const MISSING_TOKEN: &str = "MISSING_TOKEN";
    pub const INVALID_TOKEN: &str = "INVALID_TOKEN";
    pub const EXPIRED_TOKEN: &str = "EXPIRED_TOKEN";
    pub const JWT_DECODE_FAILED: &str = "JWT_DECODE_FAILED";
    pub const DATABASE_ERROR: &str = "DATABASE_ERROR";
}

// In any error case, we will create an AppError object. As AppError has Intoresponse implemnted on it, that object will be transformed into a response by axum
// if a function returns impl IntoResponse, then it can return an AppError as it implements IntoResponse trait.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = ErrorResponse {
            error: ErrorBody {
                code: self.code.to_string(),
                message: self.message.to_string(),
            },
        };

        (self.status, Json(body)).into_response()
    }
}

/*
we could easily skip the ErrorResponse struct and just return the ErrorBody directly.
Most professional APIs choose a standard "root" key for different types of responses so the frontend knows exactly where to look.
Error root: { "error": { ... } }

If we use ErrorResponse, our JSON looks like this:

JSON
{
  "error": {
    "code": "INVALID_TOKEN",
    "message": "Bearer prefix missing"
  }
}
If we remove it and just use ErrorBody, it looks like this:

JSON
{
  "code": "INVALID_TOKEN",
  "message": "Bearer prefix missing"
} */
