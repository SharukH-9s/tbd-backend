use crate::api::extractors::types::Claims;
use crate::api::utils::image::compress_image;
use crate::config::state::SharedState;
use crate::domain::dtos::responses::SuccessResponse;
use crate::domain::errors::AppError;
use crate::services::turf_photo_service::TurfPhotoService;
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::IntoResponse,
};

pub async fn upload_photos_handler(
    State(database): State<SharedState>,
    axum::Extension(claims): axum::Extension<Claims>,
    Path(turf_id): Path<i32>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut category = String::new();
    let mut game_id: Option<i32> = None;
    let mut raw_files: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        tracing::error!(error = %e, "Multipart extraction failed");
        AppError::bad_request("MULTIPART_ERROR", "Failed to parse multipart form data")
    })? {
        let field_name = field.name().unwrap_or("").to_string();

        match field_name.as_str() {
            "category" => {
                category = field.text().await.map_err(|_| {
                    AppError::bad_request("INVALID_FIELD", "Invalid category field")
                })?;
            }
            "game_id" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() && text != "null" && text != "undefined" {
                    game_id = text.parse::<i32>().ok();
                }
            }
            "photos" | "photo" | "file" | "files" => {
                let filename = field.file_name().unwrap_or("photo.jpg").to_string();
                let bytes = field.bytes().await.map_err(|_| {
                    AppError::bad_request("INVALID_FILE", "Failed to read photo data")
                })?;
                raw_files.push((filename, bytes.to_vec()));
            }
            _ => {
                // Ignore unexpected fields
            }
        }
    }

    if category.is_empty() {
        category = "Facility Photos".to_string();
    }

    // Determine resize cap: cover photos get a larger canvas than facility shots.
    let max_px: u32 = if category == "Cover Photo" { 1920 } else { 1280 };

    // Compress each image on a blocking thread (CPU-bound: decode + Lanczos3 + JPEG encode).
    // All outputs are JPEG regardless of input format.
    let mut files: Vec<(String, String, Vec<u8>)> = Vec::with_capacity(raw_files.len());
    for (filename, raw_bytes) in raw_files {
        let compressed = tokio::task::spawn_blocking(move || compress_image(raw_bytes, max_px))
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "Image compression task panicked");
                AppError::internal("COMPRESSION_ERROR", "Failed to compress image")
            })?;

        // Normalise extension to .jpg and mime to image/jpeg since output is always JPEG
        let filename = {
            let stem = std::path::Path::new(&filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("photo");
            format!("{}.jpg", stem)
        };
        files.push((filename, "image/jpeg".to_string(), compressed));
    }

    let uploaded = TurfPhotoService::upload_photos(
        &database,
        turf_id,
        claims.sub,
        claims.role,
        game_id,
        category,
        files,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse {
            message: "Photos uploaded successfully".to_string(),
            data: uploaded,
        }),
    ))
}
