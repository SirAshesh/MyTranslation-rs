mod cjk;
mod geometry;
#[cfg(test)]
mod tests;

use self::cjk::{
    absorb_furigana, build_cjk_interior_cleanup_line, build_vertical_bridge_lines,
    can_cluster_as_vertical_cjk, is_kana_char, is_short_cjk_chunk, is_vertical_bubble_lines,
    is_vertical_cjk_line, sort_vertical_columns_rtl, SYNTHETIC_INTERIOR_CLEANUP_TAG,
};

pub fn is_interior_cleanup_line(line: &LineInfo) -> bool {
    line.text == SYNTHETIC_INTERIOR_CLEANUP_TAG
}
use self::geometry::{
    compute_bubble_oriented_box, compute_oriented_metrics, normalize_quad, project_quad_on_angle,
    MIN_TILT_RAD,
};
use crate::model::{Bubble, LineInfo, OcrLineItem};
use crate::pipe::text_cleaner::{
    contains_cjk, is_junk_or_sfx, join_manga_lines, normalize_source_text,
};

struct Cluster {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    dialogue_lines: Vec<LineInfo>,
    furigana_lines: Vec<LineInfo>,
}

fn compute_cluster_limits(img_width: u32, img_height: u32) -> (f32, f32) {
    let is_tall_strip = img_height > img_width;
    let is_ultra_tall_strip = img_height > img_width.saturating_mul(2);
    let max_cluster_w = if is_tall_strip {
        (img_width as f32 * 0.85).max(260.0)
    } else {
        (img_width as f32 * 0.55).max(240.0)
    };
    let max_cluster_h = if is_ultra_tall_strip {
        (img_height as f32 * 0.45)
            .min((img_width as f32 * 1.25).max(360.0))
            .max(240.0)
    } else if is_tall_strip {
        (img_height as f32 * 0.45).max(240.0)
    } else {
        (img_height as f32 * 0.35).max(220.0)
    };
    (max_cluster_w, max_cluster_h)
}

fn classify_dominant_lang<'a, 'b>(
    items: impl Iterator<Item = (f32, &'a str)>,
    requested_lang: &'b str,
) -> &'b str {
    let mut kana_count = 0usize;
    let mut han_count = 0usize;
    let mut latin_count = 0usize;

    for (score, text) in items {
        if score < 0.45 {
            continue;
        }
        for ch in text.chars() {
            if matches!(ch, '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}') {
                kana_count += 1;
            } else if matches!(ch, '\u{4E00}'..='\u{9FFF}') {
                han_count += 1;
            } else if ch.is_ascii_alphabetic() {
                latin_count += 1;
            }
        }
    }

    let cjk_count = kana_count + han_count;
    if kana_count >= 1 && (cjk_count >= latin_count || (latin_count < 16 && kana_count >= 2)) {
        "ja"
    } else if han_count >= 2 && han_count * 2 > latin_count {
        "zh"
    } else if requested_lang.is_empty() || requested_lang.eq_ignore_ascii_case("auto") {
        "en"
    } else {
        requested_lang
    }
}

pub fn detect_dominant_source_lang<'a>(
    lines: &[OcrLineItem],
    requested_lang: &'a str,
) -> &'a str {
    classify_dominant_lang(lines.iter().map(|l| (l.score, l.text.as_str())), requested_lang)
}

fn should_suppress_cjk_hallucinations(lines: &[LineInfo], source_lang: &str) -> bool {
    let effective_lang =
        classify_dominant_lang(lines.iter().map(|l| (l.score, l.text.as_str())), source_lang);
    let lang_lower = effective_lang.to_ascii_lowercase();
    if lang_lower.starts_with("ja") || lang_lower.starts_with("zh") {
        return false;
    }

    let mut latin_dialogue_lines = 0usize;
    let mut latin_any_lines = 0usize;
    let mut cjk_lines = 0usize;
    for l in lines {
        if contains_cjk(&l.text) {
            cjk_lines += 1;
            continue;
        }
        if l.score < 0.75 {
            continue;
        }
        let latin_words = l
            .text
            .split_whitespace()
            .filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
            .count();
        let ascii_alpha = l.text.chars().filter(|c| c.is_ascii_alphabetic()).count();
        if latin_words >= 2 || ascii_alpha >= 5 {
            latin_dialogue_lines += 1;
        }
        if ascii_alpha >= 2 {
            latin_any_lines += 1;
        }
    }

    latin_dialogue_lines >= 2 || (latin_any_lines >= 4 && latin_any_lines > cjk_lines * 2)
}

pub(crate) fn line_from_ocr_item(
    item: &OcrLineItem,
    img_width: u32,
    img_height: u32,
) -> Option<LineInfo> {
    let text = item.text.trim().to_string();
    if text.is_empty() {
        return None;
    }

    let quad = normalize_quad(item.r#box, img_width, img_height);
    let min_x = quad.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = quad.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = quad.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = quad.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);

    let w = (max_x - min_x).max(1.0);
    let h = (max_y - min_y).max(1.0);
    let (true_w, true_h, angle_rad) = compute_oriented_metrics(&quad);

    Some(LineInfo {
        quad,
        x1: min_x,
        y1: min_y,
        x2: max_x,
        y2: max_y,
        cx: (min_x + max_x) / 2.0,
        cy: (min_y + max_y) / 2.0,
        w,
        h,
        true_w,
        true_h,
        angle_rad,
        text,
        score: item.score,
    })
}

pub(crate) fn rebuild_bubble_from_lines(
    id: usize,
    mut dialogue_lines: Vec<LineInfo>,
    furigana_lines: Vec<LineInfo>,
    img_width: u32,
    img_height: u32,
    source_lang: &str,
    enforce_junk_filter: bool,
) -> Option<Bubble> {
    if dialogue_lines.is_empty() {
        return None;
    }

    let (cx1, cy1, cx2, cy2) = dialogue_lines.iter().fold(
        (
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ),
        |(ax1, ay1, ax2, ay2), line| {
            (
                ax1.min(line.x1),
                ay1.min(line.y1),
                ax2.max(line.x2),
                ay2.max(line.y2),
            )
        },
    );
    let bubble_w = (cx2 - cx1).max(1.0);
    let bubble_h = (cy2 - cy1).max(1.0);
    let is_vertical_bubble = is_vertical_bubble_lines(&dialogue_lines, bubble_w, bubble_h);

    let bubble_angle = if is_vertical_bubble {
        0.0
    } else {
        let mut cos_sum = 0.0f32;
        let mut sin_sum = 0.0f32;
        for line in &dialogue_lines {
            let weight = (line.true_w * line.true_h).max(1.0);
            cos_sum += weight * line.angle_rad.cos();
            sin_sum += weight * line.angle_rad.sin();
        }
        let mean_angle = sin_sum.atan2(cos_sum);
        if mean_angle.abs() < MIN_TILT_RAD {
            0.0
        } else {
            mean_angle
        }
    };

    if is_vertical_bubble {
        sort_vertical_columns_rtl(&mut dialogue_lines);
    } else {
        dialogue_lines.sort_by(|a, b| {
            let (_, _, av_min, av_max) = project_quad_on_angle(&a.quad, bubble_angle);
            let (_, _, bv_min, bv_max) = project_quad_on_angle(&b.quad, bubble_angle);
            let (au_min, au_max, _, _) = project_quad_on_angle(&a.quad, bubble_angle);
            let (bu_min, bu_max, _, _) = project_quad_on_angle(&b.quad, bubble_angle);
            let av_mid = (av_min + av_max) * 0.5;
            let bv_mid = (bv_min + bv_max) * 0.5;
            let dcv = (bv_mid - av_mid).abs();
            let row_h = a.true_h.max(b.true_h);
            if dcv > row_h * 0.45 {
                av_mid
                    .partial_cmp(&bv_mid)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                let au_mid = (au_min + au_max) * 0.5;
                let bu_mid = (bu_min + bu_max) * 0.5;
                au_mid
                    .partial_cmp(&bu_mid)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        });
    }

    let raw_texts: Vec<&str> = dialogue_lines.iter().map(|l| l.text.as_str()).collect();
    let combined = join_manga_lines(&raw_texts);
    let normalized = normalize_source_text(&combined);
    let avg_score =
        dialogue_lines.iter().map(|l| l.score).sum::<f32>() / dialogue_lines.len().max(1) as f32;

    if enforce_junk_filter && is_junk_or_sfx(&normalized, avg_score, bubble_w, bubble_h) {
        return None;
    }

    let (cx, cy, oriented_w, oriented_h, is_left_aligned) =
        compute_bubble_oriented_box(&dialogue_lines, bubble_angle, is_vertical_bubble);

    let bridges = if is_vertical_bubble {
        build_vertical_bridge_lines(&dialogue_lines)
    } else {
        Vec::new()
    };
    let mut inpaint_lines = dialogue_lines;
    inpaint_lines.extend(furigana_lines);
    inpaint_lines.extend(bridges);

    let (bx1, by1, bx2, by2) = inpaint_lines.iter().fold(
        (cx1, cy1, cx2, cy2),
        |(ax1, ay1, ax2, ay2), line| {
            (
                ax1.min(line.x1),
                ay1.min(line.y1),
                ax2.max(line.x2),
                ay2.max(line.y2),
            )
        },
    );

    let lang_lower = source_lang.to_ascii_lowercase();
    let is_cjk_bubble =
        lang_lower.starts_with("ja") || lang_lower.starts_with("zh") || contains_cjk(&normalized);
    if is_cjk_bubble {
        let is_ja = lang_lower.starts_with("ja") || normalized.chars().any(is_kana_char);
        inpaint_lines.push(build_cjk_interior_cleanup_line(
            bx1,
            by1,
            bx2,
            by2,
            is_ja,
            is_vertical_bubble,
            img_width,
            img_height,
        ));
    }

    Some(Bubble {
        id,
        x1: bx1,
        y1: by1,
        x2: bx2,
        y2: by2,
        cx,
        cy,
        w: (bx2 - bx1).max(1.0),
        h: (by2 - by1).max(1.0),
        oriented_w,
        oriented_h,
        angle_rad: bubble_angle,
        is_left_aligned,
        source_text: normalized,
        lines: inpaint_lines,
    })
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

    let effective_source_lang = detect_dominant_source_lang(&ocr_lines, source_lang);

    let mut lines = Vec::new();
    for item in ocr_lines {
        let Some(line) = line_from_ocr_item(&item, img_width, img_height) else {
            continue;
        };

        if is_junk_or_sfx(&line.text, line.score, line.true_w, line.true_h) {
            continue;
        }

        lines.push(line);
    }

    if lines.is_empty() {
        return Vec::new();
    }

    if should_suppress_cjk_hallucinations(&lines, effective_source_lang) {
        lines.retain(|l| !contains_cjk(&l.text));
    }

    if lines.is_empty() {
        return Vec::new();
    }

    let lang_lower = effective_source_lang.to_ascii_lowercase();
    let is_lang_cjk = lang_lower.starts_with("ja") || lang_lower.starts_with("zh");
    let mut entries = if is_lang_cjk || lines.iter().any(|l| contains_cjk(&l.text)) {
        absorb_furigana(lines)
    } else {
        lines
            .into_iter()
            .map(|line| cjk::LineWithFurigana {
                line,
                furigana: Vec::new(),
            })
            .collect()
    };

    let vertical_line_count = entries
        .iter()
        .filter(|e| e.line.true_h > e.line.true_w * 1.15)
        .count();
    let mostly_vertical = (vertical_line_count as f32 / entries.len().max(1) as f32) > 0.40;

    if mostly_vertical || is_lang_cjk {
        entries.sort_by(|a, b| {
            b.line
                .cx
                .partial_cmp(&a.line.cx)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.line
                        .cy
                        .partial_cmp(&b.line.cy)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    } else {
        entries.sort_by(|a, b| {
            a.line
                .cy
                .partial_cmp(&b.line.cy)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.line
                        .cx
                        .partial_cmp(&b.line.cx)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
    }

    let (max_cluster_w, max_cluster_h) = compute_cluster_limits(img_width, img_height);
    let mut clusters: Vec<Cluster> = Vec::new();

    for item in entries {
        let l = item.line;
        let cand_is_vertical_cjk =
            is_vertical_cjk_line(&l) || (is_lang_cjk && is_short_cjk_chunk(&l));

        let mut best_idx = None;
        let mut best_score = f32::MAX;

        for (idx, c) in clusters.iter().enumerate() {
            let cand_x1 = c.x1.min(l.x1);
            let cand_y1 = c.y1.min(l.y1);
            let cand_x2 = c.x2.max(l.x2);
            let cand_y2 = c.y2.max(l.y2);

            let eff_max_h = if cand_is_vertical_cjk && img_height <= img_width.saturating_mul(2) {
                max_cluster_h.max(img_height as f32 * 0.65)
            } else {
                max_cluster_h
            };

            if (cand_x2 - cand_x1) > max_cluster_w || (cand_y2 - cand_y1) > eff_max_h {
                continue;
            }

            if cand_is_vertical_cjk {
                if let Some(score) =
                    can_cluster_as_vertical_cjk(&l, &c.dialogue_lines, is_lang_cjk)
                {
                    if score < best_score {
                        best_score = score;
                        best_idx = Some(idx);
                    }
                }
            } else {
                let last_line = match c.dialogue_lines.last() {
                    Some(line) => line,
                    None => continue,
                };
                let cluster_is_vert = c
                    .dialogue_lines
                    .iter()
                    .filter(|cl| cl.true_h >= cl.true_w * 1.25)
                    .count()
                    * 2
                    >= c.dialogue_lines.len();
                if cluster_is_vert {
                    continue;
                }

                let angle_diff = (l.angle_rad - last_line.angle_rad).abs();
                if angle_diff > 0.25 {
                    continue;
                }

                let ref_angle = if l.true_w > last_line.true_w {
                    l.angle_rad
                } else {
                    last_line.angle_rad
                };
                let (lu_min, lu_max, lv_min, lv_max) =
                    project_quad_on_angle(&last_line.quad, ref_angle);
                let (cu_min, cu_max, cv_min, cv_max) = project_quad_on_angle(&l.quad, ref_angle);

                let last_v_mid = (lv_min + lv_max) * 0.5;
                let mut row_u_min = lu_min;
                let mut row_u_max = lu_max;
                let mut row_v_min = lv_min;
                let mut row_v_max = lv_max;
                for cl in &c.dialogue_lines {
                    let (u1, u2, v1, v2) = project_quad_on_angle(&cl.quad, ref_angle);
                    let v_mid = (v1 + v2) * 0.5;
                    if (v_mid - last_v_mid).abs() <= last_line.true_h * 0.45 {
                        row_u_min = row_u_min.min(u1);
                        row_u_max = row_u_max.max(u2);
                        row_v_min = row_v_min.min(v1);
                        row_v_max = row_v_max.max(v2);
                    }
                }

                let row_w = (row_u_max - row_u_min).max(1.0);
                let dv = (cv_min - row_v_max).max(row_v_min - cv_max).max(0.0);
                let overlap_v = row_v_max.min(cv_max) - row_v_min.max(cv_min);
                let du_gap = (cu_min - row_u_max).max(row_u_min - cu_max).max(0.0);
                let du_center = ((cu_min + cu_max) * 0.5 - (row_u_min + row_u_max) * 0.5).abs();
                let du_left = (cu_min - row_u_min).abs();
                let overlap_u = row_u_max.min(cu_max) - row_u_min.max(cu_min);
                let min_true_w = l.true_w.min(row_w);

                let last_ends_sentence =
                    last_line.text.ends_with("...") || last_line.text.ends_with(". ");
                let max_gap = if last_ends_sentence {
                    last_line.true_h * 0.35
                } else {
                    last_line.true_h * 0.65
                };

                let is_adjacent_line = dv <= max_gap;
                let height_ratio =
                    l.true_h.max(last_line.true_h) / l.true_h.min(last_line.true_h).max(1.0);
                let is_same_row_fragment = overlap_v >= l.true_h.min(last_line.true_h) * 0.45
                    && du_gap <= l.true_h.min(last_line.true_h) * 0.65
                    && height_ratio <= 1.35;
                let is_centered_continuation =
                    overlap_u >= min_true_w * 0.45 && du_center <= min_true_w * 0.35 + 15.0;
                let is_left_continuation = overlap_u >= min_true_w * 0.45
                    && dv <= last_line.true_h * 0.55
                    && height_ratio <= 1.35
                    && (du_left <= 6.0
                        || (du_left <= last_line.true_h * 0.55 + 4.0 && l.true_w <= row_w * 1.25));

                if is_same_row_fragment
                    || (is_adjacent_line && (is_centered_continuation || is_left_continuation))
                {
                    let align_penalty = if is_same_row_fragment {
                        du_gap * 0.5
                    } else if is_centered_continuation {
                        du_center * 0.5
                    } else {
                        du_left * 0.5
                    };
                    let score = dv + align_penalty;
                    if score < best_score {
                        best_score = score;
                        best_idx = Some(idx);
                    }
                }
            }
        }

        if let Some(idx) = best_idx {
            let c = &mut clusters[idx];
            c.x1 = c.x1.min(l.x1);
            c.y1 = c.y1.min(l.y1);
            c.x2 = c.x2.max(l.x2);
            c.y2 = c.y2.max(l.y2);
            c.dialogue_lines.push(l);
            c.furigana_lines.extend(item.furigana);
        } else {
            clusters.push(Cluster {
                x1: l.x1,
                y1: l.y1,
                x2: l.x2,
                y2: l.y2,
                dialogue_lines: vec![l],
                furigana_lines: item.furigana,
            });
        }
    }

    let mut bubbles = Vec::new();
    let mut bubble_id = 1;

    for c in clusters {
        if let Some(bubble) = rebuild_bubble_from_lines(
            bubble_id,
            c.dialogue_lines,
            c.furigana_lines,
            img_width,
            img_height,
            effective_source_lang,
            true,
        ) {
            bubbles.push(bubble);
            bubble_id += 1;
        }
    }

    bubbles
}
