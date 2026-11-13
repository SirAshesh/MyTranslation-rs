use crate::utilities::LlmUsageStats;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize)]
pub(crate) struct OpenAiChatRequest {
    pub(crate) model: String,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) temperature: f32,
    pub(crate) max_tokens: u32,
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

#[derive(Deserialize)]
struct TranslationItem {
    id: usize,
    target: String,
}

fn strip_reasoning_block(raw_content: &str) -> &str {
    match raw_content.rfind("</think>") {
        Some(end) => &raw_content[end + "</think>".len()..],
        None => raw_content,
    }
}

pub(crate) fn parse_translation_json(raw_content: &str) -> HashMap<usize, String> {
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
}
