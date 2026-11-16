mod corridor;
mod ink_scan;

use std::collections::HashSet;

use image::RgbImage;
use tracing::info;

use crate::model::{Bubble, LineInfo, OcrLineItem};
use crate::pipe::clustering::{line_from_ocr_item, rebuild_bubble_from_lines};
use crate::pipe::ocr::OcrManager;

use corridor::{is_dialogue_text_line, median_line_height, trace_bubble_corridor, BubbleCorridor};
use ink_scan::{
    find_uncovered_ink_bands, is_corridor_unseparated_between, is_point_inside_other_bubble,
    InkBand,
};

fn line_overlaps_band(line: &LineInfo, band: InkBand) -> bool {
    let bx1 = band.x1 as f32 - 5.0;
    let by1 = band.y1 as f32 - 5.0;
    let bx2 = band.x2 as f32 + 5.0;
    let by2 = band.y2 as f32 + 5.0;
    line.x2 >= bx1 && line.x1 <= bx2 && line.y2 >= by1 && line.y1 <= by2
}

fn line_overlaps_existing(candidate: &LineInfo, existing: &[LineInfo]) -> bool {
    existing.iter().any(|line| {
        if candidate.cx >= line.x1 - 2.0
            && candidate.cx <= line.x2 + 2.0
            && candidate.cy >= line.y1 - 2.0
            && candidate.cy <= line.y2 + 2.0
        {
            return true;
        }
        let ix1 = candidate.x1.max(line.x1);
        let iy1 = candidate.y1.max(line.y1);
        let ix2 = candidate.x2.min(line.x2);
        let iy2 = candidate.y2.min(line.y2);
        if ix2 <= ix1 || iy2 <= iy1 {
            return false;
        }
        let inter = (ix2 - ix1) * (iy2 - iy1);
        let min_area = (candidate.w * candidate.h)
            .min(line.w * line.h)
            .max(1.0);
        inter / min_area >= 0.30
    })
}

fn is_valid_recovered_line(
    line: &LineInfo,
    corridor: &BubbleCorridor,
    bubbles: &[Bubble],
    bubble_idx: usize,
) -> bool {
    if line.score < 0.32 || !line.text.chars().any(|ch| ch.is_alphanumeric()) {
        return false;
    }
    let cy_u = line.cy.round().max(0.0) as u32;
    let Some((wl, wr)) = corridor.row_bounds_at(cy_u) else {
        return false;
    };
    if line.cx <= (wl + 2) as f32 || line.cx >= wr.saturating_sub(2) as f32 {
        return false;
    }
    if is_point_inside_other_bubble(line.cx, line.cy, bubbles, bubble_idx) {
        return false;
    }
    !line_overlaps_existing(line, &bubbles[bubble_idx].lines)
}

fn absorb_same_corridor_siblings(
    img: &RgbImage,
    corridors: &[Option<BubbleCorridor>],
    bubbles: &mut Vec<Bubble>,
    source_lang: &str,
) -> usize {
    let mut absorbed = vec![false; bubbles.len()];
    let mut merges = 0usize;

    for i in 0..bubbles.len() {
        if absorbed[i] {
            continue;
        }
        let Some(ref corridor) = corridors[i] else {
            continue;
        };
        let line_h = median_line_height(&bubbles[i]);
        let mut extra_lines = Vec::new();

        for j in 0..bubbles.len() {
            if i == j || absorbed[j] {
                continue;
            }
            let text_lines: Vec<LineInfo> = bubbles[j]
                .lines
                .iter()
                .filter(|l| is_dialogue_text_line(l))
                .cloned()
                .collect();
            if text_lines.is_empty() || text_lines.len() > 5 {
                continue;
            }
            let cy_u = bubbles[j].cy.round().max(0.0) as u32;
            let Some((wl, wr)) = corridor.row_bounds_at(cy_u) else {
                continue;
            };
            if bubbles[j].cx <= (wl + 3) as f32 || bubbles[j].cx >= wr.saturating_sub(3) as f32 {
                continue;
            }
            let v_gap = if bubbles[j].y2 < bubbles[i].y1 {
                bubbles[i].y1 - bubbles[j].y2
            } else if bubbles[i].y2 < bubbles[j].y1 {
                bubbles[j].y1 - bubbles[i].y2
            } else {
                0.0
            };
            if v_gap > line_h * 2.2 {
                continue;
            }

            let gap_top = (bubbles[i].y2.min(bubbles[j].y2)).round().max(0.0) as u32 + 2;
            let gap_bot =
                ((bubbles[i].y1.max(bubbles[j].y1)).round().max(0.0) as u32).saturating_sub(2);
            if gap_top <= gap_bot
                && !is_corridor_unseparated_between(img, corridor, gap_top, gap_bot)
            {
                continue;
            }

            absorbed[j] = true;
            extra_lines.extend(text_lines);
            merges += 1;
        }

        if !extra_lines.is_empty() {
            let mut combined: Vec<LineInfo> = bubbles[i]
                .lines
                .iter()
                .filter(|l| is_dialogue_text_line(l))
                .cloned()
                .collect();
            combined.extend(extra_lines);
            if let Some(rebuilt) = rebuild_bubble_from_lines(
                bubbles[i].id,
                combined,
                Vec::new(),
                img.width(),
                img.height(),
                source_lang,
                false,
            ) {
                bubbles[i] = rebuilt;
            }
        }
    }

    if merges > 0 {
        let mut idx = 0usize;
        bubbles.retain(|_| {
            let keep = !absorbed[idx];
            idx += 1;
            keep
        });
        for (new_id, bubble) in bubbles.iter_mut().enumerate() {
            bubble.id = new_id + 1;
        }
    }

    merges
}

pub(crate) fn recover_bubbles_with_ocr_fn<F>(
    img: &RgbImage,
    raw_ocr_lines: &[OcrLineItem],
    bubbles: &mut Vec<Bubble>,
    source_lang: &str,
    mut run_region_ocr: F,
) -> usize
where
    F: FnMut(u32, u32, u32, u32) -> Result<Vec<OcrLineItem>, String>,
{
    if bubbles.is_empty() {
        return 0;
    }

    let (img_w, img_h) = (img.width(), img.height());
    let corridors: Vec<Option<BubbleCorridor>> = bubbles
        .iter()
        .map(|bubble| trace_bubble_corridor(img, bubble))
        .collect();

    let mut total_recovered = absorb_same_corridor_siblings(img, &corridors, bubbles, source_lang);
    let refreshed_corridors: Vec<Option<BubbleCorridor>> = if total_recovered > 0 {
        bubbles
            .iter()
            .map(|bubble| trace_bubble_corridor(img, bubble))
            .collect()
    } else {
        corridors
    };

    for idx in 0..bubbles.len() {
        let Some(ref corridor) = refreshed_corridors[idx] else {
            continue;
        };
        let uncovered_bands = find_uncovered_ink_bands(img, corridor, bubbles, idx);
        if uncovered_bands.is_empty() {
            continue;
        }

        let mut recovered_lines = Vec::new();
        let mut covered_band_indices = HashSet::new();

        for item in raw_ocr_lines {
            let Some(line) = line_from_ocr_item(item, img_w, img_h) else {
                continue;
            };
            if !is_valid_recovered_line(&line, corridor, bubbles, idx) {
                continue;
            }
            let mut hits_band = false;
            for (b_idx, &band) in uncovered_bands.iter().enumerate() {
                if line_overlaps_band(&line, band) {
                    covered_band_indices.insert(b_idx);
                    hits_band = true;
                }
            }
            if hits_band {
                recovered_lines.push(line);
            }
        }

        if covered_band_indices.len() < uncovered_bands.len() {
            let crop_x1 = corridor.min_left.saturating_sub(6);
            let crop_y1 = corridor.top_y.saturating_sub(6);
            let crop_x2 = (corridor.max_right + 6).min(img_w);
            let crop_y2 = (corridor.bottom_y + 6).min(img_h);

            if let Ok(crop_lines) = run_region_ocr(crop_x1, crop_y1, crop_x2, crop_y2) {
                for item in crop_lines {
                    let Some(line) = line_from_ocr_item(&item, img_w, img_h) else {
                        continue;
                    };
                    if !is_valid_recovered_line(&line, corridor, bubbles, idx)
                        || line_overlaps_existing(&line, &recovered_lines)
                    {
                        continue;
                    }
                    recovered_lines.push(line);
                }
            }
        }

        if recovered_lines.is_empty() {
            continue;
        }

        total_recovered += recovered_lines.len();
        let mut combined: Vec<LineInfo> = bubbles[idx]
            .lines
            .iter()
            .filter(|l| is_dialogue_text_line(l))
            .cloned()
            .collect();
        combined.extend(recovered_lines);

        if let Some(rebuilt) = rebuild_bubble_from_lines(
            bubbles[idx].id,
            combined,
            Vec::new(),
            img_w,
            img_h,
            source_lang,
            false,
        ) {
            bubbles[idx] = rebuilt;
        }
    }

    if total_recovered > 0 {
        info!(
            "Bubble interior recovery recovered/merged {} in-bubble lines/fragments",
            total_recovered
        );
    }

    total_recovered
}

pub fn recover_missed_bubble_text(
    img: &RgbImage,
    ocr: &OcrManager,
    raw_ocr_lines: &[OcrLineItem],
    bubbles: &mut Vec<Bubble>,
    source_lang: &str,
) -> usize {
    recover_bubbles_with_ocr_fn(img, raw_ocr_lines, bubbles, source_lang, |x1, y1, x2, y2| {
        ocr.run_ocr_region(img, x1, y1, x2, y2)
    })
}

#[cfg(test)]
mod tests;
