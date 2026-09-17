use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// Lightweight photo metadata stored and returned without binary data
#[derive(Debug, Serialize, Deserialize, Clone, FromRow)]
pub struct TurfPhotoMetadata {
    pub id: i32,
    pub turf_id: i32,
    pub game_id: Option<i32>,
    pub category: String,
    pub filename: String,
    pub mime_type: String,
    pub file_size: i32,
    pub created_at: DateTime<Utc>,
}

/// Photo metadata with direct streaming URL for client consumption
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TurfPhotoResponse {
    pub id: i32,
    pub turf_id: i32,
    pub game_id: Option<i32>,
    pub category: String,
    pub filename: String,
    pub mime_type: String,
    pub file_size: i32,
    pub url: String,
    pub created_at: DateTime<Utc>,
}

impl TurfPhotoResponse {
    pub fn from_metadata(meta: TurfPhotoMetadata) -> Self {
        let url = format!("/api/v1/turfs/photos/{}/raw", meta.id);
        Self {
            id: meta.id,
            turf_id: meta.turf_id,
            game_id: meta.game_id,
            category: meta.category,
            filename: meta.filename,
            mime_type: meta.mime_type,
            file_size: meta.file_size,
            url,
            created_at: meta.created_at,
        }
    }
}

/// Raw image binary data fetched only when streaming the photo
#[derive(Debug, Clone, FromRow)]
pub struct TurfPhotoRaw {
    #[allow(dead_code)]
    pub id: i32,
    pub mime_type: String,
    pub photo_data: Vec<u8>,
}
