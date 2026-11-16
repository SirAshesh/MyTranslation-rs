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

fn contains_ascii_phrase(haystack: &str, phrase: &str) -> bool {
    let haystack_lower = haystack.to_ascii_lowercase();
    let phrase_lower = phrase.to_ascii_lowercase();
    let bytes = haystack_lower.as_bytes();
    let mut search_from = 0;

    while let Some(offset) = haystack_lower[search_from..].find(&phrase_lower) {
        let start = search_from + offset;
        let end = start + phrase_lower.len();
        let left_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let right_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if left_ok && right_ok {
            return true;
        }
        search_from = start + 1;
    }
    false
}

fn contains_cjk_char_standalone(text: &str, ch: char, blocked_next: char) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(current) = chars.next() {
        if current == ch && chars.peek() != Some(&blocked_next) {
            return true;
        }
    }
    false
}

fn capitalize_honorific_prefixes(text: &str) -> String {
    const SUFFIXES: [&str; 7] = [
        "-san", "-kun", "-chan", "-sama", "-dono", "-sensei", "-senpai",
    ];
    let mut result = text.to_string();
    for suffix in SUFFIXES {
        let mut search_from = 0;
        while let Some(offset) = result[search_from..].find(suffix) {
            let suffix_start = search_from + offset;
            let suffix_end = suffix_start + suffix.len();
            let followed_by_boundary = suffix_end == result.len()
                || !result[suffix_end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphanumeric());
            if followed_by_boundary {
                let word_start = result[..suffix_start]
                    .rfind(|c: char| !c.is_ascii_alphabetic())
                    .map_or(0, |idx| idx + 1);
                if word_start < suffix_start {
                    if let Some(first_byte) = result.as_bytes().get(word_start).copied() {
                        if first_byte.is_ascii_lowercase() {
                            let upper = (first_byte as char).to_ascii_uppercase().to_string();
                            result.replace_range(word_start..word_start + 1, &upper);
                        }
                    }
                }
            }
            search_from = suffix_end;
        }
    }
    result
}

pub fn normalize_hymt_input_casing(text: &str) -> String {
    let collapsed = normalize_bubble_text(text);
    let mut alpha_count = 0usize;
    let mut upper_count = 0usize;
    for ch in collapsed.chars() {
        if ch.is_ascii_alphabetic() {
            alpha_count += 1;
            if ch.is_ascii_uppercase() {
                upper_count += 1;
            }
        }
    }
    if alpha_count < 4 || upper_count != alpha_count {
        return collapsed;
    }

    let lower = collapsed.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    let mut normalized = String::with_capacity(lower.len());
    let mut at_sentence_start = true;

    for (i, ch) in lower.char_indices() {
        let prev_is_alnum = i > 0 && bytes[i - 1].is_ascii_alphanumeric();
        let next_byte = bytes.get(i + 1).copied();
        let is_standalone_i = ch == 'i'
            && !prev_is_alnum
            && next_byte.is_none_or(|b| !b.is_ascii_alphanumeric());

        if ch.is_ascii_alphabetic() {
            if at_sentence_start || is_standalone_i {
                normalized.push(ch.to_ascii_uppercase());
            } else {
                normalized.push(ch);
            }
            at_sentence_start = false;
        } else {
            normalized.push(ch);
            if matches!(ch, '.' | '!' | '?') {
                at_sentence_start = true;
            } else if !ch.is_whitespace() && !matches!(ch, '"' | '\'' | '(' | '[') {
                at_sentence_start = false;
            }
        }
    }

    capitalize_honorific_prefixes(&normalized)
}

fn collect_hymt_glossary_rules(text: &str, target_label: &str) -> Vec<&'static str> {
    const HONORIFIC_RULES: [(&str, &str); 13] = [
        ("先輩", "先輩 翻译成 Senpai"),
        ("先生", "先生 翻译成 Sensei"),
        ("お兄ちゃん", "お兄ちゃん 翻译成 Kak"),
        ("お姉ちゃん", "お姉ちゃん 翻译成 Kak"),
        ("오빠", "오빠 翻译成 Oppa"),
        ("누나", "누나 翻译成 Noona"),
        ("언니", "언니 翻译成 Unnie"),
        ("선배님", "선배님 翻译成 Sunbae"),
        ("선배", "선배 翻译成 Sunbae"),
        ("师兄", "师兄 翻译成 Shixiong"),
        ("师姐", "师姐 翻译成 Shijie"),
        ("师弟", "师弟 翻译成 Shidi"),
        ("师妹", "师妹 翻译成 Shimei"),
    ];
    const INDONESIAN_CJK_PRONOUNS: [(&str, &str); 15] = [
        ("私は", "私は 翻译成 aku"),
        ("私が", "私が 翻译成 aku"),
        ("俺は", "俺は 翻译成 aku"),
        ("俺が", "俺が 翻译成 aku"),
        ("俺たち", "俺たち 翻译成 kami"),
        ("僕たち", "僕たち 翻译成 kami"),
        ("私たち", "私たち 翻译成 kami"),
        ("あなた", "あなた 翻译成 kamu"),
        ("君は", "君は 翻译成 kamu"),
        ("저는", "저는 翻译成 aku"),
        ("제가", "제가 翻译成 aku"),
        ("나는", "나는 翻译成 aku"),
        ("내가", "내가 翻译成 aku"),
        ("우리", "우리 翻译成 kita"),
        ("당신", "당신 翻译成 kamu"),
    ];
    const INDONESIAN_EN_PHRASES: [(&str, &str); 4] = [
        ("are you", "are you 翻译成 apa kamu"),
        ("thanks to you", "thanks to you 翻译成 berkat kamu"),
        ("i am", "I am 翻译成 aku"),
        ("i'm", "I'm 翻译成 aku"),
    ];

    let mut rules = Vec::new();
    for (pattern, rule) in HONORIFIC_RULES {
        if pattern == "선배" && text.contains("선배님") {
            continue;
        }
        if text.contains(pattern) {
            rules.push(rule);
        }
    }

    if target_label != "Indonesian" {
        return rules;
    }

    for (pattern, rule) in INDONESIAN_CJK_PRONOUNS {
        if text.contains(pattern) {
            rules.push(rule);
        }
    }
    if text.contains("我们") {
        rules.push("我们 翻译成 kami");
    } else if contains_cjk_char_standalone(text, '我', '们') {
        rules.push("我 翻译成 aku");
    }
    if text.contains("你们") {
        rules.push("你们 翻译成 kalian");
    } else if contains_cjk_char_standalone(text, '你', '们') {
        rules.push("你 翻译成 kamu");
    }
    for (phrase, rule) in INDONESIAN_EN_PHRASES {
        if contains_ascii_phrase(text, phrase) {
            rules.push(rule);
        }
    }
    rules
}

fn is_likely_chinese_text(text: &str) -> bool {
    let mut has_hanzi = false;
    for ch in text.chars() {
        if matches!(ch, '\u{3040}'..='\u{30FF}' | '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}') {
            return false;
        }
        if matches!(ch, '\u{4E00}'..='\u{9FFF}') {
            has_hanzi = true;
        }
    }
    has_hanzi
}

fn has_meaningful_context_target(text: &str) -> bool {
    let word_count = text.split_whitespace().count();
    let cjk_count = text
        .chars()
        .filter(|c| matches!(c, '\u{4E00}'..='\u{9FFF}'))
        .count();
    word_count >= 3 || cjk_count >= 6
}

pub fn build_hymt_user_prompt(
    text: &str,
    context: Option<&str>,
    source_lang: &str,
    target_lang: &str,
) -> String {
    let cleaned_text = normalize_hymt_input_casing(text);
    let target_label = resolve_target_language_label(target_lang);
    let glossary = collect_hymt_glossary_rules(&cleaned_text, &target_label);
    let glossary_prefix = if glossary.is_empty() {
        String::new()
    } else {
        format!("参考下面的翻译：\n{}\n", glossary.join("\n"))
    };

    let use_chinese_template = is_chinese_language(source_lang)
        || is_chinese_language(target_lang)
        || (source_lang.trim().eq_ignore_ascii_case("auto") && is_likely_chinese_text(&cleaned_text));

    if use_chinese_template {
        let zh_target = hymt_chinese_language_name(target_lang);
        if has_meaningful_context_target(&cleaned_text) {
            if let Some(ctx) = context.map(normalize_hymt_input_casing).filter(|c| !c.is_empty()) {
                return format!(
                    "{}{}\n参考上面的信息，把下面的文本翻译成{}，注意不需要翻译上文，也不要额外解释：\n{}",
                    glossary_prefix, ctx, zh_target, cleaned_text
                );
            }
        }
        return format!(
            "{}将以下文本翻译为{}，注意只需要输出翻译后的结果，不要额外解释：\n\n{}",
            glossary_prefix, zh_target, cleaned_text
        );
    }

    if target_label == "Indonesian" {
        return format!(
            "{}Translate the following comic dialogue into casual Indonesian (using aku/kamu/dia/kami), without additional explanation.\n\n{}",
            glossary_prefix, cleaned_text
        );
    }

    format!(
        "{}Translate the following segment into {}, without additional explanation.\n\n{}",
        glossary_prefix, target_label, cleaned_text
    )
}

pub fn strip_hymt_artifacts(raw: &str) -> String {
    let mut cleaned = raw.trim().to_string();
    for token in [
        "<｜hy_end_of_assistant｜>",
        "<｜hy_end_of_sentence｜>",
        "<｜hy_place▁holder▁no▁2｜>",
        "<｜hy_place▁holder▁no▁3｜>",
        "<｜hy_place▁holder▁no▁8｜>",
        "<｜hy_Assistant｜>",
        "<｜hy_begin▁of▁sentence｜>",
        "<｜hy_User｜>",
        "<|startoftext|>",
        "<|extra_0|>",
        "<|extra_4|>",
        "<|eos|>",
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
        let non_id = build_hymt_user_prompt("Hello   world", None, "en", "es");
        assert!(non_id.contains("Translate the following segment into Spanish"));
        assert!(non_id.contains("Hello world"));

        let id_prompt = build_hymt_user_prompt("WHAT ARE YOU DOING?!", None, "en", "id");
        assert!(id_prompt.contains("casual Indonesian (using aku/kamu/dia/kami)"));
        assert!(id_prompt.contains("What are you doing?!"));

        let caps_honorific = build_hymt_user_prompt("TANAKA-SAN, ARE YOU OKAY? I AM FINE.", None, "en", "id");
        assert!(caps_honorific.contains("Tanaka-san, are you okay? I am fine."));
        assert!(caps_honorific.contains("are you 翻译成 apa kamu"));
        assert!(caps_honorific.contains("I am 翻译成 aku"));

        let ja_honorific = build_hymt_user_prompt("先輩、私は平気！", None, "auto", "id");
        assert!(ja_honorific.contains("先輩 翻译成 Senpai"));
        assert!(ja_honorific.contains("私は 翻译成 aku"));
        assert!(ja_honorific.contains("Translate the following comic dialogue into casual Indonesian"));

        let zh_pair = build_hymt_user_prompt("你好世界", None, "zh", "id");
        assert!(zh_pair.contains("将以下文本翻译为印尼语"));
        assert!(zh_pair.contains("你 翻译成 kamu"));

        let zh_contextual = build_hymt_user_prompt("我们现在快走吧！", Some("别放弃！"), "auto", "id");
        assert!(zh_contextual.contains("别放弃！"));
        assert!(zh_contextual.contains("把下面的文本翻译成印尼语"));
        assert!(zh_contextual.contains("我们 翻译成 kami"));

        let stripped = strip_hymt_artifacts("Ikuti aku!<｜hy_end_of_assistant｜>");
        assert_eq!(stripped, "Ikuti aku!");

        let stripped_v2 = strip_hymt_artifacts("Ayo pergi!<｜hy_place▁holder▁no▁2｜>");
        assert_eq!(stripped_v2, "Ayo pergi!");

        let stripped_v2_7b = strip_hymt_artifacts("Jangan lupa ya!<|eos|>");
        assert_eq!(stripped_v2_7b, "Jangan lupa ya!");
    }
}
