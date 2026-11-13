use regex::Regex;
use std::collections::HashSet;

pub fn is_cjk_char(c: char) -> bool {
    matches!(c,
        '\u{3040}'..='\u{309F}' |
        '\u{30A0}'..='\u{30FF}' |
        '\u{4E00}'..='\u{9FFF}' |
        '\u{3400}'..='\u{4DBF}' |
        '\u{3000}'..='\u{303F}' |
        '\u{FF00}'..='\u{FFEF}'
    )
}

pub fn is_hangul_char(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7AF}' | '\u{1100}'..='\u{11FF}' | '\u{3130}'..='\u{318F}')
}

pub fn contains_cjk(s: &str) -> bool {
    s.chars().any(is_cjk_char)
}

pub fn contains_cjk_or_hangul(s: &str) -> bool {
    s.chars().any(|c| is_cjk_char(c) || is_hangul_char(c))
}

pub fn join_manga_lines(lines: &[&str]) -> String {
    let mut result = String::new();
    for &line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if result.is_empty() {
            result.push_str(trimmed);
            continue;
        }

        let last = result.chars().last().unwrap_or('\0');
        let next = trimmed.chars().next().unwrap_or('\0');

        if is_cjk_char(last) || is_cjk_char(next) {
            if last == '-' {
                result.pop();
            }
            result.push_str(trimmed);
        } else if last == '-' && result.chars().nth_back(1).map_or(false, |c| c.is_alphabetic()) {
            let next_lower = trimmed.to_lowercase();
            let is_honorific = next_lower.starts_with("san")
                || next_lower.starts_with("kun")
                || next_lower.starts_with("chan")
                || next_lower.starts_with("sama")
                || next_lower.starts_with("dono")
                || next_lower.starts_with("senpai")
                || next_lower.starts_with("sensei");
            if is_honorific {
                result.push_str(trimmed);
            } else {
                result.pop();
                result.push_str(trimmed);
            }
        } else {
            result.push(' ');
            result.push_str(trimmed);
        }
    }
    result
}

fn has_repeated_letters(w: &str) -> bool {
    let mut count = 0;
    let mut prev = '\0';
    for c in w.chars() {
        if c == prev && c.is_alphabetic() {
            count += 1;
            if count >= 3 {
                return true;
            }
        } else {
            prev = c;
            count = 1;
        }
    }
    false
}

pub fn is_junk_or_sfx(text: &str, score: f32, w: f32, h: f32) -> bool {
    let clean = text.trim();
    if clean.is_empty() {
        return true;
    }

    let stripped: String = clean.chars().filter(|c| c.is_alphanumeric()).collect();
    if stripped.is_empty() {
        return true;
    }

    let all_num = clean.chars().all(|c| c.is_numeric()) || stripped.chars().all(|c| c.is_numeric());
    if all_num {
        if score < 0.85 || stripped.len() > 4 || stripped.is_empty() || stripped == "000" || stripped == "00" {
            return true;
        }
        if stripped.len() == 1 && (w < 20.0 || h < 16.0 || !clean.contains("...")) {
            return true;
        }
        return false;
    }

    let is_single_token = !clean.chars().any(char::is_whitespace);
    let mixed_re = Regex::new(r"^\d+[A-Za-z]+$|^[A-Za-z]+\d+$").unwrap();
    if is_single_token && stripped.len() <= 5 && score < 0.85 && mixed_re.is_match(&stripped) {
        return true;
    }

    if contains_cjk(clean) {
        if clean.chars().count() == 1 {
            let ch = clean.chars().next().unwrap_or('\0');
            if matches!(ch, '口' | '中' | '一' | '十' | '日' | '田' | '个' | '卜' | '丁' | '丨' | '亅' | '乙') {
                return true;
            }
            if score < 0.60 {
                return true;
            }
        }
        return score < 0.35;
    }

    if contains_cjk_or_hangul(clean) {
        return score < 0.40;
    }

    let valid_short: HashSet<&'static str> = [
        "NO", "OK", "GO", "HI", "OH", "AH", "ME", "MY", "HE", "WE", "SO",
        "DO", "IF", "IN", "ON", "AT", "TO", "UP", "BY", "IS", "AM", "US", "IT",
        "SI", "YA", "DA", "JA", "TU", "LE", "LA", "DE", "UN", "VA", "HA", "SE",
        "ET", "OU", "IO", "NE", "CI", "VI", "DI", "MA", "SA", "MI", "TE", "TI",
        "ES", "EN", "EL", "YO", "PA", "AL", "DU"
    ].into_iter().collect();

    let char_count = stripped.chars().count();
    if char_count <= 2 {
        let up = stripped.to_uppercase();
        if !valid_short.contains(up.as_str()) && up != "I" && up != "A" {
            return true;
        }
        if score < 0.85 && (up == "I" || up == "A") {
            return true;
        }
        if score < 0.72 {
            return true;
        }
    }

    if char_count <= 6 && score < 0.60 {
        return true;
    }

    if char_count <= 4 && score < 0.76 && (w < 55.0 || h < 18.0) {
        return true;
    }

    if h < 15.0 && score < 0.80 {
        return true;
    }

    let paren_re = Regex::new(r"[\(\[\{]([A-Za-z\s\-]{2,16})[\)\]\}]").unwrap();
    if let Some(caps) = paren_re.captures(clean) {
        let inner = caps[1].to_uppercase();
        let sfx_terms = [
            "WHOOSH", "GAPE", "FWIP", "THUD", "TWITCH", "CLENCH", "SIGH",
            "GASP", "STEP", "CREAK", "BLUSH", "SHIVER", "DRIP", "POKE"
        ];
        if sfx_terms.iter().any(|&term| inner.contains(term)) {
            return true;
        }
    }

    let words: Vec<String> = clean
        .split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_uppercase())
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return true;
    }

    let known_sfx: HashSet<&'static str> = [
        "POKAAAN", "POKAN", "POKA", "POK", "BIKU", "PIKU", "GIKU", "PEKO", "ZUKI", "KIRI", "CKIRI",
        "ZU", "ZUZU", "SHFD", "DOKI", "PATA", "NIKORI", "DODODO", "GOGOGO",
        "FWIP", "WHOOSH", "THUD", "GULP", "WHIRL", "TWITCH", "GAPE", "CHUUSHD", "CLENCH",
        "SHIIN", "CHIRA", "GYU", "DOSA", "PASA", "WINCE", "BA", "BAA", "BASH",
        "GLL", "PAT", "COLTD", "CTWITCHD", "CCLENCHD", "HMPH", "TCH", "TSK", "PFFT",
        "ACK", "AGH", "ARGH", "UGH", "GAAH", "KAH", "KHA", "NGH", "NNGH", "OOF",
        "RS", "ACR", "FW", "FLL", "FLLS", "IP", "FOI", "SCURRY", "CHATTER", "CHATER", "CHALTER",
        "PRH", "PRCH", "SICH", "SCH", "HAN", "HAAH", "TWTCH", "SQUI", "SQUL", "DUM", "NOOM",
        "OIP", "SPLISH", "SWOOSH", "SPU", "ASE", "KINCH", "CENCH",
        "SLIP", "SLIDE", "SMACK", "SLAP", "HAA", "FUU", "FUUH", "PHEW"
    ].into_iter().collect();

    let sfx_substrings = [
        "FWIP", "WHOOSH", "CLENCH", "TWITCH", "GAPE", "THUD", "GULP",
        "POKAAAN", "POKAN", "DOKI", "HMPH", "PFFT", "CHIRA", "NIKORI", "SCURRY", "CHATTER"
    ];
    if words.len() <= 2 && words.iter().any(|w| sfx_substrings.iter().any(|&sub| w.contains(sub))) {
        return true;
    }

    let non_sfx_words: Vec<&String> = words
        .iter()
        .filter(|w| !known_sfx.contains(w.as_str()) && w.as_str() != "A" && w.as_str() != "AN" && w.as_str() != "THE")
        .collect();

    if non_sfx_words.is_empty() || non_sfx_words.iter().all(|w| w.chars().count() <= 1) {
        return true;
    }

    if words.iter().any(|w| w == "ACR" || w == "RS" || w == "FLL" || w == "FLLS" || w == "FW" || w == "FOI") && words.len() <= 3 {
        return true;
    }

    if words.len() <= 2 && words.iter().all(|w| has_repeated_letters(w)) {
        return true;
    }

    if h > w * 2.2 && clean.len() > 1 && score < 0.75 {
        return true;
    }

    false
}

pub fn normalize_source_text(text: &str) -> String {
    let mut s = text.trim().to_string();
    if s.is_empty() {
        return s;
    }

    if contains_cjk(&s) || contains_cjk_or_hangul(&s) {
        s = s.replace('\u{3000}', " ");
        let space_re = Regex::new(r"\s+").unwrap();
        return space_re.replace_all(&s, " ").trim().to_string();
    }

    let honorific_dash_re = Regex::new(r"(?i)\b([A-Za-z]+)\s*[-~—]?\s*(san|sama|kun|chan|senpai|sensei|dono)\b").unwrap();
    s = honorific_dash_re.replace_all(&s, "$1-$2").to_string();

    let hyphen_re = Regex::new(r"\b([A-Za-z]+)-\s+([A-Za-z]+)\b").unwrap();
    s = hyphen_re.replace_all(&s, "$1$2").to_string();

    let trailing_hyphen_re = Regex::new(r"\b([A-Za-z]+)-(?:\s+|$)").unwrap();
    s = trailing_hyphen_re.replace_all(&s, "$1 ").to_string();

    let one_re = Regex::new(r"\b1\b").unwrap();
    s = one_re.replace_all(&s, "I").to_string();
    let one_contract_re = Regex::new(r"\b1('m|'ll|'d|'ve)\b").unwrap();
    s = one_contract_re.replace_all(&s, "I$1").to_string();

    let cant_re = Regex::new(r"(?i)\bcan'ti\b").unwrap();
    s = cant_re.replace_all(&s, "Can't I").to_string();
    let w1fe_re = Regex::new(r"(?i)\bw1fe\b").unwrap();
    s = w1fe_re.replace_all(&s, "wife").to_string();
    let w1th_re = Regex::new(r"(?i)\bw1th\b").unwrap();
    s = w1th_re.replace_all(&s, "with").to_string();
    let hu5_re = Regex::new(r"(?i)\bhu5band\b").unwrap();
    s = hu5_re.replace_all(&s, "husband").to_string();
    let feep_re = Regex::new(r"(?i)\bfeep\b").unwrap();
    s = feep_re.replace_all(&s, "keep").to_string();

    let dot_word_re = Regex::new(r"\b([a-zA-Z]{2,})\.([a-zA-Z]{2,})\b").unwrap();
    s = dot_word_re.replace_all(&s, "$1$2").to_string();

    let dot_space_re = Regex::new(r"\b([a-zA-Z]+)\.([A-Z][a-zA-Z]+)\b").unwrap();
    s = dot_space_re.replace_all(&s, "$1. $2").to_string();

    let slash_re = Regex::new(r"/\s*([a-zA-Z]+)").unwrap();
    s = slash_re.replace_all(&s, " $1").to_string();
    let plus_re = Regex::new(r"\+([a-zA-Z]+)").unwrap();
    s = plus_re.replace_all(&s, "$1").to_string();

    let space_re = Regex::new(r"\s+").unwrap();
    s = space_re.replace_all(&s, " ").trim().to_string();

    let alpha_count = s.chars().filter(|c| c.is_alphabetic()).count();
    let upper_count = s.chars().filter(|c| c.is_uppercase()).count();

    if alpha_count > 0 && (upper_count as f32 / alpha_count as f32) > 0.60 {
        let sent_re = Regex::new(r"([.?!]+\s*)").unwrap();
        let mut result = String::new();
        let mut last_idx = 0;

        for mat in sent_re.find_iter(&s) {
            let chunk = &s[last_idx..mat.start()];
            let punct = mat.as_str();
            result.push_str(&format_sentence(chunk));
            result.push_str(punct);
            last_idx = mat.end();
        }

        if last_idx < s.len() {
            let tail = &s[last_idx..];
            result.push_str(&format_sentence(tail));
        }

        s = result;
    }

    s
}

fn format_sentence(chunk: &str) -> String {
    let trimmed = chunk.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lower = trimmed.to_lowercase();
    let mut chars = lower.chars();
    let mut formatted = match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    };

    let i_re = Regex::new(r"\bi\b").unwrap();
    formatted = i_re.replace_all(&formatted, "I").to_string();
    let im_re = Regex::new(r"\bi'm\b").unwrap();
    formatted = im_re.replace_all(&formatted, "I'm").to_string();
    let ill_re = Regex::new(r"\bi'll\b").unwrap();
    formatted = ill_re.replace_all(&formatted, "I'll").to_string();
    let id_re = Regex::new(r"\bi'd\b").unwrap();
    formatted = id_re.replace_all(&formatted, "I'd").to_string();
    let ive_re = Regex::new(r"\bi've\b").unwrap();
    formatted = ive_re.replace_all(&formatted, "I've").to_string();

    let hon_cap_re = Regex::new(r"\b([a-z]+)-(san|sama|kun|chan|senpai|sensei|dono)\b").unwrap();
    formatted = hon_cap_re
        .replace_all(&formatted, |caps: &regex::Captures| {
            let name = &caps[1];
            let suffix = &caps[2];
            let mut name_chars = name.chars();
            let cap_name = match name_chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + name_chars.as_str(),
            };
            format!("{}-{}", cap_name, suffix)
        })
        .to_string();

    formatted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cjk_detection() {
        assert!(contains_cjk("こんにちは"));
        assert!(contains_cjk("何で？"));
        assert!(contains_cjk("这不仅是测试"));
        assert!(!contains_cjk("Hello world!"));
        assert!(!contains_cjk("Apa kabar"));
    }

    #[test]
    fn test_join_manga_lines() {
        let ja_lines = ["「お前を", "絶対に許さない」"];
        assert_eq!(join_manga_lines(&ja_lines), "「お前を絶対に許さない」");

        let en_lines = ["HE HAS NO", "REASON"];
        assert_eq!(join_manga_lines(&en_lines), "HE HAS NO REASON");

        let hyphen_lines = ["SOME-", "THING"];
        assert_eq!(join_manga_lines(&hyphen_lines), "SOMETHING");
    }

    #[test]
    fn test_cjk_not_rejected_as_junk() {
        assert!(!is_junk_or_sfx("何", 0.70, 20.0, 100.0));
        assert!(!is_junk_or_sfx("誰？", 0.75, 25.0, 80.0));
        assert!(!is_junk_or_sfx("嘘…", 0.80, 20.0, 60.0));
    }

    #[test]
    fn test_sentence_capitalization() {
        let raw = "HE HAS NO REASON TO COME OUT HERE ANYMORE.";
        assert_eq!(normalize_source_text(raw), "He has no reason to come out here anymore.");
    }

    #[test]
    fn test_manhwa_dialogue_with_numbers_and_names_not_dropped() {
        assert!(!is_junk_or_sfx("LORD BLACKWOOD", 1.0, 247.0, 24.0));
        assert!(!is_junk_or_sfx("LORD BLACKWOOD,", 1.0, 302.0, 29.0));
        assert!(!is_junk_or_sfx("7 LONG YEARS.", 1.0, 204.0, 28.0));
        assert!(!is_junk_or_sfx("2 MILLION", 0.99, 138.0, 27.0));
        assert!(!is_junk_or_sfx("2 WEEKS SINCE YOU", 0.98, 256.0, 24.0));
        assert!(!is_junk_or_sfx("ATTACK NOW!", 0.99, 180.0, 24.0));
        assert!(!is_junk_or_sfx("ACROSS THE HALL", 0.99, 210.0, 24.0));
    }

    #[test]
    fn test_low_confidence_artwork_noise_dropped() {
        assert!(is_junk_or_sfx("Go", 0.47, 45.0, 25.0));
        assert!(is_junk_or_sfx("On", 0.54, 150.0, 80.0));
        assert!(is_junk_or_sfx("poond", 0.45, 36.0, 14.0));
        assert!(is_junk_or_sfx("JDRU", 0.50, 61.0, 24.0));
        assert!(is_junk_or_sfx("L GKI", 0.57, 83.0, 10.0));
        assert!(is_junk_or_sfx("GeG", 0.71, 52.0, 29.5));
        assert!(is_junk_or_sfx("cors", 0.75, 21.8, 17.6));
        assert!(!is_junk_or_sfx("GO", 0.96, 52.0, 26.0));
    }
}
