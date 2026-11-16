use image::RgbImage;

use crate::model::{Bubble, LineInfo};

use super::corridor::{
    is_dark_ink, is_dialogue_text_line, BubbleCorridor, CorridorRow, LINE_COVER_PAD,
    MIN_INTERIOR_WIDTH, WALL_INNER_MARGIN,
};

#[derive(Debug, Clone, Copy)]
pub(super) struct InkBand {
    pub y1: u32,
    pub y2: u32,
    pub x1: u32,
    pub x2: u32,
    pub total_ink: u32,
}

fn is_pixel_covered_by_lines(x: f32, y: f32, lines: &[LineInfo]) -> bool {
    lines.iter().filter(|l| is_dialogue_text_line(l)).any(|line| {
        x >= line.x1 - LINE_COVER_PAD
            && x <= line.x2 + LINE_COVER_PAD
            && y >= line.y1 - LINE_COVER_PAD
            && y <= line.y2 + LINE_COVER_PAD
    })
}

pub(super) fn is_point_inside_other_bubble(
    x: f32,
    y: f32,
    bubbles: &[Bubble],
    exclude_idx: usize,
) -> bool {
    bubbles.iter().enumerate().any(|(idx, other)| {
        idx != exclude_idx
            && x >= other.x1 - LINE_COVER_PAD
            && x <= other.x2 + LINE_COVER_PAD
            && y >= other.y1 - LINE_COVER_PAD
            && y <= other.y2 + LINE_COVER_PAD
    })
}

fn count_isolated_uncovered_ink_in_row(
    img: &RgbImage,
    row: CorridorRow,
    bubbles: &[Bubble],
    bubble_idx: usize,
) -> Option<(u32, u32, u32)> {
    let safe_x1 = row.wall_left + WALL_INNER_MARGIN;
    let safe_x2 = row.wall_right.saturating_sub(WALL_INNER_MARGIN);
    if safe_x2 <= safe_x1 + 6 {
        return None;
    }

    let y = row.y;
    let yf = y as f32;
    let owned_lines = &bubbles[bubble_idx].lines;

    let mut left_ink_free = row.wall_left + 1;
    while left_ink_free < safe_x2 && is_dark_ink(*img.get_pixel(left_ink_free, y)) {
        left_ink_free += 1;
    }
    let mut right_ink_free = row.wall_right.saturating_sub(1);
    while right_ink_free > left_ink_free && is_dark_ink(*img.get_pixel(right_ink_free, y)) {
        right_ink_free -= 1;
    }

    let scan_x1 = (left_ink_free + 2).max(safe_x1);
    let scan_x2 = right_ink_free.saturating_sub(2).min(safe_x2);
    if scan_x2 <= scan_x1 + 4 {
        return None;
    }

    let mut count = 0u32;
    let mut min_x = u32::MAX;
    let mut max_x = 0u32;
    for x in scan_x1..=scan_x2 {
        if !is_dark_ink(*img.get_pixel(x, y)) {
            continue;
        }
        let xf = x as f32;
        if is_pixel_covered_by_lines(xf, yf, owned_lines)
            || is_point_inside_other_bubble(xf, yf, bubbles, bubble_idx)
        {
            continue;
        }
        count += 1;
        min_x = min_x.min(x);
        max_x = max_x.max(x);
    }

    if count >= 2 {
        Some((count, min_x, max_x))
    } else {
        None
    }
}

pub(super) fn find_uncovered_ink_bands(
    img: &RgbImage,
    corridor: &BubbleCorridor,
    bubbles: &[Bubble],
    bubble_idx: usize,
) -> Vec<InkBand> {
    let mut bands = Vec::new();
    let mut active: Option<InkBand> = None;
    let mut empty_gap = 0u32;

    for &row in &corridor.rows {
        if let Some((count, min_x, max_x)) =
            count_isolated_uncovered_ink_in_row(img, row, bubbles, bubble_idx)
        {
            empty_gap = 0;
            active = Some(match active {
                Some(mut band) => {
                    band.y2 = row.y;
                    band.x1 = band.x1.min(min_x);
                    band.x2 = band.x2.max(max_x);
                    band.total_ink += count;
                    band
                }
                None => InkBand {
                    y1: row.y,
                    y2: row.y,
                    x1: min_x,
                    x2: max_x,
                    total_ink: count,
                },
            });
        } else if let Some(band) = active {
            empty_gap += 1;
            if empty_gap > 2 {
                if band_is_text_like(band) {
                    bands.push(band);
                }
                active = None;
                empty_gap = 0;
            }
        }
    }

    if let Some(band) = active {
        if band_is_text_like(band) {
            bands.push(band);
        }
    }

    bands
}

fn band_is_text_like(band: InkBand) -> bool {
    let band_h = band.y2 - band.y1 + 1;
    let band_w = band.x2.saturating_sub(band.x1) + 1;
    band_h >= 6 && band_w >= 8 && band.total_ink >= 16
}

pub(super) fn is_corridor_unseparated_between(
    img: &RgbImage,
    corridor: &BubbleCorridor,
    y_top: u32,
    y_bot: u32,
) -> bool {
    for y in y_top..=y_bot {
        let Some((wl, wr)) = corridor.row_bounds_at(y) else {
            return false;
        };
        let span = wr.saturating_sub(wl);
        if span < MIN_INTERIOR_WIDTH {
            return false;
        }
        let mid_x1 = wl + span / 4;
        let mid_x2 = wr - span / 4;
        let mid_w = mid_x2.saturating_sub(mid_x1) + 1;
        let mut dark = 0u32;
        for x in mid_x1..=mid_x2 {
            if is_dark_ink(*img.get_pixel(x, y)) {
                dark += 1;
            }
        }
        if (dark as f32 / mid_w.max(1) as f32) > 0.55 {
            return false;
        }
    }
    true
}
