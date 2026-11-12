use crate::model::{AppConfig, Bubble, SaveSettingsForm, TestKeyRequest};
use super::llm_client::LlmClient;
use super::local_runtime::LocalRuntime;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Translator {
    client: LlmClient,
    config: Arc<RwLock<AppConfig>>,
    pub runtime: LocalRuntime,
}

impl Translator {
    pub fn new(initial_config: AppConfig, local_data_dir: PathBuf) -> Self {
        Self {
            client: LlmClient::new(),
            config: Arc::new(RwLock::new(initial_config)),
            runtime: LocalRuntime::new(local_data_dir),
        }
    }

    pub async fn get_config(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub async fn apply_settings(&self, form: &SaveSettingsForm) -> AppConfig {
        let mut guard = self.config.write().await;
        let cfg = &mut *guard;
        let mode = form.translation_mode.trim().to_lowercase();
        if matches!(mode.as_str(), "openai" | "claude" | "local") {
            cfg.translation_mode = mode;
        }

        let text_settings = [
            (&form.openai_base_url, &mut cfg.openai_base_url),
            (&form.openai_model, &mut cfg.openai_model),
            (&form.deepseek_api_key, &mut cfg.deepseek_api_key),
            (&form.claude_base_url, &mut cfg.claude_base_url),
            (&form.claude_model, &mut cfg.claude_model),
            (&form.claude_api_key, &mut cfg.claude_api_key),
            (&form.local_model, &mut cfg.local_model),
        ];
        for (submitted, stored) in text_settings {
            if !submitted.trim().is_empty() {
                *stored = submitted.trim().to_string();
            }
        }

        cfg.clone()
    }

    pub async fn is_active_configured(&self) -> bool {
        let cfg = self.get_config().await;
        match cfg.translation_mode.as_str() {
            "claude" => !cfg.claude_api_key.trim().is_empty(),
            "local" => self.runtime.is_running().await,
            _ => !cfg.deepseek_api_key.trim().is_empty(),
        }
    }

    pub async fn test_connection(&self, req: &TestKeyRequest) -> Result<String, String> {
        let mut temp_cfg = self.get_config().await;

        let overrides = [
            (&req.translation_mode, &mut temp_cfg.translation_mode),
            (&req.openai_base_url, &mut temp_cfg.openai_base_url),
            (&req.openai_model, &mut temp_cfg.openai_model),
            (&req.api_key, &mut temp_cfg.deepseek_api_key),
            (&req.claude_base_url, &mut temp_cfg.claude_base_url),
            (&req.claude_model, &mut temp_cfg.claude_model),
            (&req.claude_api_key, &mut temp_cfg.claude_api_key),
            (&req.local_model, &mut temp_cfg.local_model),
        ];
        for (submitted, stored) in overrides {
            if !submitted.trim().is_empty() {
                *stored = submitted.trim().to_string();
            }
        }
        temp_cfg.translation_mode = temp_cfg.translation_mode.to_lowercase();

        match temp_cfg.translation_mode.as_str() {
            "claude" => self.client.test_connection_claude(&temp_cfg).await,
            _ => self.client.test_connection_chat(&temp_cfg).await,
        }
    }

    pub async fn translate_bubbles(
        &self,
        bubbles: &[Bubble],
        _source_lang: &str,
        _target_lang: &str,
    ) -> Result<HashMap<usize, String>, String> {
        let cfg = self.get_config().await;
        match cfg.translation_mode.as_str() {
            "claude" => self.client.translate_claude(&cfg, bubbles).await,
            _ => self.client.translate_chat(&cfg, bubbles).await,
        }
    }
}
