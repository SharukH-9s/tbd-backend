
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize,)]
pub struct Claims {
    pub sub: i32, // User ID
    pub exp: i64, // Expiration timestamp (seconds) <--- jsonwebtoken looks specifically for this name!  jsonwebtoken library follows the RFC 7519 standard, which specifically dictates that the field must be named exp.
    pub iat: i64, // Issued at timestamp
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user_id: i32,
}
