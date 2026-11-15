use crate::utilities::LlmUsageStats;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize)]
pub(crate) struct OpenAiModelsListResponse {
    #[serde(default)]
    pub(crate) data: Vec<OpenAiModelEntry>,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiModelEntry {
    pub(crate) id: String,
}

#[derive(Deserialize)]
struct WrappedModelsListResponse {
    #[serde(default)]
    models: Vec<OpenAiModelEntry>,
}

fn collect_unique_model_ids(entries: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut ordered = Vec::new();
    for raw in entries {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
            ordered.push(trimmed.to_string());
        }
    }
    ordered
}

pub(crate) fn parse_models_list_json(raw: &str, label: &str) -> Result<Vec<String>, String> {
    if let Ok(list) = serde_json::from_str::<OpenAiModelsListResponse>(raw) {
        if !list.data.is_empty() {
            return Ok(collect_unique_model_ids(
                list.data.into_iter().map(|entry| entry.id),
            ));
        }
        if raw.contains("\"data\"") {
            return Ok(Vec::new());
        }
    }

    if let Ok(entries) = serde_json::from_str::<Vec<OpenAiModelEntry>>(raw) {
        return Ok(collect_unique_model_ids(
            entries.into_iter().map(|entry| entry.id),
        ));
    }

    if let Ok(wrapped) = serde_json::from_str::<WrappedModelsListResponse>(raw) {
        return Ok(collect_unique_model_ids(
            wrapped.models.into_iter().map(|entry| entry.id),
        ));
    }

    Err(format!("Failed to parse {} models JSON response.", label))
}

#[derive(Serialize)]
pub(crate) struct OpenAiChatRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) temperature: f32,
    pub(crate) max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) top_k: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) repeat_penalty: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) stop: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_format: Option<ResponseFormat>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct ChatMessage {
    pub(crate) role: String,
    pub(crate) content: String,
}

#[derive(Serialize)]
pub(crate) struct ResponseFormat {
    pub(crate) r#type: String,
}

#[derive(Deserialize)]
pub(crate) struct OpenAiChatResponse {
    pub(crate) choices: Vec<ChatChoice>,
    #[serde(default)]
    pub(crate) usage: Option<OpenAiUsage>,
    #[serde(default)]
    pub(crate) timings: Option<LlamaTimings>,
}

#[derive(Deserialize, Default)]
pub(crate) struct OpenAiUsage {
    #[serde(default)]
    pub(crate) prompt_tokens: u64,
    #[serde(default)]
    pub(crate) completion_tokens: u64,
    #[serde(default)]
    pub(crate) prompt_cache_hit_tokens: u64,
    #[serde(default)]
    pub(crate) prompt_tokens_details: Option<PromptTokensDetails>,
}

#[derive(Deserialize, Default)]
pub(crate) struct PromptTokensDetails {
    #[serde(default)]
    pub(crate) cached_tokens: u64,
}

#[derive(Deserialize, Default)]
pub(crate) struct LlamaTimings {
    #[serde(default)]
    pub(crate) cache_n: u64,
}

#[derive(Deserialize)]
pub(crate) struct ChatChoice {
    pub(crate) message: ChatResponseMessage,
}

#[derive(Deserialize)]
pub(crate) struct ChatResponseMessage {
    pub(crate) content: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct ClaudeMessageRequest {
    pub(crate) model: String,
    pub(crate) max_tokens: u32,
    pub(crate) system: String,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) temperature: f32,
}

#[derive(Serialize)]
pub(crate) struct ClaudeCacheControl {
    #[serde(rename = "type")]
    pub(crate) control_type: &'static str,
}

#[derive(Serialize)]
pub(crate) struct ClaudeSystemBlock<'a> {
    #[serde(rename = "type")]
    pub(crate) block_type: &'static str,
    pub(crate) text: &'a str,
    pub(crate) cache_control: ClaudeCacheControl,
}

#[derive(Serialize)]
pub(crate) struct ClaudeCachedMessageRequest<'a> {
    pub(crate) model: &'a str,
    pub(crate) max_tokens: u32,
    pub(crate) system: Vec<ClaudeSystemBlock<'a>>,
    pub(crate) messages: &'a [ChatMessage],
    pub(crate) temperature: f32,
}

#[derive(Deserialize)]
pub(crate) struct ClaudeMessageResponse {
    pub(crate) content: Vec<ClaudeContentBlock>,
    #[serde(default)]
    pub(crate) usage: Option<ClaudeUsage>,
}

#[derive(Deserialize, Default)]
pub(crate) struct ClaudeUsage {
    #[serde(default)]
    pub(crate) input_tokens: u64,
    #[serde(default)]
    pub(crate) cache_creation_input_tokens: u64,
    #[serde(default)]
    pub(crate) cache_read_input_tokens: u64,
    #[serde(default)]
    pub(crate) output_tokens: u64,
}

#[derive(Deserialize)]
pub(crate) struct ClaudeContentBlock {
    pub(crate) text: String,
}

#[derive(Deserialize)]
struct TranslationWrapper {
    translations: Vec<TranslationItem>,
}

fn deserialize_flexible_id<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FlexId {
        Int(usize),
        Str(String),
    }

    match FlexId::deserialize(deserializer)? {
        FlexId::Int(id) => Ok(id),
        FlexId::Str(raw) => raw
            .trim()
            .parse::<usize>()
            .map_err(serde::de::Error::custom),
    }
}

#[derive(Deserialize)]
struct TranslationItem {
    #[serde(deserialize_with = "deserialize_flexible_id")]
    id: usize,
    #[serde(alias = "translation", alias = "translated", alias = "text")]
    target: String,
}

fn strip_reasoning_block(raw_content: &str) -> &str {
    match raw_content.rfind("</think>") {
        Some(end) => &raw_content[end + "</think>".len()..],
        None => raw_content,
    }
}

fn extract_fenced_block(text: &str) -> Option<&str> {
    let fence_start = text.find("```")?;
    let after_fence = &text[fence_start + 3..];
    let body = match after_fence.find('\n') {
        Some(nl) => &after_fence[nl + 1..],
        None => after_fence,
    };
    let sliced = match body.rfind("```") {
        Some(fence_end) => &body[..fence_end],
        None => body,
    };
    Some(sliced.trim())
}

fn extract_outer_json_slice(text: &str) -> Option<&str> {
    let first_obj = text.find('{');
    let first_arr = text.find('[');
    let (start, close) = match (first_obj, first_arr) {
        (Some(o), Some(a)) if a < o => (a, ']'),
        (Some(o), _) => (o, '}'),
        (None, Some(a)) => (a, ']'),
        (None, None) => return None,
    };
    let end = text.rfind(close)?;
    if end > start {
        Some(&text[start..=end])
    } else {
        None
    }
}

fn insert_non_empty(map: &mut HashMap<usize, String>, id: usize, target: String) {
    if !target.trim().is_empty() {
        map.insert(id, target);
    }
}

fn try_parse_json_candidate(candidate: &str) -> Option<HashMap<usize, String>> {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(wrapper) = serde_json::from_str::<TranslationWrapper>(trimmed) {
        let mut map = HashMap::new();
        for item in wrapper.translations {
            insert_non_empty(&mut map, item.id, item.target);
        }
        return Some(map);
    }

    if let Ok(items) = serde_json::from_str::<Vec<TranslationItem>>(trimmed) {
        let mut map = HashMap::new();
        for item in items {
            insert_non_empty(&mut map, item.id, item.target);
        }
        return Some(map);
    }

    if let Ok(obj) = serde_json::from_str::<HashMap<String, serde_json::Value>>(trimmed) {
        let mut map = HashMap::new();
        for (key, value) in obj {
            let Ok(id) = key.trim().parse::<usize>() else {
                continue;
            };
            let extracted = value.as_str().map(str::to_string).or_else(|| {
                ["target", "translation", "translated", "text"]
                    .iter()
                    .find_map(|field| value.get(*field).and_then(|v| v.as_str()))
                    .map(str::to_string)
            });
            if let Some(text) = extracted {
                insert_non_empty(&mut map, id, text);
            }
        }
        if !map.is_empty() {
            return Some(map);
        }
    }

    None
}

fn recover_partial_translation_items(text: &str) -> HashMap<usize, String> {
    let mut translation_map = HashMap::new();
    let mut brace_stack: Vec<usize> = Vec::new();
    let mut in_string = false;
    let mut escaped = false;

    for (idx, ch) in text.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => in_string = true,
            '{' => brace_stack.push(idx),
            '}' => {
                if let Some(start) = brace_stack.pop() {
                    let slice = &text[start..idx + ch.len_utf8()];
                    if let Ok(item) = serde_json::from_str::<TranslationItem>(slice) {
                        insert_non_empty(&mut translation_map, item.id, item.target);
                    }
                }
            }
            _ => {}
        }
    }

    translation_map
}

pub(crate) fn parse_translation_json(raw_content: &str) -> HashMap<usize, String> {
    let stripped = strip_reasoning_block(raw_content).trim();
    if stripped.is_empty() {
        return HashMap::new();
    }

    let fenced = extract_fenced_block(stripped);
    let candidates = [
        Some(stripped),
        fenced,
        extract_outer_json_slice(stripped),
        fenced.and_then(extract_outer_json_slice),
    ];

    for candidate in candidates.into_iter().flatten() {
        if let Some(parsed) = try_parse_json_candidate(candidate) {
            if !parsed.is_empty() {
                return parsed;
            }
        }
    }

    recover_partial_translation_items(fenced.unwrap_or(stripped))
}

pub(crate) fn extract_openai_usage(resp: &OpenAiChatResponse) -> LlmUsageStats {
    let usage = resp.usage.as_ref();
    let prompt_tokens = usage.map_or(0, |u| u.prompt_tokens);
    let completion_tokens = usage.map_or(0, |u| u.completion_tokens);
    let hit_deepseek = usage.map_or(0, |u| u.prompt_cache_hit_tokens);
    let hit_details = usage
        .and_then(|u| u.prompt_tokens_details.as_ref())
        .map_or(0, |d| d.cached_tokens);
    let hit_llama = resp.timings.as_ref().map_or(0, |t| t.cache_n);
    let prompt_cached_tokens = hit_deepseek.max(hit_details).max(hit_llama).min(prompt_tokens);

    LlmUsageStats {
        prompt_tokens,
        prompt_cached_tokens,
        completion_tokens,
        ..LlmUsageStats::default()
    }
}

pub(crate) fn extract_claude_usage(usage: Option<&ClaudeUsage>) -> LlmUsageStats {
    let Some(u) = usage else {
        return LlmUsageStats::default();
    };
    let prompt_tokens = u
        .input_tokens
        .saturating_add(u.cache_creation_input_tokens)
        .saturating_add(u.cache_read_input_tokens);
    LlmUsageStats {
        prompt_tokens,
        prompt_cached_tokens: u.cache_read_input_tokens.min(prompt_tokens),
        completion_tokens: u.output_tokens,
        ..LlmUsageStats::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_prompt_cache_tokens_across_providers() {
        let deepseek: OpenAiChatResponse = serde_json::from_str(
            r#"{"choices":[],"usage":{"prompt_tokens":400,"completion_tokens":45,"prompt_cache_hit_tokens":320}}"#,
        )
        .expect("deepseek usage");
        let ds_stats = extract_openai_usage(&deepseek);
        assert_eq!(ds_stats.prompt_tokens, 400);
        assert_eq!(ds_stats.prompt_cached_tokens, 320);

        let openai: OpenAiChatResponse = serde_json::from_str(
            r#"{"choices":[],"usage":{"prompt_tokens":500,"completion_tokens":30,"prompt_tokens_details":{"cached_tokens":384}}}"#,
        )
        .expect("openai usage");
        let oai_stats = extract_openai_usage(&openai);
        assert_eq!(oai_stats.prompt_cached_tokens, 384);

        let llama: OpenAiChatResponse = serde_json::from_str(
            r#"{"choices":[],"usage":{"prompt_tokens":350,"completion_tokens":25},"timings":{"cache_n":280}}"#,
        )
        .expect("llama usage");
        let llama_stats = extract_openai_usage(&llama);
        assert_eq!(llama_stats.prompt_cached_tokens, 280);

        let claude: ClaudeMessageResponse = serde_json::from_str(
            r#"{"content":[{"text":"{}"}],"usage":{"input_tokens":50,"cache_creation_input_tokens":0,"cache_read_input_tokens":310,"output_tokens":20}}"#,
        )
        .expect("claude usage");
        let claude_stats = extract_claude_usage(claude.usage.as_ref());
        assert_eq!(claude_stats.prompt_tokens, 360);
        assert_eq!(claude_stats.prompt_cached_tokens, 310);
        assert_eq!(claude_stats.completion_tokens, 20);
    }

    #[test]
    fn parses_openai_and_claude_models_payloads() {
        let standard = r#"{"data":[{"id":"claude-sonnet-4-20250514"},{"id":"claude-3-5-sonnet-latest"},{"id":"claude-sonnet-4-20250514"}]}"#;
        let ids = parse_models_list_json(standard, "Claude").expect("parse standard");
        assert_eq!(
            ids,
            vec!["claude-sonnet-4-20250514", "claude-3-5-sonnet-latest"]
        );

        let array_payload = r#"[{"id":"deepseek-chat"},{"id":"deepseek-reasoner"}]"#;
        let array_ids = parse_models_list_json(array_payload, "OpenAI").expect("parse array");
        assert_eq!(array_ids, vec!["deepseek-chat", "deepseek-reasoner"]);

        let empty_data = r#"{"data":[]}"#;
        assert!(parse_models_list_json(empty_data, "OpenAI")
            .expect("parse empty")
            .is_empty());
    }

    #[test]
    fn parses_translation_json_across_wrapped_and_truncated_formats() {
        let prose_wrapped = r#"<think>internal reasoning</think>
Here is the translation:
```json
{"translations":[{"id":"0","target":"Halo!"},{"id":1,"translation":"Apa kabar?"}]}
```
Done."#;
        let parsed = parse_translation_json(prose_wrapped);
        assert_eq!(parsed.get(&0).map(String::as_str), Some("Halo!"));
        assert_eq!(parsed.get(&1).map(String::as_str), Some("Apa kabar?"));

        let truncated = r#"```json
{"translations":[{"id":0,"target":"Pertama"},{"id":"1","text":"Kedua"},{"id":2,"target":"Terpotong"#;
        let recovered = parse_translation_json(truncated);
        assert_eq!(recovered.len(), 2);
        assert_eq!(recovered.get(&0).map(String::as_str), Some("Pertama"));
        assert_eq!(recovered.get(&1).map(String::as_str), Some("Kedua"));

        let empty_targets = r#"{"translations":[{"id":0,"target":"   "}]}"#;
        assert!(parse_translation_json(empty_targets).is_empty());
    }
}

