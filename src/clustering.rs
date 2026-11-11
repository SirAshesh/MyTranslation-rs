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

    if clean.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }

    if clean.chars().all(|c| c.is_ascii_punctuation() || c.is_whitespace()) {
        return true;
    }

    if clean.len() == 1 && score < 0.85 && clean != "I" && clean != "A" && clean != "!" && clean != "?" {
        return true;
    }

    if score < 0.58 {
        return true;
    }

    if h > w * 1.3 && clean.len() > 1 && score < 0.88 {
        return true;
    }

    let mixed_digit_re = Regex::new(r"\d+[A-Za-z]|[A-Za-z]+\d+").unwrap();
    if mixed_digit_re.is_match(clean) && score < 0.85 {
        return true;
    }

    let paren_re = Regex::new(r"\([A-Za-z\s\-]{2,12}\)|\[[A-Za-z\s\-]{2,12}\]").unwrap();
    let words: Vec<&str> = clean
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| c.is_ascii_punctuation()))
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return true;
    }

    if paren_re.is_match(clean) && words.len() <= 5 {
        return true;
    }

    let known_romaji_sfx: HashSet<&'static str> = [
        "POKAAAN", "POKAN", "BIKU", "PIKU", "GIKU", "PEKO", "ZUKI", "KIRI", "CKIRI",
        "ZU", "ZUZU", "SHFD", "DOKI", "PATA", "NIKORI", "DODODO", "GOGOGO",
        "FWIP", "WHOOSH", "THUD", "GULP", "WHIRL", "TWITCH", "GAPE", "CHUUSHD",
        "SHIIN", "CHIRA", "GYU", "DOSA", "PASA", "WINCE", "BA", "BAA", "BASH",
        "GLL", "PAT", "COLTD", "CTWITCHD", "CCLENCHD"
    ].into_iter().collect();

    let known_junk_words: HashSet<&'static str> = [
        "ACR", "RS", "FW", "FLL", "FLLS", "IP", "FOI"
    ].into_iter().collect();

    if words.len() <= 3 {
        let mut all_sfx = true;
        for w in &words {
            let uw = w.to_uppercase();
            if known_romaji_sfx.contains(uw.as_str()) || known_junk_words.contains(uw.as_str()) {
                continue;
            }
            if uw.contains("TWITCH") || uw.contains("CLENCH") {
                continue;
            }
            if has_repeated_letters(&uw) {
                continue;
            }
            all_sfx = false;
            break;
        }
        if all_sfx {
            return true;
        }
    }

    if words.len() == 1 {
        let uw = words[0].to_uppercase();
        if known_junk_words.contains(uw.as_str())
            || known_romaji_sfx.contains(uw.as_str())
            || uw.contains("TWITCH")
            || uw.contains("CLENCH")
        {
            return true;
        }
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

    let glued_replacements = [
        (Regex::new(r"(?i)\bSOTHIS\b").unwrap(), "SO THIS"),
        (Regex::new(r"(?i)\bTHESWEET\b").unwrap(), "THE SWEET"),
        (Regex::new(r"(?i)\bTHATIS\b").unwrap(), "THAT IS"),
        (Regex::new(r"(?i)\bAFTERWHAT\b").unwrap(), "AFTER WHAT"),
        (Regex::new(r"(?i)\bASKME\b").unwrap(), "ASK ME"),
        (Regex::new(r"(?i)\bLIKEAND\b").unwrap(), "LIKE AND"),
        (Regex::new(r"(?i)\bTOBRING\b").unwrap(), "TO BRING"),
        (Regex::new(r"(?i)\bSEEME\b").unwrap(), "SEE ME"),
        (Regex::new(r"(?i)\bMYOWN\b").unwrap(), "MY OWN"),
        (Regex::new(r"(?i)\bWHATA\b").unwrap(), "WHAT A"),
        (Regex::new(r"(?i)\bLIKETO\b").unwrap(), "LIKE TO"),
        (Regex::new(r"(?i)\bTOMAKE\b").unwrap(), "TO MAKE"),
        (Regex::new(r"(?i)\bTHENIT\b").unwrap(), "THEN IT"),
        (Regex::new(r"(?i)\bTHENI\b").unwrap(), "THEN I"),
        (Regex::new(r"(?i)\bBUTIFI\b").unwrap(), "BUT IF I"),
        (Regex::new(r"(?i)\bBUTIF\b").unwrap(), "BUT IF"),
        (Regex::new(r"(?i)\bBUTI\b").unwrap(), "BUT I"),
        (Regex::new(r"(?i)\bANDNOWI\b").unwrap(), "AND NOW I"),
        (Regex::new(r"(?i)\bNOWI\b").unwrap(), "NOW I"),
        (Regex::new(r"(?i)\bWHYIS\b").unwrap(), "WHY IS"),
        (Regex::new(r"(?i)\bDOYOU\b").unwrap(), "DO YOU"),
        (Regex::new(r"(?i)\bTRYTO\b").unwrap(), "TRY TO"),
        (Regex::new(r"(?i)\bINEVER\b").unwrap(), "I NEVER"),
        (Regex::new(r"(?i)\bIMEAN\b").unwrap(), "I MEAN"),
        (Regex::new(r"(?i)\bIOVER\b").unwrap(), "I OVER"),
        (Regex::new(r"(?i)\bIDIDN'T\b").unwrap(), "I DIDN'T"),
        (Regex::new(r"(?i)\bIDON'T\b").unwrap(), "I DON'T"),
        (Regex::new(r"(?i)\bIWAS\b").unwrap(), "I WAS"),
        (Regex::new(r"(?i)\bIONLY\b").unwrap(), "I ONLY"),
        (Regex::new(r"(?i)\bPOTIONI\b").unwrap(), "POTION I"),
        (Regex::new(r"(?i)\bITWAS\b").unwrap(), "IT WAS"),
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
