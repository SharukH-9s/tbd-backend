use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct Turf {
    pub id: i32,
    pub manager_id: i32, // The user managing/owning the turf
    pub name: String,
    pub location: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,

    // Joined from users table (not stored in turfs)
    pub manager_name: Option<String>,
    pub manager_email: Option<String>,
    pub manager_phone: Option<String>,

    // Google Maps location
    pub google_maps_url: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CreateTurfRequest {
    pub name: String,
    pub location: String,
    pub description: String,

    // Linked Manager Creation (Concierge Onboarding)
    pub manager_email: Option<String>,
    pub manager_password: Option<String>,
    pub manager_username: Option<String>,

    // Google Maps location (optional at creation)
    pub google_maps_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateTurfRequest {
    pub name: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub manager_id: Option<i32>, // Admin can use this to re-assign!

    // Google Maps location (None = no change; Some("") = clear)
    pub google_maps_url: Option<String>,
}
