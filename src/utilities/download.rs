use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

pub async fn download_file(
    client: &reqwest::Client,
    url: &str,
    destination: &Path,
    on_progress: impl Fn(u64, u64),
) -> Result<(), String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download request failed for {}: {}", url, e))?;

    if !response.status().is_success() {
        return Err(format!("Download of {} returned HTTP {}", url, response.status()));
    }

    let total_bytes = response.content_length().unwrap_or(0);
    let partial_path = partial_download_path(destination);
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create {}: {}", parent.display(), e))?;
    }

    let mut file = tokio::fs::File::create(&partial_path)
        .await
        .map_err(|e| format!("Failed to create {}: {}", partial_path.display(), e))?;

    let mut downloaded_bytes = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Download interrupted: {}", e))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write {}: {}", partial_path.display(), e))?;
        downloaded_bytes += chunk.len() as u64;
        on_progress(downloaded_bytes, total_bytes);
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush {}: {}", partial_path.display(), e))?;
    drop(file);

    tokio::fs::rename(&partial_path, destination)
        .await
        .map_err(|e| format!("Failed to finalize {}: {}", destination.display(), e))
}

fn partial_download_path(destination: &Path) -> PathBuf {
    let mut file_name = destination.file_name().unwrap_or_default().to_os_string();
    file_name.push(".part");
    destination.with_file_name(file_name)
}

