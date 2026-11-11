use super::domain::{default_source_lang, default_target_lang};
use serde::{Deserialize, Serialize};

fn default_port() -> u16 {
    5000
}

fn default_mode() -> String {
    "openai".to_string()
}

fn default_openai_url() -> String {
    "https://api.deepseek.com".to_string()
}

fn default_openai_model() -> String {
    "deepseek-chat".to_string()
}

fn default_claude_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_claude_model() -> String {
    "claude-3-5-sonnet-latest".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_mode")]
    pub translation_mode: String,
    #[serde(default = "default_openai_url")]
    pub openai_base_url: String,
    #[serde(default = "default_openai_model")]
    pub openai_model: String,
    #[serde(default)]
    pub deepseek_api_key: String,
    #[serde(default = "default_claude_url")]
    pub claude_base_url: String,
    #[serde(default = "default_claude_model")]
    pub claude_model: String,
    #[serde(default)]
    pub claude_api_key: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            port: default_port(),
            translation_mode: default_mode(),
            openai_base_url: default_openai_url(),
            openai_model: default_openai_model(),
            deepseek_api_key: String::new(),
            claude_base_url: default_claude_url(),
            claude_model: default_claude_model(),
            claude_api_key: String::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SaveSettingsForm {
    #[serde(default = "default_mode")]
    pub translation_mode: String,
    #[serde(default = "default_openai_url")]
    pub openai_base_url: String,
    #[serde(default = "default_openai_model")]
    pub openai_model: String,
    #[serde(default)]
    pub deepseek_api_key: String,
    #[serde(default = "default_claude_url")]
    pub claude_base_url: String,
    #[serde(default = "default_claude_model")]
    pub claude_model: String,
    #[serde(default)]
    pub claude_api_key: String,
}

#[derive(Debug, Serialize)]
pub struct SettingsResponse {
    pub success: bool,
    pub message: String,
    pub port: u16,
    pub translation_mode: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_key_configured: bool,
    pub openai_masked_key: String,
    pub claude_base_url: String,
    pub claude_model: String,
    pub claude_key_configured: bool,
    pub claude_masked_key: String,
    pub active_configured: bool,
}

#[derive(Debug, Deserialize)]
pub struct TestKeyRequest {
    #[serde(default)]
    pub translation_mode: String,
    #[serde(default)]
    pub openai_base_url: String,
    #[serde(default)]
    pub openai_model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub claude_base_url: String,
    #[serde(default)]
    pub claude_model: String,
    #[serde(default)]
    pub claude_api_key: String,
}

#[derive(Debug, Serialize)]
pub struct TestKeyResponse {
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct TextTranslateRequest {
    #[serde(default)]
    pub text: String,
    #[serde(rename = "sourceLang", default = "default_source_lang")]
    pub source_lang: String,
    #[serde(rename = "targetLang", default = "default_target_lang")]
    pub target_lang: String,
}

#[derive(Debug, Serialize)]
pub struct TextTranslateResponse {
    pub success: bool,
    pub source: String,
    pub target: String,
    pub elapsed_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}
