use crate::model::Bubble;

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
        return "the automatically detected source language".to_string();
    }
    resolve_language_name(trimmed, trimmed)
}

pub fn resolve_target_language_label(target_lang: &str) -> String {
    resolve_language_name(target_lang, "Indonesian")
}

fn uses_word_spaces(target_name: &str) -> bool {
    !matches!(
        target_name,
        "Japanese" | "Simplified Chinese" | "Traditional Chinese" | "Thai"
    )
}

fn conversational_style_rule(target_name: &str) -> String {
    if target_name == "Indonesian" {
        return "3. NATURAL CONVERSATIONAL TONE: Use modern, natural Indonesian comic dialogue (aku/kamu, udah, nggak, gimana, kayak, kok, sih, dong, ya). Avoid stiff, formal textbook language or literal machine translation.".to_string();
    }
    format!(
        "3. NATURAL CONVERSATIONAL TONE: Use modern, natural {} comic dialogue matching each character's personality, mood, and relationship. Avoid stiff textbook phrasing or literal word-for-word machine translation.",
        target_name
    )
}

fn idioms_rule(target_name: &str) -> String {
    if target_name == "Indonesian" {
        return "5. COMMON IDIOMATIC EXPRESSIONS:\n   - \"Excuse me?\" / \"Excuse me!\" is a polite query (\"Permisi?\") or an affronted reaction (\"Apa katamu?!\" / \"Maksudmu?!\"). NEVER translate literally as \"Alasan me\".".to_string();
    }
    format!(
        "5. NATURAL IDIOMS & EXPRESSIONS:\n   - Translate idioms, exclamations, and rhetorical phrases by intended meaning in {} rather than literal dictionary definitions (e.g., \"Excuse me?!\" can mean \"Pardon?\" or \"What did you just say?!\" depending on context).",
        target_name
    )
}

fn spacing_instruction(target_name: &str) -> String {
    if uses_word_spaces(target_name) {
        return format!(
            "ENSURE ALL WORDS IN {} ARE PROPERLY SEPARATED BY SPACES WITH NO MERGED WORDS!",
            target_name.to_uppercase()
        );
    }
    format!(
        "Follow natural {} orthography and punctuation without inserting unnatural Latin spacing.",
        target_name
    )
}

fn json_example_block(target_name: &str) -> String {
    if target_name == "Indonesian" {
        return r#"{
  "translations": [
    {"id": 1, "target": "Tapi kupikir dia udah dapet apa yang dia pesan?"},
    {"id": 2, "target": "Makasih udah ngizinin aku masuk."}
  ]
}"#
        .to_string();
    }

    format!(
        r#"{{
  "translations": [
    {{"id": 1, "target": "<Natural {target_name} translation for item 1>"}},
    {{"id": 2, "target": "<Natural {target_name} translation for item 2>"}}
  ]
}}"#
    )
}

pub fn build_scanlation_system_prompt(
    source_lang: &str,
    target_lang: &str,
    local_mode: bool,
) -> String {
    let source_label = resolve_source_language_label(source_lang);
    let target_label = resolve_target_language_label(target_lang);

    if local_mode {
        let local_register = if target_label == "Indonesian" {
            "- Close peers/friends: Use \"aku/kamu\" and casual conversational words (udah, nggak, gimana, kok, sih, dong, ya).\n   - Avoid stiff, formal textbook phrasing (\"Saya/Anda\" ONLY when speaking to nobles, royalty, or high officials).\n   - Honorifics: Preserve suffixes (-san, -kun, -chan, -sama, -senpai, -sensei) or adapt naturally (\"Tuan [Nama]\", \"Nona [Nama]\", \"Kak [Nama]\").\n   - \"Excuse me?\" / \"Excuse me!\" -> \"Permisi?\" or shock reaction \"Maksudmu apa?!\"."
                .to_string()
        } else {
            format!(
                "- Adapt honorifics and social hierarchy naturally into {target_label} (preserve iconic suffixes like -san/-kun/-chan/-sama/-senpai/-sensei when they fit character identity, or use appropriate {target_label} forms of address).\n   - Match speech register to the relationship (casual with peers/friends, respectful with elders, nobles, or superiors)."
            )
        };

        return format!(
            r#"You are an expert manga and comic scanlation translator translating dialogue from {source_label} into natural {target_label}.
Task: Translate every dialogue item accurately from {source_label} into fluent, expressive {target_label} suited for comic speech bubbles.

HONORIFIC & SCANLATION RULES:
1. HONORIFICS & REGISTER:
   {local_register}
2. FIX OCR MERGED TEXT & TYPOS:
   - Comic OCR input may drop spaces between words due to narrow comic font kerning (e.g., "wemust takea waterbucket" -> "we must take a water bucket", "anda love potion" -> "and a love potion").
   - Intelligently segment words and understand the intended dialogue in {source_label} before translating into {target_label}.
   - {spacing}
3. CONCISE & PANEL-FITTING:
   - Keep dialogue concise and punchy to fit neatly inside speech bubbles.
4. PRESERVE EMOTIONAL PUNCTUATION:
   - Keep ellipsis (...), question marks (?), exclamation marks (!), and tone intact.
5. REQUIRED JSON OUTPUT FORMAT (every value MUST be written in {target_label}):
{{
  "translations": [
    {{"id": 1, "target": "<Natural {target_label} translation>"}}
  ]
}}"#,
            spacing = spacing_instruction(&target_label)
        );
    }

    format!(
        r#"You are a professional comic and manga scanlation translator.
Your task is to translate comic dialogue from {source_label} into natural, fluent, and expressive {target_label} suited for conversational comic panels.

SCANLATION QUALITY STANDARDS:
1. CONCISE & FIT FOR SPEECH BUBBLES: Comic bubble space is strictly limited. Keep translations punchy and compact without omitting essential meaning.
2. AVOID REDUNDANCY: Do not repeat identical subjects or objects within the same speech bubble.
{tone_rule}
4. TOLERANCE FOR OCR MERGED WORDS & TYPOS:
   Input text originates from comic OCR in {source_label} which frequently drops spaces due to tight lettering (e.g. "wemust takea waterbucket" -> "we must take a water bucket", "hehas" -> "he has", "anda love potion" -> "and a love potion", "imiss you" -> "I miss you").
   Always segment and understand the intended source dialogue in context before translating.
   {spacing}
{idiom_rule}
6. PRESERVE EMOTIONAL PUNCTUATION: Maintain ellipsis (...), question marks (?), exclamation marks (!), and character expressiveness.
7. TARGET LANGUAGE STRICTNESS: Write ALL translated text exclusively in {target_label}.

OUTPUT FORMAT MUST BE A VALID JSON OBJECT WITH A 'translations' KEY:
{examples}"#,
        tone_rule = conversational_style_rule(&target_label),
        spacing = spacing_instruction(&target_label),
        idiom_rule = idioms_rule(&target_label),
        examples = json_example_block(&target_label),
    )
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
        .map(|b| serde_json::json!({ "id": b.id, "text": b.source_text }))
        .collect();

    format!(
        "Translate the following comic dialogue items from {} into natural {}:\n{}",
        source_label,
        target_label,
        serde_json::to_string_pretty(&input_items).unwrap_or_default()
    )
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
            source_text: "Hello world".to_string(),
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
        assert_eq!(
            resolve_source_language_label("auto"),
            "the automatically detected source language"
        );
        assert_eq!(resolve_source_language_label("ko"), "Korean");
    }

    #[test]
    fn builds_dynamic_target_and_source_prompts() {
        let english_prompt = build_scanlation_system_prompt("ja", "en", false);
        assert!(english_prompt.contains("from Japanese into natural, fluent, and expressive English"));
        assert!(!english_prompt.contains("Tapi kupikir"));
        assert!(english_prompt.contains("Natural English translation"));

        let indonesian_prompt = build_scanlation_system_prompt("auto", "id", false);
        assert!(indonesian_prompt.contains("automatically detected source language"));
        assert!(indonesian_prompt.contains("natural, fluent, and expressive Indonesian"));
        assert!(indonesian_prompt.contains("aku/kamu"));

        let japanese_local = build_scanlation_system_prompt("en", "ja", true);
        assert!(japanese_local.contains("from English into natural Japanese"));
        assert!(japanese_local.contains("without inserting unnatural Latin spacing"));

        let user_prompt = build_translation_user_prompt(&[sample_bubble()], "ko", "es");
        assert!(user_prompt.contains("from Korean into natural Spanish"));
        assert!(user_prompt.contains("Hello world"));
    }
}
