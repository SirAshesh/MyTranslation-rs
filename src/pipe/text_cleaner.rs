use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;

fn cached_regex<'a>(slot: &'a OnceLock<Option<Regex>>, pattern: &str) -> Option<&'a Regex> {
    slot.get_or_init(|| Regex::new(pattern).ok()).as_ref()
}

fn compile_replacement_table(
    entries: &[(&'static str, &'static str)],
) -> Vec<(Regex, &'static str)> {
    entries
        .iter()
        .filter_map(|&(pat, rep)| Regex::new(pat).ok().map(|re| (re, rep)))
        .collect()
}

fn is_digits_and_ascii_letters(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() < 2 {
        return false;
    }
    let starts_digit = bytes[0].is_ascii_digit();
    if !starts_digit && !bytes[0].is_ascii_alphabetic() {
        return false;
    }
    let mut switched = false;
    for &b in &bytes[1..] {
        let is_digit = b.is_ascii_digit();
        let is_alpha = b.is_ascii_alphabetic();
        if !is_digit && !is_alpha {
            return false;
        }
        if !switched {
            if is_digit != starts_digit {
                switched = true;
            }
        } else if is_digit == starts_digit {
            return false;
        }
    }
    switched
}

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

fn trailing_alpha_token(text: &str) -> &str {
    let trimmed = text.trim_end();
    let start = trimmed
        .rfind(|c: char| !c.is_alphabetic())
        .map_or(0, |idx| {
            idx + trimmed[idx..].chars().next().map_or(1, char::len_utf8)
        });
    &trimmed[start..]
}

fn leading_alpha_token(text: &str) -> &str {
    let trimmed = text.trim_start();
    let end = trimmed
        .find(|c: char| !c.is_alphabetic())
        .unwrap_or(trimmed.len());
    &trimmed[..end]
}

fn is_honorific_suffix(token: &str) -> bool {
    let lower = token.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "san" | "kun" | "chan" | "sama" | "dono" | "senpai" | "sensei"
    )
}

fn is_stuttered_hyphen_break(prev: &str, next: &str) -> bool {
    let prev_lower = prev.to_ascii_lowercase();
    let next_lower = next.to_ascii_lowercase();
    if prev_lower.is_empty() || next_lower.is_empty() {
        return false;
    }
    if prev_lower.len() == 1 {
        return next_lower.starts_with(&prev_lower);
    }
    if next_lower.starts_with(&prev_lower) {
        return true;
    }
    if let Some(first_ch) = next_lower.chars().next() {
        let rest = next_lower.trim_start_matches(first_ch);
        let collapsed = format!("{}{}", first_ch, rest);
        if collapsed.starts_with(&prev_lower) {
            return true;
        }
    }
    false
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
        } else if last == '-'
            && result
                .chars()
                .nth_back(1)
                .map_or(false, |c| c.is_alphabetic())
        {
            let prev_token = trailing_alpha_token(&result[..result.len() - 1]);
            let next_token = leading_alpha_token(trimmed);
            if is_honorific_suffix(next_token) {
                result.push_str(trimmed);
            } else if is_stuttered_hyphen_break(prev_token, next_token) {
                result.push(' ');
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
    let stripped_lower = stripped.to_lowercase();
    if stripped_lower.contains("mangayyorg") || stripped_lower.contains("erisscans") {
        return true;
    }

    let all_num = clean.chars().all(|c| c.is_numeric()) || stripped.chars().all(|c| c.is_numeric());
    if all_num {
        let has_num_dialogue_punct =
            clean.contains("...") || clean.contains('?') || clean.contains('!');
        if !has_num_dialogue_punct
            || score < 0.85
            || stripped.len() > 4
            || stripped.is_empty()
            || stripped == "000"
            || stripped == "00"
            || (w > 85.0 && h > 85.0)
            || (stripped.len() == 1 && (w < 20.0 || h < 16.0 || !clean.contains("...")))
        {
            return true;
        }
        return false;
    }

    let is_single_token = !clean.chars().any(char::is_whitespace);
    if is_single_token
        && stripped.len() <= 5
        && score < 0.85
        && is_digits_and_ascii_letters(&stripped)
    {
        return true;
    }

    if contains_cjk(clean) {
        if clean.chars().count() == 1 {
            let ch = clean.chars().next().unwrap_or('\0');
            if matches!(
                ch,
                '口' | '中' | '一' | '十' | '日' | '田' | '个' | '卜' | '丁' | '丨' | '亅' | '乙'
            ) {
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

    if is_single_token
        && matches!(
            stripped_lower.as_str(),
            "kyt"
                | "erxi"
                | "yotot"
                | "yorot"
                | "otor"
                | "yor"
                | "ktoo"
                | "yloh"
                | "myok"
                | "tritm"
                | "rim"
                | "wok"
        )
    {
        return true;
    }

    if is_single_token && h > 85.0 && w > 85.0 {
        let is_known_exclamation = matches!(
            stripped_lower.as_str(),
            "huh" | "what" | "no" | "wait" | "stop" | "hey" | "ah" | "aah" | "oh"
        );
        if !is_known_exclamation {
            return true;
        }
    }

    let valid_short: HashSet<&'static str> = [
        "NO", "OK", "GO", "HI", "OH", "AH", "ME", "MY", "HE", "WE", "SO", "DO", "IF", "IN", "ON",
        "AT", "TO", "UP", "BY", "IS", "AM", "US", "IT", "SI", "YA", "DA", "JA", "TU", "LE", "LA",
        "DE", "UN", "VA", "HA", "SE", "ET", "OU", "IO", "NE", "CI", "VI", "DI", "MA", "SA", "MI",
        "TE", "TI", "ES", "EN", "EL", "YO", "PA", "AL", "DU",
    ]
    .into_iter()
    .collect();

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

    let has_dialogue_punct =
        clean.contains("...") || clean.contains("..") || clean.contains('?') || clean.contains('!');
    let vocal_interjections: HashSet<&'static str> = [
        "NGH", "NNGH", "MMM", "HMM", "UHH", "AHH", "OHH", "EHH", "HUH", "UGH", "GAAH", "ARGH",
        "HMPH", "PHEW",
    ]
    .into_iter()
    .collect();

    let words: Vec<String> = clean
        .split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_uppercase()
        })
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return true;
    }

    let is_vocal_dialogue = (score >= 0.80 || (has_dialogue_punct && score >= 0.55))
        && words
            .iter()
            .all(|w| vocal_interjections.contains(w.as_str()));

    if char_count <= 6 && score < 0.60 && !is_vocal_dialogue {
        return true;
    }

    if char_count <= 4 && score < 0.76 && (w < 55.0 || h < 18.0) && !is_vocal_dialogue {
        return true;
    }

    if h < 15.0 && score < 0.80 && !is_vocal_dialogue {
        return true;
    }

    static PAREN_RE: OnceLock<Option<Regex>> = OnceLock::new();
    if let Some(caps) = cached_regex(&PAREN_RE, r"[\(\[\{]([A-Za-z\s\-]{2,16})[\)\]\}]")
        .and_then(|re| re.captures(clean))
    {
        let inner = caps[1].to_uppercase();
        let sfx_terms = [
            "WHOOSH", "GAPE", "FWIP", "THUD", "TWITCH", "CLENCH", "SIGH", "GASP", "STEP", "CREAK",
            "BLUSH", "SHIVER", "DRIP", "POKE",
        ];
        if sfx_terms.iter().any(|&term| inner.contains(term)) {
            return true;
        }
    }

    if is_vocal_dialogue {
        return false;
    }

    let known_sfx: HashSet<&'static str> = [
        "POKAAAN", "POKAN", "POKA", "POK", "BIKU", "PIKU", "GIKU", "PEKO", "ZUKI", "KIRI", "CKIRI",
        "ZU", "ZUZU", "SHFD", "DOKI", "PATA", "NIKORI", "DODODO", "GOGOGO", "FWIP", "WHOOSH",
        "THUD", "GULP", "WHIRL", "TWITCH", "GAPE", "CHUUSHD", "CLENCH", "SHIIN", "CHIRA", "GYU",
        "DOSA", "PASA", "WINCE", "BA", "BAA", "BASH", "GLL", "PAT", "COLTD", "CTWITCHD",
        "CCLENCHD", "HMPH", "TCH", "TSK", "PFFT", "ACK", "AGH", "ARGH", "UGH", "GAAH", "KAH",
        "KHA", "NGH", "NNGH", "OOF", "RS", "ACR", "FW", "FLL", "FLLS", "IP", "FOI", "SCURRY",
        "CHATTER", "CHATER", "CHALTER", "PRH", "PRCH", "SICH", "SCH", "HAN", "HAAH", "TWTCH",
        "SQUI", "SQUL", "DUM", "NOOM", "OIP", "SPLISH", "SWOOSH", "SPU", "ASE", "KINCH", "CENCH",
        "SLIP", "SLIDE", "SMACK", "SLAP", "HAA", "FUU", "FUUH", "PHEW",
    ]
    .into_iter()
    .collect();

    let sfx_substrings = [
        "FWIP", "WHOOSH", "CLENCH", "TWITCH", "GAPE", "THUD", "GULP", "POKAAAN", "POKAN", "DOKI",
        "HMPH", "PFFT", "CHIRA", "NIKORI", "SCURRY", "CHATTER",
    ];
    let allow_vocal_token = |w: &String| -> bool {
        (score >= 0.80 || has_dialogue_punct) && vocal_interjections.contains(w.as_str())
    };
    if words.len() <= 2
        && words
            .iter()
            .any(|w| !allow_vocal_token(w) && sfx_substrings.iter().any(|&sub| w.contains(sub)))
    {
        return true;
    }

    let non_sfx_words: Vec<&String> = words
        .iter()
        .filter(|w| {
            allow_vocal_token(w)
                || (!known_sfx.contains(w.as_str())
                    && w.as_str() != "A"
                    && w.as_str() != "AN"
                    && w.as_str() != "THE")
        })
        .collect();

    if non_sfx_words.is_empty() || non_sfx_words.iter().all(|w| w.chars().count() <= 1) {
        return true;
    }

    if words
        .iter()
        .any(|w| w == "ACR" || w == "RS" || w == "FLL" || w == "FLLS" || w == "FW" || w == "FOI")
        && words.len() <= 3
    {
        return true;
    }

    if words.len() <= 2
        && words
            .iter()
            .all(|w| has_repeated_letters(w) && !allow_vocal_token(w))
    {
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

    static SPACE_RE: OnceLock<Option<Regex>> = OnceLock::new();
    if contains_cjk(&s) || contains_cjk_or_hangul(&s) {
        s = s.replace('\u{3000}', " ");
        return match cached_regex(&SPACE_RE, r"\s+") {
            Some(re) => re.replace_all(&s, " ").trim().to_string(),
            None => s.trim().to_string(),
        };
    }

    static HYPHEN_SPACE_RE: OnceLock<Option<Regex>> = OnceLock::new();
    static NORMALIZE_TABLE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let table = NORMALIZE_TABLE.get_or_init(|| {
        compile_replacement_table(&[
            (
                r"(?i)\b([A-Za-z]+)\s*[-~—]?\s*(san|sama|kun|chan|senpai|sensei|dono)\b",
                "$1-$2",
            ),
            (r"\b1\b", "I"),
            (r"\b1('m|'ll|'d|'ve)\b", "I$1"),
            (r"(?i)\bcan'ti\b", "Can't I"),
            (r"(?i)\bw1fe\b", "wife"),
            (r"(?i)\bw1th\b", "with"),
            (r"(?i)\bhu5band\b", "husband"),
            (r"(?i)\bfeep\b", "keep"),
            (r"\b([a-zA-Z]{2,})\.([a-zA-Z]{2,})\b", "$1$2"),
            (r"\b([a-zA-Z]+)\.([A-Z][a-zA-Z]+)\b", "$1. $2"),
            (r"/\s*([a-zA-Z]+)", " $1"),
            (r"\+([a-zA-Z]+)", "$1"),
            (r"\s+", " "),
        ])
    });
    if let Some(hyphen_re) = cached_regex(&HYPHEN_SPACE_RE, r"\b([A-Za-z]+)-\s+([A-Za-z]+)\b") {
        s = hyphen_re
            .replace_all(&s, |caps: &regex::Captures| {
                let left = &caps[1];
                let right = &caps[2];
                if is_honorific_suffix(right) || is_stuttered_hyphen_break(left, right) {
                    caps[0].to_string()
                } else {
                    format!("{}{}", left, right)
                }
            })
            .into_owned();
    }
    static TRAILING_HYPHEN_RE: OnceLock<Option<Regex>> = OnceLock::new();
    if let Some(trailing_re) = cached_regex(&TRAILING_HYPHEN_RE, r"\b([A-Za-z]+)-\s*$") {
        s = trailing_re.replace_all(&s, "$1").into_owned();
    }
    for (re, rep) in table {
        s = re.replace_all(&s, *rep).into_owned();
    }
    s = s.trim().to_string();

    let alpha_count = s.chars().filter(|c| c.is_alphabetic()).count();
    let upper_count = s.chars().filter(|c| c.is_uppercase()).count();

    if alpha_count > 0 && (upper_count as f32 / alpha_count as f32) > 0.60 {
        static SENT_RE: OnceLock<Option<Regex>> = OnceLock::new();
        if let Some(sent_re) = cached_regex(&SENT_RE, r"([.?!]+\s*)") {
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

    static SENTENCE_TABLE: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let fixes = SENTENCE_TABLE.get_or_init(|| {
        compile_replacement_table(&[
            (r"\bi\b", "I"),
            (r"\bi'm\b", "I'm"),
            (r"\bi'll\b", "I'll"),
            (r"\bi'd\b", "I'd"),
            (r"\bi've\b", "I've"),
        ])
    });
    for (re, rep) in fixes {
        formatted = re.replace_all(&formatted, *rep).into_owned();
    }

    static HON_CAP_RE: OnceLock<Option<Regex>> = OnceLock::new();
    if let Some(hon_cap_re) = cached_regex(
        &HON_CAP_RE,
        r"\b([a-z]+)-(san|sama|kun|chan|senpai|sensei|dono)\b",
    ) {
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
    }

    formatted
}

#[cfg(test)]
#[path = "text_cleaner_tests.rs"]
mod tests;
