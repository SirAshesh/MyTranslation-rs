use crate::model::{Bubble, LineInfo, OcrLineItem};
use crate::pipe::text_cleaner::{
    contains_cjk, is_junk_or_sfx, join_manga_lines, normalize_source_text,
};

struct Cluster {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    cx_sum: f32,
    cy_sum: f32,
    w_sum: f32,
    h_sum: f32,
    lines: Vec<LineInfo>,
}

pub fn cluster_lines(
    ocr_lines: Vec<OcrLineItem>,
    img_width: u32,
    img_height: u32,
    source_lang: &str,
) -> Vec<Bubble> {
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

    if lines.is_empty() {
        return Vec::new();
    }

    let is_lang_cjk = source_lang.starts_with("ja") || source_lang.starts_with("zh");
    let vertical_line_count = lines.iter().filter(|l| l.h > l.w * 1.15).count();
    let mostly_vertical = (vertical_line_count as f32 / lines.len() as f32) > 0.40;

    if mostly_vertical || is_lang_cjk {
        lines.sort_by(|a, b| {
            b.cx.partial_cmp(&a.cx)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cy.partial_cmp(&b.cy).unwrap_or(std::cmp::Ordering::Equal))
        });
    } else {
        lines.sort_by(|a, b| {
            a.cy.partial_cmp(&b.cy)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cx.partial_cmp(&b.cx).unwrap_or(std::cmp::Ordering::Equal))
        });
    }

    let mut clusters: Vec<Cluster> = Vec::new();

    for l in lines {
        let line_is_vertical = l.h > l.w * 1.15 || (contains_cjk(&l.text) && l.h >= l.w);
        let mut matched = false;

        for c in clusters.iter_mut() {
            let count = c.lines.len() as f32;
            let avg_cx = c.cx_sum / count;
            let avg_cy = c.cy_sum / count;
            let avg_w = (c.w_sum / count + l.w) / 2.0;
            let avg_h = (c.h_sum / count + l.h) / 2.0;

            let cluster_w = c.x2 - c.x1;
            let cluster_h = c.y2 - c.y1;

            let dy = (l.y1 - c.y2).max(c.y1 - l.y2).max(0.0);
            let dcx = (l.cx - avg_cx).abs();
            let overlap_x = c.x2.min(l.x2) - c.x1.max(l.x1);
            let horizontal_match = dy <= avg_h * 1.55 && (dcx <= 40.0 || overlap_x > 0.35 * cluster_w.min(l.w));

            let dx = (l.x1 - c.x2).max(c.x1 - l.x2).max(0.0);
            let dcy = (l.cy - avg_cy).abs();
            let overlap_y = c.y2.min(l.y2) - c.y1.max(l.y1);
            let vertical_match = (line_is_vertical || is_lang_cjk)
                && dx <= avg_w * 2.2
                && (dcy <= avg_h * 0.75 || overlap_y > 0.30 * cluster_h.min(l.h));

            if horizontal_match || vertical_match {
                c.x1 = c.x1.min(l.x1);
                c.y1 = c.y1.min(l.y1);
                c.x2 = c.x2.max(l.x2);
                c.y2 = c.y2.max(l.y2);
                c.cx_sum += l.cx;
                c.cy_sum += l.cy;
                c.w_sum += l.w;
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
                cx_sum: l.cx,
                cy_sum: l.cy,
                w_sum: l.w,
                h_sum: l.h,
                lines: vec![l],
            });
        }
    }

    let mut bubbles = Vec::new();
    let mut bubble_id = 1;

    for mut c in clusters {
        let is_vertical_bubble = c.lines.iter().filter(|l| l.h > l.w * 1.15).count() * 2 >= c.lines.len()
            || (c.lines.iter().any(|l| contains_cjk(&l.text)) && (c.y2 - c.y1) > (c.x2 - c.x1) * 1.2);

        if is_vertical_bubble {
            c.lines.sort_by(|a, b| {
                let dcx = (b.cx - a.cx).abs();
                let col_w = a.w.max(b.w);
                if dcx > col_w * 0.45 {
                    b.cx.partial_cmp(&a.cx).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        } else {
            c.lines.sort_by(|a, b| {
                let dcy = (b.cy - a.cy).abs();
                let row_h = a.h.max(b.h);
                if dcy > row_h * 0.45 {
                    a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    a.x1.partial_cmp(&b.x1).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        }

        let raw_texts: Vec<&str> = c.lines.iter().map(|l| l.text.as_str()).collect();
        let combined = join_manga_lines(&raw_texts);
        let normalized = normalize_source_text(&combined);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_horizontal_line_clustering() {
        let lines = vec![
            OcrLineItem {
                text: "HELLO".to_string(),
                score: 0.95,
                r#box: [[10.0, 10.0], [50.0, 10.0], [50.0, 25.0], [10.0, 25.0]],
            },
            OcrLineItem {
                text: "WORLD".to_string(),
                score: 0.95,
                r#box: [[10.0, 30.0], [50.0, 30.0], [50.0, 45.0], [10.0, 45.0]],
            },
        ];

        let bubbles = cluster_lines(lines, 200, 200, "en");
        assert_eq!(bubbles.len(), 1);
        assert_eq!(bubbles[0].source_text, "Hello world");
    }

    #[test]
    fn test_vertical_japanese_manga_clustering() {
        let col_right = OcrLineItem {
            text: "何でそんなこと".to_string(),
            score: 0.92,
            r#box: [[150.0, 20.0], [170.0, 20.0], [170.0, 140.0], [150.0, 140.0]],
        };
        let col_left = OcrLineItem {
            text: "言うんだよ".to_string(),
            score: 0.90,
            r#box: [[120.0, 20.0], [140.0, 20.0], [140.0, 130.0], [120.0, 130.0]],
        };

        let bubbles = cluster_lines(vec![col_right, col_left], 300, 300, "ja");
        assert_eq!(bubbles.len(), 1);
        assert_eq!(bubbles[0].source_text, "何でそんなこと言うんだよ");
    }
}
