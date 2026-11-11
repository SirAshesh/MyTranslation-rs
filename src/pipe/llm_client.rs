use crate::model::{AppConfig, Bubble};
use crate::utilities::{resolve_claude_url, resolve_openai_url};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone)]
pub struct LlmClient {
    client: Client,
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
    content: String,
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
    r#"Kamu adalah penerjemah komik / manga profesional kelas atas (scanlator profesional).
Tugasmu menerjemahkan teks dialog komik berbahasa Inggris ke Bahasa Indonesia yang SANGAT ALAMI, luwes, ekspresif, dan sesuai konteks percakapan komik.

ATURAN DAN STANDAR KUALITAS SCANLATION:
1. RINGKAS DAN PAS DI BALON KOMIK: Ruang balon komik sangat terbatas. Terjemahkan secara padat dan tidak bertele-tele tanpa memotong makna.
2. HINDARI PENGULANGAN KATA / REDUNDANSI: Jangan mengulang subjek atau objek yang sama dalam 1 balon. (Contoh scanlation alami: "...sebelum kamu minum ramuan itu, kamu sempat bilang kalau nggak begitu ngefek ke penyihir.").
3. GAYA BAHASA ALAMI: Gunakan bahasa percakapan komik modern yang santai dan luwes (aku/kamu, udah, nggak/ngga, gimana, kayak, kok, sih, dong, ya). Jangan gunakan bahasa baku kaku ala buku sekolah atau terjemahan mesin harfiah.
4. KOREKSI KATA MENEMPEL / TANPA SPASI DARI OCR:
   Teks input bahasa Inggris berasal dari OCR manga yang sering menghilangkan spasi antar kata karena font yang rapat.
   - "anda love potion" / "alovepotion" -> maksudnya "and a love potion" / "a love potion" -> Terjemahkan: "dan ramuan cinta..." / "ramuan cinta." (JANGAN terjemahkan "anda" sebagai "kamu"!).
   - "imiss you" -> maksudnya "I miss you" -> Terjemahkan: "aku kangen kamu".
   - "Harij-san" / "HarijuSan" / "Hariju-san" -> Nama karakter adalah "Harij-san" (atau Tuan Harij).
   - "ICAN'TEVEN", "ITWASALL ANILLUSION", "IFSIRHARIJ", "ISTILLVIVIDLY", "THATOCCURRED" -> Pisahkan kata-katanya dan pahami maknanya dengan benar.
   PASTIKAN PADA HASIL TERJEMAHAN BAHASA INDONESIA SEMUA KATA TERPISAH DENGAN SPASI YANG TEPAT DAN TIDAK ADA KATA YANG MENEMPEL!
5. UNGKAPAN UMUM KOMIK:
   - "Excuse me?" / "Excuse me!" adalah ungkapan sopan ("Permisi?") atau reaksi kaget/tersinggung ("Apa katamu?!" / "Maksudmu?!"). JANGAN PERNAH terjemahkan menjadi "Alasan me".
6. PERTAHANKAN TANDA BACA EMOSI: Pertahankan tanda elipsis (...), tanda tanya (?), tanda seru (!), dan nuansa ekspresi karakter.

FORMAT OUTPUT WAJIB BERUPA JSON OBJECT dengan kunci 'translations':
{
  "translations": [
    {"id": 1, "target": "Tapi kupikir dia udah dapet apa yang dia pesan?"},
    {"id": 2, "target": "Makasih udah ngizinin aku masuk."}
  ]
}"#
}

fn parse_translation_json(raw_content: &str) -> HashMap<usize, String> {
    let mut translation_map = HashMap::new();
    let trimmed = raw_content
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
            if let Ok(id) = k.parse::<usize>() {
                if let Some(s) = v.as_str() {
                    translation_map.insert(id, s.to_string());
                }
            }
        }
    }

    translation_map
}

impl LlmClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(45))
            .build()
            .unwrap_or_else(|_| Client::new());
        Self { client }
    }

    pub async fn test_connection_openai(&self, cfg: &AppConfig) -> Result<String, String> {
        let key = cfg.deepseek_api_key.trim();
        if key.is_empty() {
            return Err("OpenAI API key is empty.".to_string());
        }

        let url = resolve_openai_url(&cfg.openai_base_url);
        let model = if cfg.openai_model.trim().is_empty() {
            "deepseek-chat".to_string()
        } else {
            cfg.openai_model.trim().to_string()
        };

        let req_body = OpenAiChatRequest {
            model: model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "ping".to_string(),
            }],
            temperature: 0.1,
            max_tokens: 5,
            response_format: None,
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if resp.status().is_success() {
            Ok(format!("Connected to {} (model: {}).", url, model))
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("Endpoint returned HTTP {}: {}", status, body))
        }
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

    pub async fn translate_openai(
        &self,
        cfg: &AppConfig,
        bubbles: &[Bubble],
    ) -> Result<HashMap<usize, String>, String> {
        if cfg.deepseek_api_key.trim().is_empty() {
            return Err("OpenAI-compatible API key is not configured. Open Settings in the web interface to add your key.".to_string());
        }

        let input_items: Vec<serde_json::Value> = bubbles
            .iter()
            .map(|b| serde_json::json!({ "id": b.id, "text": b.source_text }))
            .collect();

        let user_prompt = format!(
            "Terjemahkan dialog-dialog komik berikut ke Bahasa Indonesia alami:\n{}",
            serde_json::to_string_pretty(&input_items).unwrap_or_default()
        );

        let url = resolve_openai_url(&cfg.openai_base_url);
        let model = if cfg.openai_model.trim().is_empty() {
            "deepseek-chat".to_string()
        } else {
            cfg.openai_model.trim().to_string()
        };

        let req_body = OpenAiChatRequest {
            model,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: scanlation_system_prompt().to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
            temperature: 0.3,
            max_tokens: 2048,
            response_format: Some(ResponseFormat {
                r#type: "json_object".to_string(),
            }),
        };

        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", cfg.deepseek_api_key.trim()))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("OpenAI-compatible request error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Endpoint returned HTTP {}: {}", status, body));
        }

        let chat_res: OpenAiChatResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse response JSON: {}", e))?;

        let raw_content = chat_res
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        Ok(parse_translation_json(&raw_content))
    }

    pub async fn translate_claude(
        &self,
        cfg: &AppConfig,
        bubbles: &[Bubble],
    ) -> Result<HashMap<usize, String>, String> {
        if cfg.claude_api_key.trim().is_empty() {
            return Err("Claude API key is not configured. Open Settings in the web interface to add your key.".to_string());
        }

        let input_items: Vec<serde_json::Value> = bubbles
            .iter()
            .map(|b| serde_json::json!({ "id": b.id, "text": b.source_text }))
            .collect();

        let user_prompt = format!(
            "Terjemahkan dialog-dialog komik berikut ke Bahasa Indonesia alami:\n{}",
            serde_json::to_string_pretty(&input_items).unwrap_or_default()
        );

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
                content: user_prompt,
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
            .map(|c| c.text.clone())
            .unwrap_or_default();

        Ok(parse_translation_json(&raw_content))
    }
}
