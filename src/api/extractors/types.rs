use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: i32,                                 // User ID
    pub role: crate::domain::models::users::Role, // User Role
    pub exp: i64,                                 // Expiration timestamp (seconds)
    pub iat: i64,                                 // Issued at timestamp
    pub jti: String,                              // JWT ID — unique per token, used for Redis blacklist
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user_id: i32,
}
