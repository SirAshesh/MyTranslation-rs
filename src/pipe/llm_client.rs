use crate::model::{local_server_url, AppConfig, Bubble};
use crate::pipe::prompts::{build_scanlation_system_prompt, build_translation_user_prompt};
use crate::utilities::{redact_secrets, resolve_claude_url, resolve_openai_url};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
}

struct ChatEndpoint {
    label: &'static str,
    url: String,
    api_key: Option<String>,
    model: String,
    is_local: bool,
    temperature: f32,
    unreachable_hint: &'static str,
}

#[derive(Serialize)]
struct OpenAiChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Deserialize)]
struct ChatResponseMessage {
    content: Option<String>,
}

#[derive(Serialize)]
struct ClaudeMessageRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

#[derive(Deserialize)]
struct ClaudeMessageResponse {
    content: Vec<ClaudeContentBlock>,
}

#[derive(Deserialize)]
struct ClaudeContentBlock {
    text: String,
}

#[derive(Deserialize)]
struct TranslationWrapper {
    translations: Vec<TranslationItem>,
}

#[derive(Deserialize)]
struct TranslationItem {
    id: usize,
    target: String,
}

fn resolve_chat_endpoint(cfg: &AppConfig) -> Result<ChatEndpoint, String> {
    if cfg.translation_mode == "local" {
        return Ok(ChatEndpoint {
            label: "Local model server",
            url: resolve_openai_url(&local_server_url()),
            api_key: None,
            model: cfg.local_model.trim().to_string(),
            is_local: true,
            temperature: 0.1,
            unreachable_hint: " Start the model from Settings > Local Model first.",
        });
    }

    let api_key = cfg.deepseek_api_key.trim();
    if api_key.is_empty() {
        return Err("OpenAI-compatible API key is not configured. Open Settings in the web interface to add your key.".to_string());
    }
    let model = cfg.openai_model.trim();

    Ok(ChatEndpoint {
        label: "OpenAI-compatible endpoint",
        url: resolve_openai_url(&cfg.openai_base_url),
        api_key: Some(api_key.to_string()),
        model: if model.is_empty() { "deepseek-chat" } else { model }.to_string(),
        is_local: false,
        temperature: 0.3,
        unreachable_hint: "",
    })
}

fn strip_reasoning_block(raw_content: &str) -> &str {
    match raw_content.rfind("</think>") {
        Some(end) => &raw_content[end + "</think>".len()..],
        None => raw_content,
    }
}

fn parse_translation_json(raw_content: &str) -> HashMap<usize, String> {
    let mut translation_map = HashMap::new();
    let trimmed = strip_reasoning_block(raw_content)
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if let Ok(wrapper) = serde_json::from_str::<TranslationWrapper>(trimmed) {
        for item in wrapper.translations {
            translation_map.insert(item.id, item.target);
        }
        return translation_map;
    }

    if let Ok(items) = serde_json::from_str::<Vec<TranslationItem>>(trimmed) {
        for item in items {
            translation_map.insert(item.id, item.target);
        }
        return translation_map;
    }

    if let Ok(obj) = serde_json::from_str::<HashMap<String, serde_json::Value>>(trimmed) {
        for (k, v) in obj {
            if let (Ok(id), Some(s)) = (k.parse::<usize>(), v.as_str()) {
                translation_map.insert(id, s.to_string());
            }
        }
    }

    translation_map
}

impl LlmClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(90))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    pub fn http_client(&self) -> &Client {
        &self.client
    }

    async fn send_chat(
        &self,
        url: &str,
        api_key: Option<&str>,
        label: &str,
        unreachable_hint: &str,
        timeout_secs: u64,
        request: &OpenAiChatRequest,
    ) -> Result<OpenAiChatResponse, (Option<u16>, String)> {
        let mut builder = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(timeout_secs))
            .json(request);
        if let Some(key) = api_key {
            builder = builder.header("Authorization", format!("Bearer {}", key));
        }

        let secrets: Vec<&str> = api_key.into_iter().collect();
        let response = builder.send().await.map_err(|e| {
            (
                None,
                redact_secrets(
                    &format!("{} request failed: {}.{}", label, e, unreachable_hint),
                    &secrets,
                ),
            )
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let code = Some(status.as_u16());
            let body = response.text().await.unwrap_or_default();
            return Err((
                code,
                redact_secrets(
                    &format!("{} returned HTTP {}: {}", label, status, body),
                    &secrets,
                ),
            ));
        }

        response.json().await.map_err(|e| {
            (
                None,
                format!("Failed to parse {} response: {}", label, e),
            )
        })
    }

    pub(crate) async fn ping_openai_compatible(
        &self,
        url: &str,
        api_key: Option<&str>,
        model: &str,
        label: &str,
        unreachable_hint: &str,
        timeout_secs: u64,
    ) -> Result<(), (Option<u16>, String)> {
        let request = OpenAiChatRequest {
            model: model.to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "ping".to_string(),
            }],
            temperature: 0.1,
            max_tokens: 5,
            response_format: None,
        };
        self.send_chat(url, api_key, label, unreachable_hint, timeout_secs, &request)
            .await
            .map(|_| ())
    }

    pub(crate) async fn send_openai_compatible_translation(
        &self,
        url: &str,
        api_key: Option<&str>,
        model: &str,
        system_prompt: &str,
        temperature: f32,
        bubbles: &[Bubble],
        source_lang: &str,
        target_lang: &str,
        label: &str,
        unreachable_hint: &str,
        timeout_secs: u64,
    ) -> Result<HashMap<usize, String>, (Option<u16>, String)> {
        let request = OpenAiChatRequest {
            model: model.to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: build_translation_user_prompt(bubbles, source_lang, target_lang),
                },
            ],
            temperature,
            max_tokens: 2048,
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
        };

        let chat_response = self
            .send_chat(url, api_key, label, unreachable_hint, timeout_secs, &request)
            .await?;
        let raw_content = chat_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or_default();

        Ok(parse_translation_json(raw_content))
    }

    #[cfg(feature = "free-mode")]
    pub(crate) fn comic_system_prompt(&self, source_lang: &str, target_lang: &str) -> String {
        build_scanlation_system_prompt(source_lang, target_lang, false)
    }

    pub async fn test_connection_chat(&self, cfg: &AppConfig) -> Result<String, String> {
        let endpoint = resolve_chat_endpoint(cfg)?;
        self.ping_openai_compatible(
            &endpoint.url,
            endpoint.api_key.as_deref(),
            &endpoint.model,
            endpoint.label,
            endpoint.unreachable_hint,
            90,
        )
        .await
        .map_err(|(_, msg)| msg)?;
        Ok(format!("Connected to {} (model: {}).", endpoint.url, endpoint.model))
    }

    pub async fn translate_chat(
        &self,
        cfg: &AppConfig,
        bubbles: &[Bubble],
        source_lang: &str,
        target_lang: &str,
    ) -> Result<HashMap<usize, String>, String> {
        let endpoint = resolve_chat_endpoint(cfg)?;
        let system_prompt =
            build_scanlation_system_prompt(source_lang, target_lang, endpoint.is_local);
        self.send_openai_compatible_translation(
            &endpoint.url,
            endpoint.api_key.as_deref(),
            &endpoint.model,
            &system_prompt,
            endpoint.temperature,
            bubbles,
            source_lang,
            target_lang,
            endpoint.label,
            endpoint.unreachable_hint,
            90,
        )
        .await
        .map_err(|(_, msg)| msg)
    }

    pub async fn test_connection_claude(&self, cfg: &AppConfig) -> Result<String, String> {
        let key = cfg.claude_api_key.trim();
        if key.is_empty() {
            return Err("Claude API key is empty.".to_string());
        }

        let url = resolve_claude_url(&cfg.claude_base_url);
        let model = if cfg.claude_model.trim().is_empty() {
            "claude-3-5-sonnet-latest".to_string()
        } else {
            cfg.claude_model.trim().to_string()
        };

        let req_body = ClaudeMessageRequest {
            model: model.clone(),
            max_tokens: 10,
            system: "Respond with pong".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "ping".to_string(),
            }],
            temperature: 0.1,
        };

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("Claude network error: {}", e))?;

        if resp.status().is_success() {
            Ok(format!("Connected to {} (model: {}).", url, model))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Claude endpoint returned HTTP {}: {}", status, body))
        }
    }

    pub async fn translate_claude(
        &self,
        cfg: &AppConfig,
        bubbles: &[Bubble],
        source_lang: &str,
        target_lang: &str,
    ) -> Result<HashMap<usize, String>, String> {
        if cfg.claude_api_key.trim().is_empty() {
            return Err("Claude API key is not configured. Open Settings in the web interface to add your key.".to_string());
        }

        let url = resolve_claude_url(&cfg.claude_base_url);
        let model = if cfg.claude_model.trim().is_empty() {
            "claude-3-5-sonnet-latest".to_string()
        } else {
            cfg.claude_model.trim().to_string()
        };

        let req_body = ClaudeMessageRequest {
            model,
            max_tokens: 2048,
            system: build_scanlation_system_prompt(source_lang, target_lang, false),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: build_translation_user_prompt(bubbles, source_lang, target_lang),
            }],
            temperature: 0.3,
        };

        let resp = self
            .client
            .post(&url)
            .header("x-api-key", cfg.claude_api_key.trim())
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("Claude request error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Claude endpoint returned HTTP {}: {}", status, body));
        }

        let claude_res: ClaudeMessageResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Claude response JSON: {}", e))?;

        let raw_content = claude_res
            .content
            .first()
            .map(|c| c.text.as_str())
            .unwrap_or_default();

        Ok(parse_translation_json(raw_content))
    }
}
