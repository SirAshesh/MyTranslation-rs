use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use reqwest::redirect::Policy;
use reqwest::Url;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

const MAX_REDIRECT_HOPS: usize = 3;

#[derive(Debug)]
pub enum ImageExtractError {
    TooLarge(String),
    Invalid(String),
}

impl ImageExtractError {
    pub fn message(&self) -> &str {
        match self {
            Self::TooLarge(msg) | Self::Invalid(msg) => msg,
        }
    }
}

fn format_too_large_message(actual_bytes: usize, max_bytes: usize) -> String {
    let actual_mb = actual_bytes as f64 / (1024.0 * 1024.0);
    let max_mb = (max_bytes as f64 / (1024.0 * 1024.0)).max(1.0);
    format!(
        "Image size ({:.2} MB) exceeds the maximum allowed limit of {:.0} MB.",
        actual_mb, max_mb
    )
}

fn is_disallowed_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            let is_cgnat = octets[0] == 100 && (octets[1] & 0b1100_0000) == 64;
            let is_benchmark = octets[0] == 198 && (octets[1] == 18 || octets[1] == 19);
            let is_reserved = octets[0] >= 240;
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                || is_cgnat
                || is_benchmark
                || is_reserved
        }
        IpAddr::V6(v6) => {
            if let Some(mapped_v4) = v6.to_ipv4_mapped() {
                return is_disallowed_ip(IpAddr::V4(mapped_v4));
            }
            let seg0 = v6.segments()[0];
            let is_unique_local = (seg0 & 0xfe00) == 0xfc00;
            let is_link_local = (seg0 & 0xffc0) == 0xfe80;
            let is_site_local = (seg0 & 0xffc0) == 0xfec0;
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || is_unique_local
                || is_link_local
                || is_site_local
        }
    }
}

fn is_disallowed_hostname(host: &str) -> bool {
    let lower = host.trim_end_matches('.').to_ascii_lowercase();
    lower == "localhost"
        || lower.ends_with(".localhost")
        || lower.ends_with(".local")
        || lower.ends_with(".internal")
        || lower.ends_with(".arpa")
}

async fn validate_and_resolve_url(raw_url: &str) -> Result<(Url, String, SocketAddr), ImageExtractError> {
    let parsed = Url::parse(raw_url)
        .map_err(|e| ImageExtractError::Invalid(format!("Invalid image URL: {}", e)))?;

    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(ImageExtractError::Invalid(
            "Only http:// and https:// image URLs are allowed.".to_string(),
        ));
    }

    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(ImageExtractError::Invalid(
            "Image URLs with embedded credentials are not allowed.".to_string(),
        ));
    }

    let host_str = parsed
        .host_str()
        .ok_or_else(|| ImageExtractError::Invalid("Image URL is missing a host.".to_string()))?
        .trim_matches(|c| c == '[' || c == ']')
        .to_string();

    if is_disallowed_hostname(&host_str) {
        return Err(ImageExtractError::Invalid(
            "Image URL host is not allowed.".to_string(),
        ));
    }

    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| ImageExtractError::Invalid("Image URL port is invalid.".to_string()))?;

    if let Ok(ip) = host_str.parse::<IpAddr>() {
        if is_disallowed_ip(ip) {
            return Err(ImageExtractError::Invalid(
                "Private, loopback, or reserved IP addresses are not allowed.".to_string(),
            ));
        }
        return Ok((parsed, host_str, SocketAddr::new(ip, port)));
    }

    let resolved: Vec<SocketAddr> = tokio::net::lookup_host((host_str.as_str(), port))
        .await
        .map_err(|e| ImageExtractError::Invalid(format!("Failed to resolve image host: {}", e)))?
        .collect();

    if resolved.is_empty() {
        return Err(ImageExtractError::Invalid(
            "Image host resolved to no addresses.".to_string(),
        ));
    }

    if resolved.iter().any(|addr| is_disallowed_ip(addr.ip())) {
        return Err(ImageExtractError::Invalid(
            "Image URL resolved to a disallowed internal or private IP address.".to_string(),
        ));
    }

    Ok((parsed, host_str, resolved[0]))
}

fn decode_base64_bounded(b64_data: &str, max_bytes: usize) -> Result<Vec<u8>, ImageExtractError> {
    let max_encoded_len = max_bytes.saturating_mul(2).saturating_add(1024);
    if b64_data.len() > max_encoded_len {
        let approx_bytes = (b64_data.len() * 3) / 4;
        return Err(ImageExtractError::TooLarge(format_too_large_message(
            approx_bytes,
            max_bytes,
        )));
    }
    let decoded = BASE64
        .decode(b64_data)
        .map_err(|e| ImageExtractError::Invalid(format!("Base64 decode failed: {}", e)))?;
    if decoded.len() > max_bytes {
        return Err(ImageExtractError::TooLarge(format_too_large_message(
            decoded.len(),
            max_bytes,
        )));
    }
    Ok(decoded)
}

async fn download_bounded_image(url: &str, max_bytes: usize) -> Result<Vec<u8>, ImageExtractError> {
    let mut current_url = url.to_string();
    let mut resp = None;

    for _ in 0..=MAX_REDIRECT_HOPS {
        let (parsed, host_str, pinned_addr) = validate_and_resolve_url(&current_url).await?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
            .resolve(&host_str, pinned_addr)
            .build()
            .map_err(|e| {
                ImageExtractError::Invalid(format!("Failed to create HTTP client: {}", e))
            })?;

        let response = client.get(parsed.as_str()).send().await.map_err(|e| {
            ImageExtractError::Invalid(format!("Failed to download image from URL: {}", e))
        })?;

        if response.status().is_redirection() {
            let Some(location) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
            else {
                return Err(ImageExtractError::Invalid(
                    "Image URL returned redirect without Location header.".to_string(),
                ));
            };
            let next = parsed.join(location).map_err(|e| {
                ImageExtractError::Invalid(format!("Invalid redirect URL: {}", e))
            })?;
            current_url = next.to_string();
            continue;
        }

        resp = Some(response);
        break;
    }

    let mut resp = resp.ok_or_else(|| {
        ImageExtractError::Invalid("Too many redirects while downloading image URL.".to_string())
    })?;

    if !resp.status().is_success() {
        return Err(ImageExtractError::Invalid(format!(
            "Image URL returned HTTP {}",
            resp.status()
        )));
    }

    if let Some(len) = resp.content_length() {
        if len > max_bytes as u64 {
            return Err(ImageExtractError::TooLarge(format_too_large_message(
                len as usize,
                max_bytes,
            )));
        }
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| ImageExtractError::Invalid(format!("Failed to read image bytes: {}", e)))?
    {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(ImageExtractError::TooLarge(format_too_large_message(
                bytes.len().saturating_add(chunk.len()),
                max_bytes,
            )));
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}

pub async fn extract_image_bytes(src: &str, max_bytes: usize) -> Result<Vec<u8>, ImageExtractError> {
    let trimmed = src.trim();
    let limit = max_bytes.max(1024);

    if trimmed.starts_with("data:") {
        if let Some(idx) = trimmed.find(";base64,") {
            let b64_data = &trimmed[idx + 8..];
            return decode_base64_bounded(b64_data, limit);
        }
    }

    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return download_bounded_image(trimmed, limit).await;
    }

    if let Ok(decoded) = decode_base64_bounded(trimmed, limit) {
        return Ok(decoded);
    }

    if let Err(err @ ImageExtractError::TooLarge(_)) = decode_base64_bounded(trimmed, limit) {
        return Err(err);
    }

    Err(ImageExtractError::Invalid(
        "Invalid image format. Expected Base64 data URI or HTTP URL.".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_base64_exceeding_max_bytes() {
        let oversized = vec![b'A'; 2048];
        let b64 = BASE64.encode(&oversized);
        let err = extract_image_bytes(&b64, 1024).await.unwrap_err();
        assert!(matches!(err, ImageExtractError::TooLarge(_)));
    }

    #[tokio::test]
    async fn accepts_data_uri_within_max_bytes() {
        let payload = b"hello-image-bytes";
        let uri = format!("data:image/png;base64,{}", BASE64.encode(payload));
        let bytes = extract_image_bytes(&uri, 1024 * 1024).await.unwrap();
        assert_eq!(bytes, payload);
    }

    #[tokio::test]
    async fn blocks_ssrf_loopback_private_and_metadata_urls() {
        for url in [
            "http://127.0.0.1:5000/api/settings",
            "http://localhost:5000/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/admin",
            "http://192.168.1.1/",
            "http://[::1]:5000/",
            "http://[::ffff:127.0.0.1]:5000/",
            "http://100.64.0.1/",
            "file:///etc/passwd",
        ] {
            let err = extract_image_bytes(url, 1024 * 1024).await.unwrap_err();
            assert!(
                matches!(err, ImageExtractError::Invalid(_)),
                "Expected rejection for {}",
                url
            );
        }
    }
}
