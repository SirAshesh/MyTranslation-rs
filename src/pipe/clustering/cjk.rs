use crate::model::LineInfo;
use crate::pipe::text_cleaner::contains_cjk;

pub(crate) const SYNTHETIC_INTERIOR_CLEANUP_TAG: &str = "__cjk_interior_cleanup__";

pub(super) struct LineWithFurigana {
    pub(super) line: LineInfo,
    pub(super) furigana: Vec<LineInfo>,
}

struct VerticalColumn {
    mean_cx: f32,
    max_w: f32,
    x1: f32,
    x2: f32,
    lines: Vec<LineInfo>,
}

pub(super) fn is_kana_char(ch: char) -> bool {
    matches!(ch, '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}')
}

fn is_han_char(ch: char) -> bool {
    matches!(ch, '\u{4E00}'..='\u{9FFF}')
}

fn has_han(text: &str) -> bool {
    text.chars().any(is_han_char)
}

fn count_cjk_chars(text: &str) -> usize {
    text.chars()
        .filter(|&ch| is_kana_char(ch) || is_han_char(ch))
        .count()
}

fn is_pure_kana_furigana(text: &str) -> bool {
    let mut kana_count = 0usize;
    for ch in text.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if matches!(ch, '?' | '!' | '？' | '！') || is_han_char(ch) {
            return false;
        }
        if is_kana_char(ch) {
            kana_count += 1;
        } else if ch != 'ー' {
            return false;
        }
    }
    kana_count >= 1
}

pub(super) fn is_vertical_cjk_line(line: &LineInfo) -> bool {
    contains_cjk(&line.text) && line.true_h >= line.true_w * 1.25
}

pub(super) fn is_short_cjk_chunk(line: &LineInfo) -> bool {
    let cjk_count = count_cjk_chars(&line.text);
    if !contains_cjk(&line.text) || !(1..=4).contains(&cjk_count) {
        return false;
    }
    let max_aspect = if cjk_count <= 2 { 2.4 } else { 1.45 };
    line.true_w < line.true_h * max_aspect
}

fn match_furigana_to_host(cand: &LineInfo, host: &LineInfo) -> Option<f32> {
    if !has_han(&host.text) {
        return None;
    }

    let vertical_ruby = cand.w <= host.w * 0.55
        && cand.cx >= host.cx + host.w * 0.25
        && (cand.x1 - host.x2).max(0.0) <= host.w * 0.65
        && cand.x2 <= host.x2 + host.w * 0.90;
    if vertical_ruby {
        let overlap_y = cand.y2.min(host.y2) - cand.y1.max(host.y1);
        if overlap_y >= cand.h.min(host.h) * 0.35 {
            let x_gap = (cand.x1 - host.x2).max(0.0);
            return Some(x_gap - overlap_y * 0.1);
        }
    }

    let horiz_ruby = host.true_w >= host.true_h * 1.20
        && cand.h <= host.h * 0.55
        && cand.cy <= host.cy - host.h * 0.25
        && (host.y1 - cand.y2).max(0.0) <= host.h * 0.65
        && cand.y1 >= host.y1 - host.h * 0.90;
    if horiz_ruby {
        let overlap_x = cand.x2.min(host.x2) - cand.x1.max(host.x1);
        if overlap_x >= cand.w.min(host.w) * 0.35 {
            let y_gap = (host.y1 - cand.y2).max(0.0);
            return Some(y_gap - overlap_x * 0.1);
        }
    }

    None
}

fn shares_column_with_dialogue(lines: &[LineInfo], cand_idx: usize) -> bool {
    let cand = &lines[cand_idx];
    lines.iter().enumerate().any(|(idx, other)| {
        if idx == cand_idx {
            return false;
        }
        let other_is_dialogue = has_han(&other.text) || !is_pure_kana_furigana(&other.text);
        if !other_is_dialogue {
            return false;
        }
        let max_w = cand.w.max(other.w);
        let min_w = cand.w.min(other.w);
        let x_overlap = cand.x2.min(other.x2) - cand.x1.max(other.x1);
        let dcx = (cand.cx - other.cx).abs();
        let v_gap = (cand.y1 - other.y2).max(other.y1 - cand.y2).max(0.0);
        dcx <= max_w * 0.45 && x_overlap >= min_w * 0.45 && v_gap <= max_w * 1.5
    })
}

pub(super) fn absorb_furigana(lines: Vec<LineInfo>) -> Vec<LineWithFurigana> {
    let mut host_idx_for_line: Vec<Option<usize>> = vec![None; lines.len()];
    let mut attached: Vec<Vec<LineInfo>> = vec![Vec::new(); lines.len()];

    for (idx, cand) in lines.iter().enumerate() {
        if !is_pure_kana_furigana(&cand.text) || shares_column_with_dialogue(&lines, idx) {
            continue;
        }
        let mut best_host = None;
        let mut best_score = f32::MAX;
        for (host_idx, host) in lines.iter().enumerate() {
            if host_idx == idx {
                continue;
            }
            if let Some(score) = match_furigana_to_host(cand, host) {
                if score < best_score {
                    best_score = score;
                    best_host = Some(host_idx);
                }
            }
        }
        host_idx_for_line[idx] = best_host;
    }

    for (idx, opt_host) in host_idx_for_line.iter().enumerate() {
        if let Some(host_idx) = opt_host {
            if host_idx_for_line[*host_idx].is_none() {
                attached[*host_idx].push(lines[idx].clone());
            }
        }
    }

    lines
        .into_iter()
        .enumerate()
        .filter_map(|(idx, line)| {
            if let Some(host_idx) = host_idx_for_line[idx] {
                if host_idx_for_line[host_idx].is_none() {
                    return None;
                }
            }
            let furigana = std::mem::take(&mut attached[idx]);
            Some(LineWithFurigana { line, furigana })
        })
        .collect()
}

pub(super) fn can_cluster_as_vertical_cjk(
    cand: &LineInfo,
    cluster_lines: &[LineInfo],
    is_lang_cjk: bool,
) -> Option<f32> {
    let cand_compat = is_vertical_cjk_line(cand) || (is_lang_cjk && is_short_cjk_chunk(cand));
    if !cand_compat {
        return None;
    }

    let cluster_has_wide_horiz = cluster_lines.iter().any(|cl| {
        !is_vertical_cjk_line(cl) && count_cjk_chars(&cl.text) > 4 && cl.true_w > cl.true_h * 1.25
    });
    if cluster_has_wide_horiz {
        return None;
    }

    let cluster_compat = cluster_lines
        .iter()
        .all(|cl| is_vertical_cjk_line(cl) || (is_lang_cjk && is_short_cjk_chunk(cl)))
        && (cluster_lines.iter().any(is_vertical_cjk_line)
            || is_lang_cjk && is_short_cjk_chunk(cand));
    if !cluster_compat {
        return None;
    }

    let mut best_score = f32::MAX;
    let mut matched = false;

    for cl in cluster_lines {
        let min_w = cand.w.min(cl.w);
        let max_w = cand.w.max(cl.w);
        let width_ratio = max_w / min_w.max(1.0);
        let x_overlap = cl.x2.min(cand.x2) - cl.x1.max(cand.x1);
        let has_tiny_chunk =
            count_cjk_chars(&cand.text) <= 2 || count_cjk_chars(&cl.text) <= 2;
        let max_width_ratio = if has_tiny_chunk && x_overlap >= min_w * 0.65 {
            3.0
        } else {
            2.2
        };
        if width_ratio > max_width_ratio {
            continue;
        }

        let dcx = (cand.cx - cl.cx).abs();
        let v_gap = (cand.y1 - cl.y2).max(cl.y1 - cand.y2).max(0.0);
        if (x_overlap >= min_w * 0.40 || dcx <= max_w * 0.45) && v_gap <= max_w * 1.45 {
            let score = v_gap + dcx * 0.5;
            if score < best_score {
                best_score = score;
                matched = true;
            }
        }

        let dx = (cl.x1 - cand.x2).max(cand.x1 - cl.x2).max(0.0);
        let dcy = (cand.cy - cl.cy).abs();
        let overlap_y = cl.y2.min(cand.y2) - cl.y1.max(cand.y1);
        let min_h = cand.h.min(cl.h);
        if width_ratio <= 2.2
            && dx <= max_w * 0.85 + 12.0
            && (overlap_y >= min_h * 0.35 || dcy <= min_h * 0.45 + 12.0)
        {
            let score = dx + dcy * 0.35;
            if score < best_score {
                best_score = score;
                matched = true;
            }
        }
    }

    if matched {
        Some(best_score)
    } else {
        None
    }
}

pub(super) fn is_vertical_bubble_lines(lines: &[LineInfo], c_w: f32, c_h: f32) -> bool {
    lines.iter().filter(|l| l.true_h > l.true_w * 1.15).count() * 2 >= lines.len()
        || (lines.iter().any(|l| contains_cjk(&l.text)) && c_h > c_w * 1.2)
}

fn group_into_rtl_columns(dialogue_lines: &[LineInfo]) -> Vec<VerticalColumn> {
    let mut sorted = dialogue_lines.to_vec();
    sorted.sort_by(|a, b| {
        b.cx.partial_cmp(&a.cx)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut columns: Vec<VerticalColumn> = Vec::new();
    for line in sorted {
        let mut target_idx = None;
        for (idx, col) in columns.iter().enumerate() {
            let ref_w = col.max_w.max(line.w);
            if (line.cx - col.mean_cx).abs() <= ref_w * 0.48 {
                target_idx = Some(idx);
                break;
            }
        }

        if let Some(idx) = target_idx {
            let col = &mut columns[idx];
            let count = col.lines.len() as f32;
            col.mean_cx = (col.mean_cx * count + line.cx) / (count + 1.0);
            col.max_w = col.max_w.max(line.w);
            col.x1 = col.x1.min(line.x1);
            col.x2 = col.x2.max(line.x2);
            col.lines.push(line);
        } else {
            columns.push(VerticalColumn {
                mean_cx: line.cx,
                max_w: line.w,
                x1: line.x1,
                x2: line.x2,
                lines: vec![line],
            });
        }
    }

    columns.sort_by(|a, b| {
        b.mean_cx
            .partial_cmp(&a.mean_cx)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for col in &mut columns {
        col.lines.sort_by(|a, b| {
            a.y1.partial_cmp(&b.y1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    columns
}

pub(super) fn sort_vertical_columns_rtl(dialogue_lines: &mut Vec<LineInfo>) {
    let columns = group_into_rtl_columns(dialogue_lines);
    dialogue_lines.clear();
    for col in columns {
        dialogue_lines.extend(col.lines);
    }
}

fn make_synthetic_line(x1: f32, y1: f32, x2: f32, y2: f32, text: String) -> LineInfo {
    let w = (x2 - x1).max(1.0);
    let h = (y2 - y1).max(1.0);
    LineInfo {
        quad: [[x1, y1], [x2, y1], [x2, y2], [x1, y2]],
        x1,
        y1,
        x2,
        y2,
        cx: (x1 + x2) * 0.5,
        cy: (y1 + y2) * 0.5,
        w,
        h,
        true_w: w,
        true_h: h,
        angle_rad: 0.0,
        text,
        score: 1.0,
    }
}

pub(super) fn build_cjk_interior_cleanup_line(
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    is_ja: bool,
    is_vertical_bubble: bool,
    img_width: u32,
    img_height: u32,
) -> LineInfo {
    let bubble_w = (x2 - x1).max(1.0);
    let right_pad = if is_ja && is_vertical_bubble {
        6.0 + (bubble_w * 0.18).min(14.0)
    } else {
        6.0
    };
    let cx1 = (x1 - 2.0).max(0.0);
    let cy1 = (y1 - 2.0).max(0.0);
    let cx2 = (x2 + right_pad).min(img_width as f32).max(cx1 + 1.0);
    let cy2 = (y2 + 2.0).min(img_height as f32).max(cy1 + 1.0);
    make_synthetic_line(
        cx1,
        cy1,
        cx2,
        cy2,
        SYNTHETIC_INTERIOR_CLEANUP_TAG.to_string(),
    )
}

pub(super) fn build_vertical_bridge_lines(dialogue_lines: &[LineInfo]) -> Vec<LineInfo> {
    if dialogue_lines.len() < 2 {
        return Vec::new();
    }

    let columns = group_into_rtl_columns(dialogue_lines);
    let mut bridges = Vec::new();

    for col in &columns {
        for pair in col.lines.windows(2) {
            let top = &pair[0];
            let bot = &pair[1];
            let v_gap = bot.y1 - top.y2;
            let ref_w = top.w.max(bot.w);
            if v_gap > 1.0 && v_gap <= ref_w * 1.45 {
                let bx1 = top.x1.min(bot.x1);
                let bx2 = top.x2.max(bot.x2);
                bridges.push(make_synthetic_line(
                    bx1,
                    top.y2 - 1.0,
                    bx2,
                    bot.y1 + 1.0,
                    String::new(),
                ));
            }
        }
    }

    if columns.len() >= 2 {
        let cluster_top_y = dialogue_lines
            .iter()
            .map(|l| l.y1)
            .fold(f32::INFINITY, f32::min);
        for col in &columns {
            if let Some(first) = col.lines.first() {
                let top_gap = first.y1 - cluster_top_y;
                if top_gap >= col.max_w * 0.40 && top_gap <= col.max_w * 1.60 {
                    bridges.push(make_synthetic_line(
                        col.x1,
                        cluster_top_y,
                        col.x2,
                        first.y1 + 1.0,
                        String::new(),
                    ));
                }
            }
        }
    }

    bridges
}
