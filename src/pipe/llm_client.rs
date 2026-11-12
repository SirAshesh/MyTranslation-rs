use crate::model::{local_server_url, AppConfig, Bubble};
use crate::utilities::{resolve_claude_url, resolve_openai_url};
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
    system_prompt: &'static str,
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

fn scanlation_system_prompt() -> &'static str {
    r#"You are a professional comic and manga scanlation translator.
Your task is to translate comic dialogue into natural, fluent, and expressive Indonesian suited for conversational comic panels.

SCANLATION QUALITY STANDARDS:
1. CONCISE & FIT FOR SPEECH BUBBLES: Comic bubble space is strictly limited. Keep translations punchy and compact without omitting essential meaning.
2. AVOID REDUNDANCY: Do not repeat identical subjects or objects within the same speech bubble.
3. NATURAL CONVERSATIONAL TONE: Use modern, natural Indonesian comic dialogue (aku/kamu, udah, nggak, gimana, kayak, kok, sih, dong, ya). Avoid stiff, formal textbook language or literal machine translation.
4. FIX OCR MERGED WORDS:
   Input text originates from comic OCR which frequently drops spaces due to tight lettering.
   - "anda love potion" / "alovepotion" -> means "and a love potion" / "a love potion" -> Translate as "dan ramuan cinta..." / "ramuan cinta" (NEVER translate "anda" as the pronoun "you").
   - "imiss you" -> means "I miss you" -> Translate as "aku kangen kamu".
   - "Harij-san" / "HarijuSan" / "Hariju-san" -> Character name is "Harij-san" (or Tuan Harij).
   - "ICAN'TEVEN", "ITWASALL ANILLUSION", "IFSIRHARIJ", "ISTILLVIVIDLY", "THATOCCURRED" -> Separate words cleanly and understand their meaning accurately.
   ENSURE ALL WORDS IN INDONESIAN ARE PROPERLY SEPARATED BY SPACES WITH NO MERGED WORDS!
5. COMMON IDIOMATIC EXPRESSIONS:
   - "Excuse me?" / "Excuse me!" is a polite query ("Permisi?") or an affronted reaction ("Apa katamu?!" / "Maksudmu?!"). NEVER translate literally as "Alasan me".
6. PRESERVE EMOTIONAL PUNCTUATION: Maintain ellipsis (...), question marks (?), exclamation marks (!), and character expressiveness.

OUTPUT FORMAT MUST BE A VALID JSON OBJECT WITH A 'translations' KEY:
{
  "translations": [
    {"id": 1, "target": "Tapi kupikir dia udah dapet apa yang dia pesan?"},
    {"id": 2, "target": "Makasih udah ngizinin aku masuk."}
  ]
}"#
}

fn local_scanlation_system_prompt() -> &'static str {
    r#"You are an expert manga and comic scanlation translator translating dialogue into natural Indonesian.
Task: Translate comic dialogue into fluent, expressive Indonesian that understands honorifics and social hierarchy.

HONORIFIC & SCANLATION RULES:
1. HONORIFICS & FORMS OF ADDRESS:
   - Preserve Japanese honorific suffixes (-san, -kun, -chan, -sama, -senpai, -sensei) if they are part of character identity (e.g., "Harij-san", "Karin-chan", "Senpai").
   - Alternatively, adapt naturally to social hierarchy: "Tuan [Nama]" (for nobles/masters), "Nona [Nama]", "Kak [Nama]".
   - "Excuse me?" / "Excuse me!" -> "Permisi?" or shock reaction "Maksudmu apa?!" (NEVER translate as "Alasan me").
2. NATURAL COMIC REGISTER:
   - Close peers/friends: Use "aku/kamu" and casual conversational words (udah, nggak, gimana, kok, sih, dong, ya).
   - Avoid stiff, formal textbook phrasing ("Saya/Anda" ONLY when speaking to nobles, royalty, or high officials).
3. FIX OCR MERGED TEXT:
   - Separate concatenated OCR words (e.g., "anda love potion" means "and a love potion" -> "dan ramuan cinta", NOT the pronoun "anda").
4. CONCISE & PANEL-FITTING:
   - Keep dialogue concise and punchy to fit neatly inside speech bubbles.
5. REQUIRED JSON OUTPUT FORMAT:
{
  "translations": [
    {"id": 1, "target": "Natural Indonesian translation"}
  ]
}"#
}

fn build_translation_user_prompt(bubbles: &[Bubble]) -> String {
    let input_items: Vec<serde_json::Value> = bubbles
        .iter()
        .map(|b| serde_json::json!({ "id": b.id, "text": b.source_text }))
        .collect();

    format!(
        "Translate the following comic dialogue items into natural Indonesian:\n{}",
        serde_json::to_string_pretty(&input_items).unwrap_or_default()
    )
}

fn resolve_chat_endpoint(cfg: &AppConfig) -> Result<ChatEndpoint, String> {
    if cfg.translation_mode == "local" {
        return Ok(ChatEndpoint {
            label: "Local model server",
            url: resolve_openai_url(&local_server_url()),
            api_key: None,
            model: cfg.local_model.trim().to_string(),
            system_prompt: local_scanlation_system_prompt(),
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
        system_prompt: scanlation_system_prompt(),
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

    async fn send_chat(
        &self,
        endpoint: &ChatEndpoint,
        request: &OpenAiChatRequest,
    ) -> Result<OpenAiChatResponse, String> {
        let mut builder = self
            .client
            .post(&endpoint.url)
            .header("Content-Type", "application/json")
            .json(request);
        if let Some(api_key) = &endpoint.api_key {
            builder = builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = builder.send().await.map_err(|e| {
            format!("{} request failed: {}.{}", endpoint.label, e, endpoint.unreachable_hint)
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("{} returned HTTP {}: {}", endpoint.label, status, body));
        }

        response
            .json()
            .await
            .map_err(|e| format!("Failed to parse {} response: {}", endpoint.label, e))
    }

    pub async fn test_connection_chat(&self, cfg: &AppConfig) -> Result<String, String> {
        let endpoint = resolve_chat_endpoint(cfg)?;
        let request = OpenAiChatRequest {
            model: endpoint.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "ping".to_string(),
            }],
            temperature: 0.1,
            max_tokens: 5,
            response_format: None,
        };

        self.send_chat(&endpoint, &request).await?;
        Ok(format!("Connected to {} (model: {}).", endpoint.url, endpoint.model))
    }

    pub async fn translate_chat(
        &self,
        cfg: &AppConfig,
        bubbles: &[Bubble],
    ) -> Result<HashMap<usize, String>, String> {
        let endpoint = resolve_chat_endpoint(cfg)?;
        let request = OpenAiChatRequest {
            model: endpoint.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: endpoint.system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: build_translation_user_prompt(bubbles),
                },
            ],
            temperature: endpoint.temperature,
            max_tokens: 2048,
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
        };

        let chat_response = self.send_chat(&endpoint, &request).await?;
        let raw_content = chat_response
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .unwrap_or_default();

        Ok(parse_translation_json(raw_content))
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
            system: scanlation_system_prompt().to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: build_translation_user_prompt(bubbles),
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
