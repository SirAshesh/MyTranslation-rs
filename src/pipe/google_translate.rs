use crate::model::Bubble;
use crate::pipe::llm_client::LlmTranslationOutput;
use crate::pipe::prompts::normalize_bubble_text;
use crate::utilities::LlmUsageStats;
use std::collections::HashMap;
use tracing::warn;
use translators::{GoogleTranslator, Translator as CrateTranslator};

const GOOGLE_TRANSLATE_TIMEOUT_SECS: usize = 35;
const GOOGLE_TRANSLATE_SEQUENTIAL_DELAY_MS: usize = 150;

fn build_google_translator(delay: usize, max_concurrency: Option<usize>) -> GoogleTranslator {
    GoogleTranslator {
        timeout: GOOGLE_TRANSLATE_TIMEOUT_SECS,
        delay,
        max_concurrency,
        ..GoogleTranslator::default()
    }
}

/// Normalizes extension or user-provided language codes into codes accepted by Google Translate.
pub fn resolve_google_lang_code(code: &str, default_code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return default_code.to_string();
    }

    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" | "detect" => default_code.to_string(),
        "id" | "ind" | "indonesian" | "bahasa indonesia" => "id".to_string(),
        "en" | "eng" | "english" | "en-us" | "en-gb" => "en".to_string(),
        "ja" | "jp" | "jpn" | "japanese" => "ja".to_string(),
        "zh" | "zh-cn" | "zh-hans" | "chi" | "zho" | "chinese" | "chs" => "zh-CN".to_string(),
        "zh-tw" | "zh-hk" | "zh-hant" | "cht" => "zh-TW".to_string(),
        "ko" | "kor" | "korean" => "ko".to_string(),
        "vi" | "vie" | "vietnamese" => "vi".to_string(),
        "th" | "tha" | "thai" => "th".to_string(),
        "ms" | "msa" | "zsm" | "malay" | "bahasa melayu" => "ms".to_string(),
        "es" | "spa" | "spanish" => "es".to_string(),
        "fr" | "fra" | "fre" | "french" => "fr".to_string(),
        "de" | "deu" | "ger" | "german" => "de".to_string(),
        "ru" | "rus" | "russian" => "ru".to_string(),
        "ar" | "ara" | "arabic" => "ar".to_string(),
        "pt" | "pt-br" | "pt-pt" | "por" | "portuguese" => "pt".to_string(),
        "it" | "ita" | "italian" => "it".to_string(),
        "nl" | "nld" | "dut" | "dutch" => "nl".to_string(),
        "pl" | "pol" | "polish" => "pl".to_string(),
        "tr" | "tur" | "turkish" => "tr".to_string(),
        "hi" | "hin" | "hindi" => "hi".to_string(),
        "tl" | "fil" | "filipino" | "tagalog" => "tl".to_string(),
        _ => normalized
            .split('-')
            .next()
            .filter(|prefix| !prefix.is_empty())
            .unwrap_or(&normalized)
            .to_string(),
    }
}

/// Verifies connectivity to the built-in Google Translate endpoint.
pub async fn test_google_translate_connection() -> Result<String, String> {
    let translator = build_google_translator(0, Some(1));
    let sample = translator
        .translate_async("Hello", "en", "id")
        .await
        .map_err(|e| format!("Google Translate connection failed: {}", e))?;
    Ok(format!(
        "Connected to Google Translate (sample: \"Hello\" -> \"{}\").",
        sample.trim()
    ))
}

/// Translates comic bubbles using a single batched Google Translate request with sequential fallback.
pub async fn translate_with_google(
    bubbles: &[Bubble],
    source_lang: &str,
    target_lang: &str,
) -> Result<LlmTranslationOutput, String> {
    let non_empty_jobs: Vec<(usize, String)> = bubbles
        .iter()
        .filter_map(|bubble| {
            let normalized = normalize_bubble_text(&bubble.source_text);
            (!normalized.is_empty()).then_some((bubble.id, normalized))
        })
        .collect();

    if non_empty_jobs.is_empty() {
        return Ok(LlmTranslationOutput::default());
    }

    let src = resolve_google_lang_code(source_lang, "");
    let dst = resolve_google_lang_code(target_lang, "id");
    let translator = build_google_translator(0, Some(1));

    if non_empty_jobs.len() == 1 {
        let (bubble_id, text) = &non_empty_jobs[0];
        let translated = translator
            .translate_async(text, &src, &dst)
            .await
            .map_err(|e| format!("Google Translate failed for bubble {}: {}", bubble_id, e))?;
        let mut translations = HashMap::with_capacity(1);
        let trimmed = translated.trim().to_string();
        if !trimmed.is_empty() {
            translations.insert(*bubble_id, trimmed);
        }
        return Ok(LlmTranslationOutput {
            translations,
            usage: LlmUsageStats::default(),
        });
    }

    let batched_input = non_empty_jobs
        .iter()
        .map(|(_, text)| text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let batched_output = translator
        .translate_async(&batched_input, &src, &dst)
        .await
        .map_err(|e| format!("Google Translate batch request failed: {}", e))?;

    let batched_lines: Vec<&str> = batched_output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();

    if batched_lines.len() == non_empty_jobs.len() {
        let translations = non_empty_jobs
            .iter()
            .zip(batched_lines)
            .map(|((bubble_id, _), line)| (*bubble_id, line.to_string()))
            .collect();
        return Ok(LlmTranslationOutput {
            translations,
            usage: LlmUsageStats::default(),
        });
    }

    warn!(
        "Google Translate batch returned {} lines for {} bubbles; falling back to sequential requests",
        batched_lines.len(),
        non_empty_jobs.len()
    );

    let sequential_translator =
        build_google_translator(GOOGLE_TRANSLATE_SEQUENTIAL_DELAY_MS, Some(1));
    let mut translations = HashMap::with_capacity(non_empty_jobs.len());
    for (bubble_id, text) in non_empty_jobs {
        let translated = sequential_translator
            .translate_async(&text, &src, &dst)
            .await
            .map_err(|e| format!("Google Translate failed for bubble {}: {}", bubble_id, e))?;
        let trimmed = translated.trim().to_string();
        if !trimmed.is_empty() {
            translations.insert(bubble_id, trimmed);
        }
    }

    Ok(LlmTranslationOutput {
        translations,
        usage: LlmUsageStats::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_google_language_codes() {
        assert_eq!(resolve_google_lang_code("auto", ""), "");
        assert_eq!(resolve_google_lang_code("", "id"), "id");
        assert_eq!(resolve_google_lang_code("jp", ""), "ja");
        assert_eq!(resolve_google_lang_code("Japanese", ""), "ja");
        assert_eq!(resolve_google_lang_code("zh", "id"), "zh-CN");
        assert_eq!(resolve_google_lang_code("zh-TW", "id"), "zh-TW");
        assert_eq!(resolve_google_lang_code("indonesian", "id"), "id");
        assert_eq!(resolve_google_lang_code("en-US", ""), "en");
    }
}
