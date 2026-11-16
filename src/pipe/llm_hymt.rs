use super::{ChatEndpoint, LlmClient, LlmTranslationOutput};
use crate::model::Bubble;
use crate::pipe::llm_wire::{extract_openai_usage, ChatMessage, OpenAiChatRequest};
use crate::pipe::prompts::{
    build_hymt_user_prompt, normalize_hymt_input_casing, strip_hymt_artifacts,
};
use crate::utilities::LlmUsageStats;
use futures_util::stream::{self, StreamExt};
use std::collections::HashMap;

const HYMT_LOCAL_CONCURRENCY: usize = 6;

fn hymt_max_tokens(source_text: &str) -> u32 {
    ((source_text.chars().count() as u32 * 3) + 48).clamp(64, 384)
}

impl LlmClient {
    pub(super) async fn translate_hymt_local(
        &self,
        endpoint: &ChatEndpoint,
        bubbles: &[Bubble],
        source_lang: &str,
        target_lang: &str,
    ) -> Result<LlmTranslationOutput, String> {
        if bubbles.is_empty() {
            return Ok(LlmTranslationOutput::default());
        }

        let stop_tokens = vec![
            "<｜hy_end_of_assistant｜>".to_string(),
            "<｜hy_end_of_sentence｜>".to_string(),
            "<｜hy_place▁holder▁no▁2｜>".to_string(),
            "<|eos|>".to_string(),
            "<|extra_0|>".to_string(),
        ];

        let jobs: Vec<(usize, OpenAiChatRequest)> = bubbles
            .iter()
            .enumerate()
            .map(|(index, bubble)| {
                let context_start = index.saturating_sub(3);
                let context_lines: Vec<String> = bubbles[context_start..index]
                    .iter()
                    .map(|b| normalize_hymt_input_casing(&b.source_text))
                    .filter(|text| !text.is_empty())
                    .collect();
                let context = if context_lines.is_empty() {
                    None
                } else {
                    Some(context_lines.join("\n"))
                };

                let prompt = build_hymt_user_prompt(
                    &bubble.source_text,
                    context.as_deref(),
                    source_lang,
                    target_lang,
                );
                let request = OpenAiChatRequest {
                    model: endpoint.model.clone(),
                    messages: vec![ChatMessage {
                        role: "user".to_string(),
                        content: prompt,
                    }],
                    temperature: 0.35,
                    max_tokens: hymt_max_tokens(&bubble.source_text),
                    top_p: Some(0.6),
                    top_k: Some(20),
                    repeat_penalty: Some(1.05),
                    stop: Some(stop_tokens.clone()),
                    response_format: None,
                };
                (bubble.id, request)
            })
            .collect();

        let concurrency = HYMT_LOCAL_CONCURRENCY.min(jobs.len()).max(1);
        let api_key = endpoint.api_key.as_deref();
        let results: Vec<Result<(usize, String, LlmUsageStats), String>> = stream::iter(jobs)
            .map(|(bubble_id, request)| async move {
                let response = self
                    .send_chat(
                        &endpoint.url,
                        api_key,
                        endpoint.label,
                        endpoint.unreachable_hint,
                        90,
                        &request,
                    )
                    .await
                    .map_err(|(_, msg)| msg)?;

                let usage = extract_openai_usage(&response);
                let raw = response
                    .choices
                    .first()
                    .and_then(|choice| choice.message.content.as_deref())
                    .unwrap_or_default();
                let translated = strip_hymt_artifacts(raw);
                Ok((bubble_id, translated, usage))
            })
            .buffer_unordered(concurrency)
            .collect()
            .await;

        let mut translations = HashMap::with_capacity(bubbles.len());
        let mut total_usage = LlmUsageStats::default();
        for item in results {
            let (bubble_id, translated, usage) = item?;
            total_usage.prompt_tokens += usage.prompt_tokens;
            total_usage.prompt_cached_tokens += usage.prompt_cached_tokens;
            total_usage.completion_tokens += usage.completion_tokens;
            if !translated.is_empty() {
                translations.insert(bubble_id, translated);
            }
        }

        Ok(LlmTranslationOutput {
            translations,
            usage: total_usage,
        })
    }
}
