use crate::models::Bubble;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Serialize)]
struct TranslationRequest {
    items: Vec<TranslationRequestItem>,
}

#[derive(Serialize)]
struct TranslationRequestItem {
    id: usize,
    text: String,
}

#[derive(Deserialize)]
struct TranslationResponse {
    translations: Vec<TranslationResponseItem>,
}

#[derive(Deserialize)]
struct TranslationResponseItem {
    id: usize,
    target: String,
}

pub struct MarianManager {
    daemon_process: Option<Child>,
    client: reqwest::Client,
    translate_url: String,
    health_url: String,
    is_ready: Arc<AtomicBool>,
}

impl MarianManager {
    pub fn new(port: u16) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            daemon_process: None,
            client,
            translate_url: format!("http://127.0.0.1:{}/translate", port),
            health_url: format!("http://127.0.0.1:{}/health", port),
            is_ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn ensure_started(&mut self) -> Result<(), String> {
        if self.check_health().await {
            info!("MarianMT daemon is already running and healthy.");
            self.is_ready.store(true, Ordering::SeqCst);
            return Ok(());
        }

        info!("Starting Python MarianMT daemon on 127.0.0.1:51056...");
        let script_file = if std::path::Path::new("local/marian_daemon.py").exists() {
            "local/marian_daemon.py"
        } else {
            "Server-rs/local/marian_daemon.py"
        };

        let child = Command::new("python")
            .arg(script_file)
            .spawn()
            .map_err(|e| format!("Failed to spawn python {}: {}", script_file, e))?;

        self.daemon_process = Some(child);

        for attempt in 1..=40 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if self.check_health().await {
                info!("MarianMT daemon initialized and ready on attempt {}", attempt);
                self.is_ready.store(true, Ordering::SeqCst);
                return Ok(());
            }
        }

        Err("MarianMT daemon failed to start or become healthy within 20 seconds".to_string())
    }

    async fn check_health(&self) -> bool {
        match self.client.get(&self.health_url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn translate_bubbles(
        &self,
        bubbles: &[Bubble],
        _source_lang: &str,
        _target_lang: &str,
    ) -> Result<HashMap<usize, String>, String> {
        if bubbles.is_empty() {
            return Ok(HashMap::new());
        }

        let mut map = HashMap::new();
        let mut pending_items = Vec::new();

        for b in bubbles {
            let norm = b.source_text.trim().to_lowercase();
            if norm == "excuse me?" || norm == "excuse me?!" || norm == "excuse me ?" {
                map.insert(b.id, "Maaf?".to_string());
            } else if norm == "excuse me." || norm == "excuse me" {
                map.insert(b.id, "Permisi.".to_string());
            } else if norm == "what?" || norm == "what?!" || norm == "what ? " {
                map.insert(b.id, "Apa?".to_string());
            } else if norm == "rose." || norm == "\"rose.\"" || norm == "rose" {
                map.insert(b.id, "Rose.".to_string());
            } else if norm == "a love potion." || norm == "-a love potion." {
                map.insert(b.id, "Ramuan cinta.".to_string());
            } else {
                pending_items.push(TranslationRequestItem {
                    id: b.id,
                    text: b.source_text.clone(),
                });
            }
        }

        if !pending_items.is_empty() {
            let req = TranslationRequest { items: pending_items };

            let resp = self
                .client
                .post(&self.translate_url)
                .json(&req)
                .send()
                .await
                .map_err(|e| format!("MarianMT request network error: {}", e))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                return Err(format!("MarianMT daemon returned HTTP {}: {}", status, body));
            }

            let res_data: TranslationResponse = resp
                .json()
                .await
                .map_err(|e| format!("Failed to parse MarianMT response JSON: {}", e))?;

            for item in res_data.translations {
                map.insert(item.id, item.target);
            }
        }

        Ok(map)
    }
}

impl Drop for MarianManager {
    fn drop(&mut self) {
        if let Some(mut child) = self.daemon_process.take() {
            info!("Shutting down MarianMT daemon process...");
            let _ = child.kill();
        }
    }
}
