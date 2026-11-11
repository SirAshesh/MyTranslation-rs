use crate::models::Bubble;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;
use tracing::warn;

#[derive(Clone)]
pub struct DeepSeekClient {
    client: reqwest::Client,
    api_key: String,
    api_url: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
    response_format: ResponseFormat,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    r#type: String,
}

#[derive(Deserialize)]
struct ChatResponse {
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

#[derive(Serialize, Deserialize)]
struct TranslationWrapper {
    #[serde(default)]
    translations: Vec<TranslationItem>,
}

#[derive(Serialize, Deserialize)]
struct TranslationItem {
    id: usize,
    target: String,
}

impl DeepSeekClient {
    pub fn new(api_key: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(25))
            .build()
            .unwrap_or_default();

        Self {
            client,
            api_key,
            api_url: "https://api.deepseek.com/chat/completions".to_string(),
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

        let input_items: Vec<serde_json::Value> = bubbles
            .iter()
            .map(|b| {
                serde_json::json!({
                    "id": b.id,
                    "text": b.source_text
                })
            })
            .collect();

        let system_prompt = r#"Kamu adalah penerjemah komik / manga profesional kelas atas (scanlator profesional).
Tugasmu menerjemahkan teks dialog komik berbahasa Inggris ke Bahasa Indonesia yang SANGAT ALAMI, luwes, ekspresif, dan sesuai konteks percakapan komik.

ATURAN DAN STANDAR KUALITAS SCANLATION:
1. RINGKAS DAN PAS DI BALON KOMIK: Ruang balon komik sangat terbatas. Terjemahkan secara padat dan tidak bertele-tele tanpa memotong makna.
2. HINDARI PENGULANGAN KATA / REDUNDANSI: Jangan mengulang subjek atau objek yang sama dalam 1 balon. (Contoh scanlation alami: "...sebelum kamu minum ramuan itu, kamu sempat bilang kalau nggak begitu ngefek ke penyihir.").
3. GAYA BAHASA ALAMI: Gunakan bahasa percakapan komik modern yang santai dan luwes (aku/kamu, udah, nggak/ngga, gimana, kayak, kok, sih, dong, ya). Jangan gunakan bahasa baku kaku ala buku sekolah atau terjemahan mesin harfiah.
4. PERBAIKI KATA DAN SEGMENTASI HASIL OCR:
   - "anda love potion" / "anda love" adalah kesalahan OCR dari "and a love potion" / "and a love" -> Terjemahkan "dan ramuan cinta..." (KATA "anda" DI SINI ADALAH "and a", BUKAN KATA GANTI "Anda/kamu").
   - "imiss you" / "imiss" -> "I miss you" (Terjemahkan: "Aku kangen kamu" / "Aku merindukanmu").
   - "Harij-san" / "HarijuSan" / "Hariju-san" -> Nama karakter adalah "Harij-san" (atau Tuan Harij), jangan ubah nama atau memelintir gelar kehormatannya.
5. UNGKAPAN UMUM KOMIK:
   - "Excuse me?" / "Excuse me!" adalah ungkapan sopan ("Permisi?") atau reaksi kaget/tersinggung ("Apa katamu?!" / "Maksudmu?!"). JANGAN PERNAH terjemahkan menjadi "Alasan me".
6. PERTAHANKAN TANDA BACA EMOSI: Pertahankan tanda elipsis (...), tanda tanya (?), tanda seru (!), dan nuansa ekspresi karakter.

FORMAT OUTPUT WAJIB BERUPA JSON OBJECT dengan kunci 'translations':
{
  "translations": [
    {"id": 1, "target": "Tapi kupikir dia udah dapet apa yang dia pesan?"},
    {"id": 2, "target": "Makasih udah ngizinin aku masuk."}
  ]
}"#;

        let user_prompt = format!(
            "Terjemahkan dialog-dialog komik berikut ke Bahasa Indonesia alami:\n{}",
            serde_json::to_string_pretty(&input_items).unwrap_or_default()
        );

        let req_body = ChatRequest {
            model: "deepseek-chat".to_string(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt,
                },
            ],
            temperature: 0.3,
            max_tokens: 2048,
            response_format: ResponseFormat {
                r#type: "json_object".to_string(),
            },
        };

        let resp = self
            .client
            .post(&self.api_url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&req_body)
            .send()
            .await
            .map_err(|e| format!("DeepSeek request network error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("DeepSeek API returned HTTP {}: {}", status, body));
        }

        let chat_res: ChatResponse = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse DeepSeek response JSON: {}", e))?;

        let raw_content = chat_res
            .choices
            .first()
            .map(|c| c.message.content.clone())
            .unwrap_or_default();

        let mut translation_map = HashMap::new();

        if let Ok(wrapper) = serde_json::from_str::<TranslationWrapper>(&raw_content) {
            for item in wrapper.translations {
                translation_map.insert(item.id, item.target);
            }
        }

        if translation_map.is_empty() {
            if let Ok(arr) = serde_json::from_str::<Vec<TranslationItem>>(&raw_content) {
                for item in arr {
                    translation_map.insert(item.id, item.target);
                }
            }
        }

        if translation_map.is_empty() {
            warn!("Using regex fallback to extract translations from DeepSeek output");
            let re = regex::Regex::new(r#"\{\s*"id"\s*:\s*(\d+)\s*,\s*"target"\s*:\s*"([^"]+)"\s*\}"#).unwrap();
            for cap in re.captures_iter(&raw_content) {
                if let (Ok(id), target) = (cap[1].parse::<usize>(), &cap[2]) {
                    translation_map.insert(id, target.to_string());
                }
            }
        }

        Ok(translation_map)
    }
}
