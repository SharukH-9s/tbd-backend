use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type)] // sqlx::Type is used to map the enum to the database type automatically
#[sqlx(type_name = "text", rename_all = "PascalCase")] // enum variants will be stored as text in the database, and the variants will be renamed to PascalCase (Admin, User)
pub enum Role {
    Admin,
    Manager,
    User,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)] // sqlx::FromRow is used to map the database row to the User struct automatically
pub struct User {
    // Core Auth Info (Required)
    pub id: i32,
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub phone: String,
    pub role: Role,
    // Profile Info (Optional - added later)
    pub location: Option<String>,
    // Metadata (Professional Standard)
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// DTO (Data Transfer Object) to send to the frontend
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: i32,
    pub username: String,
    pub email: String,
    pub phone: String,
    pub role: Role,
    pub location: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// to convert a User struct to a UserResponse struct
impl From<&User> for UserResponse {
    /*pub trait From<T> {
              fn from(value: T) -> Self;
    } */
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            username: user.username.clone(),
            email: user.email.clone(),
            phone: user.phone.clone(),
            role: user.role.clone(),
            location: user.location.clone(),
            created_at: user.created_at,
            updated_at: user.updated_at,
        }
    }
}
