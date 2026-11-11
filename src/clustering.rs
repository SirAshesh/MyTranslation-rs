use crate::models::{Bubble, LineInfo, OcrLineItem};
use regex::Regex;

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

        let is_pure_digits = text.chars().all(|c| c.is_ascii_digit());
        let xs: Vec<f32> = item.r#box.iter().map(|p| p[0]).collect();
        let ys: Vec<f32> = item.r#box.iter().map(|p| p[1]).collect();

        let min_x = xs.iter().cloned().fold(f32::INFINITY, f32::min).max(0.0);
        let max_x = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max).min(img_width as f32);
        let min_y = ys.iter().cloned().fold(f32::INFINITY, f32::min).max(0.0);
        let max_y = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max).min(img_height as f32);

        if is_pure_digits && (min_y > (img_height as f32 - 70.0) || max_y < 50.0) {
            continue;
        }

        if text.len() <= 2 && is_pure_digits {
            continue;
        }

        if text.len() == 1 && item.score < 0.85 && text != "I" && text != "A" && text != "!" && text != "?" {
            continue;
        }

        if item.score < 0.38 {
            continue;
        }

        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);

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
    for (idx, mut c) in clusters.into_iter().enumerate() {
        c.lines.sort_by(|a, b| a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal));

        let raw_texts: Vec<&str> = c.lines.iter().map(|l| l.text.as_str()).collect();
        let combined = raw_texts.join(" ");
        let cleaned_text = space_re.replace_all(&combined, " ").trim().to_string();

        bubbles.push(Bubble {
            id: idx + 1,
            x1: c.x1,
            y1: c.y1,
            x2: c.x2,
            y2: c.y2,
            w: (c.x2 - c.x1).max(1.0),
            h: (c.y2 - c.y1).max(1.0),
            source_text: cleaned_text,
            lines: c.lines,
        });
    }

    bubbles
}
