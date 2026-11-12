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
            result.pop();
            result.push_str(trimmed);
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

    if clean.chars().all(|c| c.is_numeric()) || stripped.chars().all(|c| c.is_numeric()) {
        return true;
    }

    let mixed_re = Regex::new(r"^\d+[A-Za-z]+$|^[A-Za-z]+\d+$").unwrap();
    if mixed_re.is_match(&stripped) {
        return true;
    }

    if contains_cjk(clean) {
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
        if !valid_short.contains(up.as_str()) && score < 0.88 {
            return true;
        }
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
        "RS", "ACR", "FW", "FLL", "FLLS", "IP", "FOI"
    ].into_iter().collect();

    let sfx_substrings = [
        "FWIP", "WHOOSH", "ACR", "CLENCH", "TWITCH", "GAPE", "THUD", "GULP", "ACK",
        "POKAAAN", "POKAN", "DOKI", "HMPH", "PFFT", "CHIRA", "NIKORI"
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

fn split_glued_words(text: &str) -> String {
    const PREFIXES: &[&str] = &[
        "he", "she", "it", "we", "they", "you", "who", "what", "where", "when", "why", "how",
        "there", "this", "that", "these", "those",
        "to", "of", "in", "for", "on", "with", "at", "by", "from", "about",
        "and", "but", "so", "if", "as", "or",
        "do", "did", "does", "can", "could", "will", "would", "shall", "should", "may", "might", "must",
        "have", "had", "has", "is", "was", "are", "were", "am",
        "no", "not", "all", "some", "any", "my", "your", "his", "her", "our", "their",
        "out", "come", "get", "look", "go",
        "i", "a",
    ];

    const SUFFIXES: &[&str] = &[
        "has", "have", "had", "is", "was", "are", "were", "be", "been", "being",
        "do", "did", "does", "done", "can", "could", "will", "would", "shall", "should",
        "may", "might", "must",
        "come", "came", "go", "went", "gone", "see", "saw", "seen", "say", "said",
        "know", "knew", "known", "want", "wanted", "need", "needed", "think", "thought",
        "feel", "felt", "tell", "told", "make", "made", "take", "took", "get", "got",
        "give", "gave", "find", "found", "hear", "heard", "look", "looked", "ask", "asked",
        "love", "potion", "person", "time", "day", "one", "thing", "way", "first", "next",
        "even", "more", "not", "no", "a", "an", "the", "to", "in",
        "on", "at", "for", "of", "with", "from", "by", "up", "out", "off", "all", "so",
        "too", "just", "now", "then", "here", "there", "about", "around", "over", "under",
        "again", "ever", "never", "always", "really", "sure", "well", "good", "bad",
        "spend", "bring", "miss",
    ];

    const DO_NOT_SPLIT: &[&str] = &[
        "anymore", "into", "onto", "upon", "today", "tonight", "tomorrow", "together", "toward", "towards",
        "without", "within", "cannot", "anyone", "anything", "anywhere", "anyway", "anyhow", "anytime",
        "someone", "something", "somewhere", "somehow", "someway", "somebody",
        "everyone", "everything", "everywhere", "everybody",
        "nobody", "nothing", "nowhere", "maybe", "already", "almost", "always", "also", "although",
        "inside", "outside", "before", "behind", "beside", "between", "beyond",
        "sometimes", "someday", "itself", "himself", "herself", "myself", "yourself", "yourselves",
        "themselves", "ourselves", "oneself", "whatever", "whenever", "wherever", "whoever", "whichever",
        "however", "forever", "nearby", "overall", "welcome", "another", "other", "otherwise",
        "become", "became", "because", "forth", "forthcoming", "reason", "season", "person",
        "about", "around", "where", "there", "these", "those", "their", "under", "never",
        "often", "after", "being", "shall", "should", "could", "would", "which", "while",
    ];

    let mut result_words = Vec::new();

    for token in text.split_whitespace() {
        let trimmed_leading = token.trim_start_matches(|c: char| !c.is_alphanumeric());
        let leading_len = token.len() - trimmed_leading.len();
        let core = trimmed_leading.trim_end_matches(|c: char| !c.is_alphanumeric());
        let trailing_len = trimmed_leading.len() - core.len();

        let leading = &token[..leading_len];
        let trailing = &token[token.len() - trailing_len..];

        if core.len() < 3 {
            result_words.push(token.to_string());
            continue;
        }

        let lower = core.to_lowercase();
        if DO_NOT_SPLIT.contains(&lower.as_str()) {
            result_words.push(token.to_string());
            continue;
        }

        let mut split_found = false;
        for &p in PREFIXES {
            if lower.starts_with(p) && lower.len() > p.len() {
                let rest = &lower[p.len()..];
                if SUFFIXES.contains(&rest) || DO_NOT_SPLIT.contains(&rest) {
                    let is_all_upper = core.chars().all(|c| c.is_uppercase() || !c.is_alphabetic());
                    let is_title = core.chars().next().map_or(false, |c| c.is_uppercase())
                        && core.chars().skip(1).all(|c| c.is_lowercase() || !c.is_alphabetic());

                    let p1 = if is_all_upper {
                        p.to_uppercase()
                    } else if p == "i" {
                        "I".to_string()
                    } else if is_title {
                        let mut c = p.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        }
                    } else {
                        p.to_string()
                    };

                    let p2 = if is_all_upper {
                        rest.to_uppercase()
                    } else if is_title {
                        let mut c = rest.chars();
                        match c.next() {
                            None => String::new(),
                            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                        }
                    } else {
                        rest.to_string()
                    };

                    result_words.push(format!("{}{} {}{}", leading, p1, p2, trailing));
                    split_found = true;
                    break;
                }
            }
        }

        if !split_found {
            result_words.push(token.to_string());
        }
    }

    result_words.join(" ")
}

pub fn normalize_source_text(text: &str) -> String {
    let mut s = text.trim().to_string();
    if s.is_empty() {
        return s;
    }

    if contains_cjk(&s) {
        s = s.replace('\u{3000}', " ");
        let space_re = Regex::new(r"\s+").unwrap();
        return space_re.replace_all(&s, " ").trim().to_string();
    }

    if contains_cjk_or_hangul(&s) {
        let space_re = Regex::new(r"\s+").unwrap();
        return space_re.replace_all(&s, " ").trim().to_string();
    }

    let hyphen_re = Regex::new(r"\b([A-Za-z]+)-\s+([A-Za-z]+)\b").unwrap();
    s = hyphen_re.replace_all(&s, "$1$2").to_string();

    let trailing_hyphen_re = Regex::new(r"\b([A-Za-z]+)-\b").unwrap();
    s = trailing_hyphen_re.replace_all(&s, "$1").to_string();

    let honorific_u_re = Regex::new(r"(?i)\b([A-Za-z]+)u(san|sama|kun|chan|senpai|sensei|dono)\b").unwrap();
    s = honorific_u_re.replace_all(&s, "$1-$2").to_string();

    let hariju_honor_re = Regex::new(r"(?i)\bhariju[-~]?(san|sama|kun|chan)\b").unwrap();
    s = hariju_honor_re.replace_all(&s, "Harij-$1").to_string();

    let hariju_standalone_re = Regex::new(r"(?i)\bhariju\b").unwrap();
    s = hariju_standalone_re.replace_all(&s, "Harij").to_string();

    let honorific_dash_re = Regex::new(r"(?i)\b([A-Za-z]+)\s*[-~—]\s*(san|sama|kun|chan|senpai|sensei|dono)\b").unwrap();
    s = honorific_dash_re.replace_all(&s, "$1-$2").to_string();

    s = split_glued_words(&s);

    let idiom_replacements = [
        (Regex::new(r"(?i)\balovepotion\b").unwrap(), "a love potion"),
        (Regex::new(r"(?i)\bamawitch\b").unwrap(), "am a witch"),
        (Regex::new(r"(?i)\bafoolof\b").unwrap(), "a fool of"),
        (Regex::new(r"(?i)\bican'teven\b|\bicanteven\b").unwrap(), "I can't even"),
        (Regex::new(r"(?i)\bitwasall\b").unwrap(), "it was all"),
        (Regex::new(r"(?i)\banillusion\b").unwrap(), "an illusion"),
        (Regex::new(r"(?i)\bi'mnottoo\b|\bimnottoo\b").unwrap(), "I'm not too"),
        (Regex::new(r"(?i)\biwasn't\b|\biwasnt\b").unwrap(), "I wasn't"),
        (Regex::new(r"(?i)\bawareof\b").unwrap(), "aware of"),
        (Regex::new(r"(?i)\biknewfrom\b").unwrap(), "I knew from"),
        (Regex::new(r"(?i)\bandannoying\b").unwrap(), "and annoying"),
        (Regex::new(r"(?i)\boftime\b").unwrap(), "of time"),
        (Regex::new(r"(?i)\bitimproves\b").unwrap(), "it improves"),
        (Regex::new(r"(?i)\bifsirharij\b").unwrap(), "if Sir Harij"),
        (Regex::new(r"(?i)\bistillvividly\b").unwrap(), "I still vividly"),
        (Regex::new(r"(?i)\bthatoccurred\b").unwrap(), "that occurred"),
        (Regex::new(r"(?i)\bi'deversee\b|\bideversee\b").unwrap(), "I'd ever see"),
        (Regex::new(r"(?i)\bifyouaskme\b").unwrap(), "if you ask me"),
        (Regex::new(r"(?i)\bbutjust\b").unwrap(), "but just"),
        (Regex::new(r"(?i)\bididn't\b|\bididnt\b").unwrap(), "I didn't"),
        (Regex::new(r"(?i)\bidon't\b|\bidont\b").unwrap(), "I don't"),
        (Regex::new(r"(?i)\bi'dbeen\b|\bidbeen\b").unwrap(), "I'd been"),
        (Regex::new(r"(?i)\bifididn't\b|\bifididnt\b").unwrap(), "if I didn't"),
        (Regex::new(r"(?i)\bifidon't\b").unwrap(), "if I don't"),
    ];

    for (re, rep) in idiom_replacements.iter() {
        s = re.replace_all(&s, *rep).to_string();
    }

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
    let harij_re = Regex::new(r"(?i)\bsir harij\b").unwrap();
    formatted = harij_re.replace_all(&formatted, "Sir Harij").to_string();
    let rose_re = Regex::new(r"(?i)\brose\b").unwrap();
    formatted = rose_re.replace_all(&formatted, "Rose").to_string();

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
    fn test_splits_concatenated_comic_words() {
        let raw = "HEHAS NO REASON TOCOME OUTHERE ANYMORE.";
        assert_eq!(normalize_source_text(raw), "He has no reason to come out here anymore.");
    }
}

