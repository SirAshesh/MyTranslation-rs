use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;

pub async fn extract_image_bytes(src: &str) -> Result<Vec<u8>, String> {
    let trimmed = src.trim();

    if trimmed.starts_with("data:") {
        if let Some(idx) = trimmed.find(";base64,") {
            let b64_data = &trimmed[idx + 8..];
            return BASE64
                .decode(b64_data)
                .map_err(|e| format!("Base64 decode failed: {}", e));
        }
    }

    if let Ok(b) = BASE64.decode(trimmed) {
        return Ok(b);
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        let resp = reqwest::get(trimmed)
            .await
            .map_err(|e| format!("Failed to download image from URL: {}", e))?;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read image bytes: {}", e))?;
        return Ok(bytes.to_vec());
    }

    Err("Invalid image format. Expected Base64 data URI or HTTP URL.".to_string())
}
