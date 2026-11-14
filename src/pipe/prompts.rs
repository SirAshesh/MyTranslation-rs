use crate::model::Bubble;

pub const CLOUD_INITIAL_SYSTEM_PROMPT: &str = r#"You are a professional comic, manga, manhwa, and manhua scanlation translator.
Translate dialogue items accurately into the target language requested in the user message, matching natural conversational comic panels.

SCANLATION & ACCURACY RULES:
1. HONORIFICS & SOCIAL REGISTER:
   - Preserve character-defining honorific suffixes (-san, -kun, -chan, -sama, -senpai, -sensei, -dono, oppa, hyung, noona, unnie, -ge, -jie) or adapt titles naturally by social hierarchy when fitting the target language.
   - Use natural peer dialogue for friends/close peers and respectful register only when addressing elders, nobles, or superiors.
2. OCR MERGED WORDS & TYPO RECOVERY:
   - Input comes from comic OCR that often drops spaces due to tight lettering (e.g. "wemust takea waterbucket" -> "we must take a water bucket", "hehas" -> "he has", "anda love potion" -> "and a love potion", "imiss you" -> "I miss you").
   - Segment and infer the intended source phrase in context before translating.
   - Follow standard target-language spacing and orthography with no accidental merged words.
3. CONCISE & IDIOMATIC DIALOGUE:
   - Keep lines punchy to fit narrow speech bubbles without omitting meaning or repeating subjects.
   - Translate idioms by intent (e.g. "Excuse me?" -> polite query or shocked challenge, never literal word-for-word nonsense).
4. PUNCTUATION & STRICT TARGET LANGUAGE:
   - Preserve emotional punctuation (..., ?, !).
   - Write every "target" string exclusively in the requested target language.
5. REQUIRED OUTPUT FORMAT:
   Return a valid JSON object strictly matching:
   {"translations":[{"id":1,"target":"<natural translated dialogue>"}]}"#;

pub const LOCAL_INITIAL_SYSTEM_PROMPT: &str = r#"You are an expert comic and manga scanlation translator.
Translate every dialogue item accurately into the target language requested in the user message.

RULES:
1. HONORIFICS & REGISTER:
   - Keep iconic honorific suffixes (-san, -kun, -chan, -sama, -senpai, -sensei) or adapt hierarchy titles naturally.
   - Match peer casual tone vs noble/superior respectful tone consistently.
2. FIX OCR MERGED TEXT:
   - OCR often drops spaces ("wemust takea waterbucket" -> "we must take a water bucket", "anda love potion" -> "and a love potion").
   - Segment intended words in the source language before translating, and output clean spacing in the target language.
3. CONCISE & EXPRESSIVE:
   - Keep dialogue short enough for speech bubbles and preserve punctuation (..., ?, !).
4. STRICT JSON OUTPUT:
   Return only valid JSON:
   {"translations":[{"id":1,"target":"<natural target translation>"}]}"#;

pub fn initial_system_prompt(local_mode: bool) -> &'static str {
    if local_mode {
        LOCAL_INITIAL_SYSTEM_PROMPT
    } else {
        CLOUD_INITIAL_SYSTEM_PROMPT
    }
}

pub fn resolve_language_name(code: &str, default_name: &str) -> String {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return default_name.to_string();
    }

    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" => default_name.to_string(),
        "id" | "ind" | "indonesian" | "bahasa indonesia" => "Indonesian".to_string(),
        "en" | "eng" | "english" | "en-us" | "en-gb" => "English".to_string(),
        "ja" | "jp" | "jpn" | "japanese" => "Japanese".to_string(),
        "zh" | "zh-cn" | "zh-hans" | "chi" | "zho" | "chinese" | "chs" => {
            "Simplified Chinese".to_string()
        }
        "zh-tw" | "zh-hk" | "zh-hant" | "cht" => "Traditional Chinese".to_string(),
        "ko" | "kor" | "korean" => "Korean".to_string(),
        "vi" | "vie" | "vietnamese" => "Vietnamese".to_string(),
        "th" | "tha" | "thai" => "Thai".to_string(),
        "ms" | "msa" | "zsm" | "malay" | "bahasa melayu" => "Malay".to_string(),
        "es" | "spa" | "spanish" => "Spanish".to_string(),
        "fr" | "fra" | "fre" | "french" => "French".to_string(),
        "de" | "deu" | "ger" | "german" => "German".to_string(),
        "ru" | "rus" | "russian" => "Russian".to_string(),
        "ar" | "ara" | "arabic" => "Arabic".to_string(),
        "pt" | "pt-br" | "pt-pt" | "por" | "portuguese" => "Portuguese".to_string(),
        "it" | "ita" | "italian" => "Italian".to_string(),
        "nl" | "nld" | "dut" | "dutch" => "Dutch".to_string(),
        "pl" | "pol" | "polish" => "Polish".to_string(),
        "tr" | "tur" | "turkish" => "Turkish".to_string(),
        "hi" | "hin" | "hindi" => "Hindi".to_string(),
        _ => {
            if let Some(prefix) = normalized.split('-').next() {
                if prefix != normalized {
                    return resolve_language_name(prefix, trimmed);
                }
            }
            trimmed.to_string()
        }
    }
}

pub fn resolve_source_language_label(source_lang: &str) -> String {
    let trimmed = source_lang.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") || trimmed.eq_ignore_ascii_case("detect") {
        return "auto-detected source language".to_string();
    }
    resolve_language_name(trimmed, trimmed)
}

pub fn resolve_target_language_label(target_lang: &str) -> String {
    resolve_language_name(target_lang, "Indonesian")
}

pub fn normalize_bubble_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn target_style_note(target_name: &str) -> &'static str {
    match target_name {
        "Indonesian" => "Style: natural Indonesian comic dialogue (aku/kamu, udah, nggak, gimana, kok, sih, dong; Saya/Anda only for nobles/superiors; never translate \"Excuse me?\" literally as \"Alasan me\").",
        "Japanese" | "Simplified Chinese" | "Traditional Chinese" | "Thai" => {
            "Style: natural comic dialogue following standard orthography with no extra Latin spaces."
        }
        _ => "Style: natural conversational comic dialogue matching speaker tone and clean word spacing.",
    }
}

pub fn build_translation_user_prompt(
    bubbles: &[Bubble],
    source_lang: &str,
    target_lang: &str,
) -> String {
    let source_label = resolve_source_language_label(source_lang);
    let target_label = resolve_target_language_label(target_lang);
    let input_items: Vec<serde_json::Value> = bubbles
        .iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id,
                "text": normalize_bubble_text(&b.source_text)
            })
        })
        .collect();

    format!(
        "Source: {} | Target: {}\n{}\nItems: {}",
        source_label,
        target_label,
        target_style_note(&target_label),
        serde_json::to_string(&input_items).unwrap_or_default()
    )
}

fn is_chinese_language(code: &str) -> bool {
    let lower = code.trim().to_ascii_lowercase();
    lower.starts_with("zh") || lower == "chi" || lower == "zho" || lower == "chs" || lower == "cht" || lower == "chinese"
}

fn hymt_chinese_language_name(code: &str) -> String {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return "印尼语".to_string();
    }
    let normalized = trimmed.to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "auto" | "id" | "ind" | "indonesian" | "bahasa indonesia" => "印尼语".to_string(),
        "en" | "eng" | "english" | "en-us" | "en-gb" => "英语".to_string(),
        "ja" | "jp" | "jpn" | "japanese" => "日语".to_string(),
        "ko" | "kor" | "korean" => "韩语".to_string(),
        "zh" | "zh-cn" | "zh-hans" | "chi" | "zho" | "chinese" | "chs" => "中文".to_string(),
        "zh-tw" | "zh-hk" | "zh-hant" | "cht" => "繁体中文".to_string(),
        "ms" | "msa" | "zsm" | "malay" | "bahasa melayu" => "马来语".to_string(),
        "vi" | "vie" | "vietnamese" => "越南语".to_string(),
        "th" | "tha" | "thai" => "泰语".to_string(),
        "tl" | "fil" | "filipino" | "tagalog" => "菲律宾语".to_string(),
        "fr" | "fra" | "fre" | "french" => "法语".to_string(),
        "de" | "deu" | "ger" | "german" => "德语".to_string(),
        "es" | "spa" | "spanish" => "西班牙语".to_string(),
        "pt" | "pt-br" | "pt-pt" | "por" | "portuguese" => "葡萄牙语".to_string(),
        "ru" | "rus" | "russian" => "俄语".to_string(),
        "ar" | "ara" | "arabic" => "阿拉伯语".to_string(),
        "it" | "ita" | "italian" => "意大利语".to_string(),
        "nl" | "nld" | "dut" | "dutch" => "荷兰语".to_string(),
        "pl" | "pol" | "polish" => "波兰语".to_string(),
        "tr" | "tur" | "turkish" => "土耳其语".to_string(),
        "hi" | "hin" | "hindi" => "印地语".to_string(),
        _ => resolve_target_language_label(trimmed),
    }
}

pub fn build_hymt_user_prompt(
    text: &str,
    context: Option<&str>,
    source_lang: &str,
    target_lang: &str,
) -> String {
    let cleaned_text = normalize_bubble_text(text);
    if let Some(ctx) = context.map(str::trim).filter(|c| !c.is_empty()) {
        let zh_target = hymt_chinese_language_name(target_lang);
        return format!(
            "{}\n参考上面的信息，把下面的文本翻译成{}，注意不需要翻译上文，也不要额外解释：\n{}",
            ctx, zh_target, cleaned_text
        );
    }

    if is_chinese_language(source_lang) || is_chinese_language(target_lang) {
        let zh_target = hymt_chinese_language_name(target_lang);
        return format!(
            "将以下文本翻译为{}，注意只需要输出翻译后的结果，不要额外解释：\n\n{}",
            zh_target, cleaned_text
        );
    }

    let en_target = resolve_target_language_label(target_lang);
    format!(
        "Translate the following segment into {}, without additional explanation.\n\n{}",
        en_target, cleaned_text
    )
}

pub fn strip_hymt_artifacts(raw: &str) -> String {
    let mut cleaned = raw.trim().to_string();
    for token in [
        "<｜hy_end_of_assistant｜>",
        "<｜hy_end_of_sentence｜>",
        "<｜hy_Assistant｜>",
        "<｜hy_begin▁of▁sentence｜>",
        "<｜hy_User｜>",
        "</s>",
    ] {
        cleaned = cleaned.replace(token, "");
    }
    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bubble() -> Bubble {
        Bubble {
            id: 1,
            x1: 0.0,
            y1: 0.0,
            x2: 100.0,
            y2: 50.0,
            cx: 50.0,
            cy: 25.0,
            w: 100.0,
            h: 50.0,
            oriented_w: 100.0,
            oriented_h: 50.0,
            angle_rad: 0.0,
            is_left_aligned: false,
            source_text: "Hello   world\nagain".to_string(),
            lines: vec![],
        }
    }

    #[test]
    fn maps_extension_language_codes_to_names() {
        assert_eq!(resolve_target_language_label("id"), "Indonesian");
        assert_eq!(resolve_target_language_label("en"), "English");
        assert_eq!(resolve_target_language_label("ja"), "Japanese");
        assert_eq!(resolve_target_language_label("zh"), "Simplified Chinese");
        assert_eq!(resolve_target_language_label("zh-TW"), "Traditional Chinese");
        assert_eq!(resolve_target_language_label("ko"), "Korean");
        assert_eq!(resolve_source_language_label("auto"), "auto-detected source language");
        assert_eq!(resolve_source_language_label("ko"), "Korean");
    }

    #[test]
    fn keeps_initial_system_prompt_immutable_for_kv_cache() {
        let sys_cloud = initial_system_prompt(false);
        let sys_local = initial_system_prompt(true);
        assert!(sys_cloud.contains("-san"));
        assert!(sys_cloud.contains("wemust takea waterbucket"));
        assert!(sys_local.contains("wemust takea waterbucket"));
        assert_eq!(sys_cloud.as_ptr(), initial_system_prompt(false).as_ptr());

        let user_id = build_translation_user_prompt(&[sample_bubble()], "auto", "id");
        assert!(user_id.contains("Source: auto-detected source language | Target: Indonesian"));
        assert!(user_id.contains("aku/kamu"));
        assert!(user_id.contains("Hello world again"));

        let user_es = build_translation_user_prompt(&[sample_bubble()], "ko", "es");
        assert!(user_es.contains("Source: Korean | Target: Spanish"));
        assert!(!user_es.contains("aku/kamu"));
    }

    #[test]
    fn formats_hymt_prompts_and_cleans_tokens() {
        let non_zh = build_hymt_user_prompt("Hello   world", None, "en", "id");
        assert!(non_zh.contains("Translate the following segment into Indonesian"));
        assert!(non_zh.contains("Hello world"));

        let zh_pair = build_hymt_user_prompt("你好世界", None, "zh", "id");
        assert!(zh_pair.contains("将以下文本翻译为印尼语"));

        let contextual = build_hymt_user_prompt("Follow me!", Some("Wait up!"), "en", "id");
        assert!(contextual.contains("Wait up!"));
        assert!(contextual.contains("把下面的文本翻译成印尼语"));

        let stripped = strip_hymt_artifacts("Ikuti aku!<｜hy_end_of_assistant｜>");
        assert_eq!(stripped, "Ikuti aku!");
    }
}
