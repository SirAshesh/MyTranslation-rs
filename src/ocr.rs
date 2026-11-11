use crate::models::{OcrDaemonResponse, OcrLineItem};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

pub struct OcrManager {
    daemon_process: Option<Child>,
    client: reqwest::Client,
    ocr_url: String,
    health_url: String,
    is_ready: Arc<AtomicBool>,
}

impl OcrManager {
    pub fn new(port: u16) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .unwrap_or_default();

        Self {
            daemon_process: None,
            client,
            ocr_url: format!("http://127.0.0.1:{}/ocr", port),
            health_url: format!("http://127.0.0.1:{}/health", port),
            is_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn ensure_started(&mut self) -> Result<(), String> {
        if self.check_health().await {
            info!("OCR daemon is already running and healthy.");
            self.is_ready.store(true, Ordering::SeqCst);
            return Ok(());
        }

        info!("Starting Python OCR daemon on 127.0.0.1:51055...");
        let script_path = std::path::Path::new("ocr_daemon.py");
        let script_file = if script_path.exists() {
            "ocr_daemon.py"
        } else {
            "Server-rs/ocr_daemon.py"
        };

        let child = Command::new("python")
            .arg(script_file)
            .spawn()
            .map_err(|e| format!("Failed to spawn python {}: {}", script_file, e))?;

        self.daemon_process = Some(child);

        for attempt in 1..=24 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if self.check_health().await {
                info!("OCR daemon initialized and ready on attempt {}", attempt);
                self.is_ready.store(true, Ordering::SeqCst);
                return Ok(());
            }
        }

        Err("OCR daemon failed to start or become healthy within 12 seconds".to_string())
    }

    async fn check_health(&self) -> bool {
        match self.client.get(&self.health_url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn run_ocr(&self, image_bytes: Vec<u8>) -> Result<Vec<OcrLineItem>, String> {
        let resp = self
            .client
            .post(&self.ocr_url)
            .body(image_bytes)
            .send()
            .await
            .map_err(|e| format!("OCR request network error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OCR daemon returned HTTP {}: {}", status, body));
        }

        let ocr_data: OcrDaemonResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse OCR response JSON: {}", e))?;

        if !ocr_data.success {
            return Err("OCR daemon indicated failure".to_string());
        }

        Ok(ocr_data.lines)
    }
}

impl Drop for OcrManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.daemon_process.take() {
            info!("Shutting down OCR daemon process...");
            let _ = child.kill();
        }
    }
}
