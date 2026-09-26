use image::imageops::FilterType;
use image::ImageReader;
use std::io::Cursor;

/// Resize and re-encode an image as JPEG (85% quality) so that its longest
/// side does not exceed `max_px`.
///
/// Rules:
///   - Only shrinks — never upscales a smaller image.
///   - All input formats (JPEG, PNG, WebP) are converted to JPEG output.
///   - Falls through to the original raw bytes on any decode/encode error so
///     that uploads always succeed, even if compression fails.
///
/// This function is CPU-bound (decode + Lanczos3 resize). Always call it
/// inside `tokio::task::spawn_blocking`.
pub fn compress_image(data: Vec<u8>, max_px: u32) -> Vec<u8> {
    let reader = match ImageReader::new(Cursor::new(&data)).with_guessed_format() {
        Ok(r) => r,
        Err(_) => return data,
    };
    let img = match reader.decode() {
        Ok(i) => i,
        Err(_) => return data,
    };

    // Only shrink — never upscale
    let img = {
        let (w, h) = (img.width(), img.height());
        if w > max_px || h > max_px {
            img.resize(max_px, max_px, FilterType::Lanczos3)
        } else {
            img
        }
    };

    // Re-encode as JPEG at 85% quality using the explicit encoder
    let mut out: Vec<u8> = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85);
    if img.write_with_encoder(encoder).is_err() {
        return data;
    }

    out
}
