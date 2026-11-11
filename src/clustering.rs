use crate::models::{Bubble, LineInfo, OcrLineItem};
use regex::Regex;
use std::collections::HashSet;

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

    if clean.chars().all(|c| c.is_ascii_digit()) || stripped.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    let mixed_re = Regex::new(r"^\d+[A-Za-z]+$|^[A-Za-z]+\d+$").unwrap();
    if mixed_re.is_match(&stripped) {
        return true;
    }

    let valid_short: HashSet<&'static str> = [
        "NO", "OK", "GO", "HI", "OH", "AH", "ME", "MY", "HE", "WE", "SO",
        "DO", "IF", "IN", "ON", "AT", "TO", "UP", "BY", "IS", "AM", "US", "IT"
    ].into_iter().collect();

    if stripped.len() <= 2 {
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

    if non_sfx_words.is_empty() || non_sfx_words.iter().all(|w| w.len() <= 1) {
        return true;
    }

    if words.iter().any(|w| w == "ACR" || w == "RS" || w == "FLL" || w == "FLLS" || w == "FW" || w == "FOI") && words.len() <= 3 {
        return true;
    }

    if words.len() <= 2 && words.iter().all(|w| has_repeated_letters(w)) {
        return true;
    }

    if h > w * 1.35 && clean.len() > 1 && score < 0.88 {
        return true;
    }

    false
}

pub fn normalize_source_text(text: &str) -> String {
    let mut s = text.trim().to_string();
    if s.is_empty() {
        return s;
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

    let glued_replacements = [
        (Regex::new(r"(?i)\banda\b").unwrap(), "and a"),
        (Regex::new(r"(?i)\bfora\b").unwrap(), "for a"),
        (Regex::new(r"(?i)\bwitha\b").unwrap(), "with a"),
        (Regex::new(r"(?i)\bwhata\b").unwrap(), "what a"),
        (Regex::new(r"(?i)\balove\b").unwrap(), "a love"),
        (Regex::new(r"(?i)\baperson\b").unwrap(), "a person"),
        (Regex::new(r"(?i)\bataperson\b").unwrap(), "at a person"),
        (Regex::new(r"(?i)\bina\b").unwrap(), "in a"),
        (Regex::new(r"(?i)\bofa\b").unwrap(), "of a"),
        (Regex::new(r"(?i)\basina\b").unwrap(), "as in a"),
        (Regex::new(r"(?i)\blikea\b").unwrap(), "like a"),
        (Regex::new(r"(?i)\bhavea\b").unwrap(), "have a"),
        (Regex::new(r"(?i)\bsucha\b").unwrap(), "such a"),
        (Regex::new(r"(?i)\bonlya\b").unwrap(), "only a"),
        (Regex::new(r"(?i)\bimiss\b").unwrap(), "I miss"),
        (Regex::new(r"(?i)\biwant\b").unwrap(), "I want"),
        (Regex::new(r"(?i)\bineed\b").unwrap(), "I need"),
        (Regex::new(r"(?i)\biknow\b").unwrap(), "I know"),
        (Regex::new(r"(?i)\bithink\b").unwrap(), "I think"),
        (Regex::new(r"(?i)\bihave\b").unwrap(), "I have"),
        (Regex::new(r"(?i)\bihad\b").unwrap(), "I had"),
        (Regex::new(r"(?i)\bihope\b").unwrap(), "I hope"),
        (Regex::new(r"(?i)\biwish\b").unwrap(), "I wish"),
        (Regex::new(r"(?i)\bilove\b").unwrap(), "I love"),
        (Regex::new(r"(?i)\bifeltlike\b").unwrap(), "I felt like"),
        (Regex::new(r"(?i)\bifelt\b").unwrap(), "I felt"),
        (Regex::new(r"(?i)\bisaid\b").unwrap(), "I said"),
        (Regex::new(r"(?i)\biwas\b").unwrap(), "I was"),
        (Regex::new(r"(?i)\biwent\b").unwrap(), "I went"),
        (Regex::new(r"(?i)\biam\b").unwrap(), "I am"),
        (Regex::new(r"(?i)\bican\b").unwrap(), "I can"),
        (Regex::new(r"(?i)\bicould\b").unwrap(), "I could"),
        (Regex::new(r"(?i)\biwould\b").unwrap(), "I would"),
        (Regex::new(r"(?i)\biassumed\b").unwrap(), "I assumed"),
        (Regex::new(r"(?i)\bionly\b").unwrap(), "I only"),
        (Regex::new(r"(?i)\binever\b").unwrap(), "I never"),
        (Regex::new(r"(?i)\bimean\b").unwrap(), "I mean"),
        (Regex::new(r"(?i)\biover\b").unwrap(), "I over"),
        (Regex::new(r"(?i)\biasked\b").unwrap(), "I asked"),
        (Regex::new(r"(?i)\bicall\b").unwrap(), "I call"),
        (Regex::new(r"(?i)\bitold\b").unwrap(), "I told"),
        (Regex::new(r"(?i)\bijust\b").unwrap(), "I just"),
        (Regex::new(r"(?i)\bididn't\b|\bididnt\b").unwrap(), "I didn't"),
        (Regex::new(r"(?i)\bidon't\b|\bidont\b").unwrap(), "I don't"),
        (Regex::new(r"(?i)\bi'dbeen\b|\bidbeen\b").unwrap(), "I'd been"),
        (Regex::new(r"(?i)\bi'dbeenso\b|\bidbeenso\b").unwrap(), "I'd been so"),
        (Regex::new(r"(?i)\bifididn't\b|\bifididnt\b").unwrap(), "if I didn't"),
        (Regex::new(r"(?i)\bifidid\b").unwrap(), "if I did"),
        (Regex::new(r"(?i)\bifidon't\b").unwrap(), "if I don't"),
        (Regex::new(r"(?i)\bsoi\b").unwrap(), "so I"),
        (Regex::new(r"(?i)\bthati\b").unwrap(), "that I"),
        (Regex::new(r"(?i)\bthingi\b").unwrap(), "thing I"),
        (Regex::new(r"(?i)\btheni\b").unwrap(), "then I"),
        (Regex::new(r"(?i)\bbuti\b").unwrap(), "but I"),
        (Regex::new(r"(?i)\bnowi\b").unwrap(), "now I"),
        (Regex::new(r"(?i)\bthefirst\b").unwrap(), "the first"),
        (Regex::new(r"(?i)\bthesweet\b").unwrap(), "the sweet"),
        (Regex::new(r"(?i)\bthatis\b").unwrap(), "that is"),
        (Regex::new(r"(?i)\bafterwhat\b").unwrap(), "after what"),
        (Regex::new(r"(?i)\baskme\b").unwrap(), "ask me"),
        (Regex::new(r"(?i)\blikeand\b").unwrap(), "like and"),
        (Regex::new(r"(?i)\btobring\b").unwrap(), "to bring"),
        (Regex::new(r"(?i)\bseeme\b").unwrap(), "see me"),
        (Regex::new(r"(?i)\bmyown\b").unwrap(), "my own"),
        (Regex::new(r"(?i)\bliketo\b").unwrap(), "like to"),
        (Regex::new(r"(?i)\btomake\b").unwrap(), "to make"),
        (Regex::new(r"(?i)\bthenit\b").unwrap(), "then it"),
        (Regex::new(r"(?i)\bbutifi\b").unwrap(), "but if I"),
        (Regex::new(r"(?i)\bbutif\b").unwrap(), "but if"),
        (Regex::new(r"(?i)\bandnowi\b").unwrap(), "and now I"),
        (Regex::new(r"(?i)\bwhyis\b").unwrap(), "why is"),
        (Regex::new(r"(?i)\bdoyou\b").unwrap(), "do you"),
        (Regex::new(r"(?i)\btryto\b").unwrap(), "try to"),
        (Regex::new(r"(?i)\bpotionthat\b").unwrap(), "potion that"),
        (Regex::new(r"(?i)\bpotionis\b").unwrap(), "potion is"),
        (Regex::new(r"(?i)\bpotioni\b").unwrap(), "potion I"),
        (Regex::new(r"(?i)\banotherlove\b").unwrap(), "another love"),
        (Regex::new(r"(?i)\bitwas\b").unwrap(), "it was"),
        (Regex::new(r"(?i)\bsothis\b").unwrap(), "so this"),
    ];

    for (re, rep) in glued_replacements.iter() {
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

pub fn cluster_lines(ocr_lines: Vec<OcrLineItem>, img_width: u32, img_height: u32) -> Vec<Bubble> {
    if ocr_lines.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for item in ocr_lines {
        let text = item.text.trim().to_string();
        if text.is_empty() {
            continue;
        }

        let xs: Vec<f32> = item.r#box.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = item.r#box.iter().map(|p| p[1]).collect();

        let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min).max(0.0);
        let max_x = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max).min(img_width as f32);
        let min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min).max(0.0);
        let max_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max).min(img_height as f32);

        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);

        if is_junk_or_sfx(&text, item.score, w, h) {
            continue;
        }

        lines.push(LineInfo {
            x1: min_x,
            y1: min_y,
            x2: max_x,
            y2: max_y,
            cx: (min_x + max_x) / 2.0,
            cy: (min_y + max_y) / 2.0,
            w,
            h,
            text,
            score: item.score,
        });
    }

    lines.sort_by(|a, b| {
        a.cy.partial_cmp(&b.cy)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cx.partial_cmp(&b.cx).unwrap_or(std::cmp::Ordering::Equal))
    });

    struct Cluster {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        w: f32,
        cx_sum: f32,
        h_sum: f32,
        lines: Vec<LineInfo>,
    }

    let mut clusters: Vec<Cluster> = Vec::new();

    for l in lines {
        let mut matched = false;
        for c in clusters.iter_mut() {
            let count = c.lines.len() as f32;
            let avg_cx = c.cx_sum / count;
            let avg_h = (c.h_sum / count + l.h) / 2.0;

            let dy = (l.y1 - c.y2).max(0.0);
            let dcx = (l.cx - avg_cx).abs();
            let overlap_x = c.x2.min(l.x2) - c.x1.max(l.x1);

            if dy <= avg_h * 1.55 && (dcx <= 38.0 || overlap_x > 0.45 * c.w.min(l.w)) {
                c.x1 = c.x1.min(l.x1);
                c.y1 = c.y1.min(l.y1);
                c.x2 = c.x2.max(l.x2);
                c.y2 = c.y2.max(l.y2);
                c.w = c.x2 - c.x1;
                c.cx_sum += l.cx;
                c.h_sum += l.h;
                c.lines.push(l.clone());
                matched = true;
                break;
            }
        }

        if !matched {
            clusters.push(Cluster {
                x1: l.x1,
                y1: l.y1,
                x2: l.x2,
                y2: l.y2,
                w: l.w,
                cx_sum: l.cx,
                h_sum: l.h,
                lines: vec![l],
            });
        }
    }

    let space_re = Regex::new(r"\s+").unwrap();

    let mut bubbles = Vec::new();
    let mut bubble_id = 1;
    for mut c in clusters {
        c.lines.sort_by(|a, b| a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal));

        let raw_texts: Vec<&str> = c.lines.iter().map(|l| l.text.as_str()).collect();
        let combined = raw_texts.join(" ");
        let cleaned_text = space_re.replace_all(&combined, " ").trim().to_string();
        let normalized = normalize_source_text(&cleaned_text);

        let bubble_w = (c.x2 - c.x1).max(1.0);
        let bubble_h = (c.y2 - c.y1).max(1.0);
        let avg_score = c.lines.iter().map(|l| l.score).sum::<f32>() / c.lines.len() as f32;

        if is_junk_or_sfx(&normalized, avg_score, bubble_w, bubble_h) {
            continue;
        }

        bubbles.push(Bubble {
            id: bubble_id,
            x1: c.x1,
            y1: c.y1,
            x2: c.x2,
            y2: c.y2,
            w: bubble_w,
            h: bubble_h,
            source_text: normalized,
            lines: c.lines,
        });
        bubble_id += 1;
    }

    bubbles
}
