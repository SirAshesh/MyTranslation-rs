use crate::model::{AppConfig, Bubble, SaveSettingsForm, TestKeyRequest};
use super::llm_client::LlmClient;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Translator {
    client: LlmClient,
    config: Arc<RwLock<AppConfig>>,
}

impl Translator {
    pub fn new(initial_config: AppConfig) -> Self {
        Self {
            client: LlmClient::new(),
            config: Arc::new(RwLock::new(initial_config)),
        }
    }

    pub async fn get_config(&self) -> AppConfig {
        self.config.read().await.clone()
    }

    pub async fn apply_settings(&self, form: &SaveSettingsForm) -> AppConfig {
        let mut cfg = self.config.write().await;

        let mode = form.translation_mode.trim().to_lowercase();
        if mode == "openai" || mode == "claude" {
            cfg.translation_mode = mode;
        }

        if !form.openai_base_url.trim().is_empty() {
            cfg.openai_base_url = form.openai_base_url.trim().to_string();
        }
        if !form.openai_model.trim().is_empty() {
            cfg.openai_model = form.openai_model.trim().to_string();
        }
        if !form.deepseek_api_key.trim().is_empty() {
            cfg.deepseek_api_key = form.deepseek_api_key.trim().to_string();
        }

        if !form.claude_base_url.trim().is_empty() {
            cfg.claude_base_url = form.claude_base_url.trim().to_string();
        }
        if !form.claude_model.trim().is_empty() {
            cfg.claude_model = form.claude_model.trim().to_string();
        }
        if !form.claude_api_key.trim().is_empty() {
            cfg.claude_api_key = form.claude_api_key.trim().to_string();
        }

        cfg.clone()
    }

    pub async fn is_active_configured(&self) -> bool {
        let cfg = self.config.read().await;
        match cfg.translation_mode.as_str() {
            "claude" => !cfg.claude_api_key.trim().is_empty(),
            _ => !cfg.deepseek_api_key.trim().is_empty(),
        }
    }

    pub async fn test_connection(&self, req: &TestKeyRequest) -> Result<String, String> {
        let mut temp_cfg = self.config.read().await.clone();

        if !req.translation_mode.trim().is_empty() {
            temp_cfg.translation_mode = req.translation_mode.trim().to_lowercase();
        }
        if !req.openai_base_url.trim().is_empty() {
            temp_cfg.openai_base_url = req.openai_base_url.trim().to_string();
        }
        if !req.openai_model.trim().is_empty() {
            temp_cfg.openai_model = req.openai_model.trim().to_string();
        }
        if !req.api_key.trim().is_empty() {
            temp_cfg.deepseek_api_key = req.api_key.trim().to_string();
        }
        if !req.claude_base_url.trim().is_empty() {
            temp_cfg.claude_base_url = req.claude_base_url.trim().to_string();
        }
        if !req.claude_model.trim().is_empty() {
            temp_cfg.claude_model = req.claude_model.trim().to_string();
        }
        if !req.claude_api_key.trim().is_empty() {
            temp_cfg.claude_api_key = req.claude_api_key.trim().to_string();
        }

        match temp_cfg.translation_mode.as_str() {
            "claude" => self.client.test_connection_claude(&temp_cfg).await,
            _ => self.client.test_connection_openai(&temp_cfg).await,
        }
    }

    pub async fn translate_bubbles(
        &self,
        bubbles: &[Bubble],
        _source_lang: &str,
        _target_lang: &str,
    ) -> Result<HashMap<usize, String>, String> {
        let cfg = self.config.read().await.clone();
        match cfg.translation_mode.as_str() {
            "claude" => self.client.translate_claude(&cfg, bubbles).await,
            _ => self.client.translate_openai(&cfg, bubbles).await,
        }
    }
}
