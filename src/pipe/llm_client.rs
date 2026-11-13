use crate::model::{local_server_url, AppConfig, Bubble};
use crate::pipe::llm_wire::{
    extract_claude_usage, extract_openai_usage, parse_translation_json, ChatMessage,
    ClaudeCacheControl, ClaudeCachedMessageRequest, ClaudeMessageRequest, ClaudeMessageResponse,
    ClaudeSystemBlock, OpenAiChatRequest, OpenAiChatResponse, ResponseFormat,
};
use crate::pipe::prompts::{build_translation_user_prompt, initial_system_prompt};
use crate::utilities::{redact_secrets, resolve_claude_url, resolve_openai_url, LlmUsageStats};
use reqwest::Client;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
}

#[derive(Clone, Debug, Default)]
pub struct LlmTranslationOutput {
    pub translations: HashMap<usize, String>,
    pub usage: LlmUsageStats,
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
    ) -> Result<LlmTranslationOutput, (Option<u16>, String)> {
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
        let usage = extract_openai_usage(&chat_response);
        let raw_content = chat_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or_default();

        Ok(LlmTranslationOutput {
            translations: parse_translation_json(raw_content),
            usage,
        })
    }

    #[cfg(feature = "free-mode")]
    pub(crate) fn comic_system_prompt(&self) -> &'static str {
        initial_system_prompt(false)
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
    ) -> Result<LlmTranslationOutput, String> {
        let endpoint = resolve_chat_endpoint(cfg)?;
        let system_prompt = initial_system_prompt(endpoint.is_local);
        self.send_openai_compatible_translation(
            &endpoint.url,
            endpoint.api_key.as_deref(),
            &endpoint.model,
            system_prompt,
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
    ) -> Result<LlmTranslationOutput, String> {
        let api_key = cfg.claude_api_key.trim();
        if api_key.is_empty() {
            return Err("Claude API key is not configured. Open Settings in the web interface to add your key.".to_string());
        }

        let url = resolve_claude_url(&cfg.claude_base_url);
        let model = if cfg.claude_model.trim().is_empty() {
            "claude-3-5-sonnet-latest".to_string()
        } else {
            cfg.claude_model.trim().to_string()
        };

        let system_prompt = initial_system_prompt(false);
        let user_messages = vec![ChatMessage {
            role: "user".to_string(),
            content: build_translation_user_prompt(bubbles, source_lang, target_lang),
        }];

        let cached_req = ClaudeCachedMessageRequest {
            model: &model,
            max_tokens: 2048,
            system: vec![ClaudeSystemBlock {
                block_type: "text",
                text: system_prompt,
                cache_control: ClaudeCacheControl {
                    control_type: "ephemeral",
                },
            }],
            messages: &user_messages,
            temperature: 0.3,
        };

        let mut resp = self
            .client
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&cached_req)
            .send()
            .await
            .map_err(|e| format!("Claude request error: {}", e))?;

        if resp.status().as_u16() == 400 {
            let err_body = resp.text().await.unwrap_or_default();
            if err_body.to_ascii_lowercase().contains("system")
                || err_body.to_ascii_lowercase().contains("cache_control")
            {
                let plain_req = ClaudeMessageRequest {
                    model,
                    max_tokens: 2048,
                    system: system_prompt.to_string(),
                    messages: user_messages,
                    temperature: 0.3,
                };
                resp = self
                    .client
                    .post(&url)
                    .header("x-api-key", api_key)
                    .header("anthropic-version", "2023-06-01")
                    .header("Content-Type", "application/json")
                    .json(&plain_req)
                    .send()
                    .await
                    .map_err(|e| format!("Claude request error: {}", e))?;
            } else {
                return Err(format!("Claude endpoint returned HTTP 400: {}", err_body));
            }
        }

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Claude endpoint returned HTTP {}: {}", status, body));
        }

        let claude_res: ClaudeMessageResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse Claude response JSON: {}", e))?;

        let usage = extract_claude_usage(claude_res.usage.as_ref());
        let raw_content = claude_res
            .content
            .first()
            .map(|c| c.text.as_str())
            .unwrap_or_default();

        Ok(LlmTranslationOutput {
            translations: parse_translation_json(raw_content),
            usage,
        })
    }
}
