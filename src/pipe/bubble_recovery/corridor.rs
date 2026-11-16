use image::{Rgb, RgbImage};

use crate::model::{Bubble, LineInfo};
use crate::pipe::clustering::is_interior_cleanup_line;

pub(super) const WALL_INNER_MARGIN: u32 = 6;
const WALL_SEARCH_OUT: i32 = 22;
const WALL_SEARCH_IN: i32 = 18;
pub(super) const LINE_COVER_PAD: f32 = 3.0;
pub(super) const MIN_INTERIOR_WIDTH: u32 = 24;

#[derive(Debug, Clone, Copy)]
pub(super) struct CorridorRow {
    pub y: u32,
    pub wall_left: u32,
    pub wall_right: u32,
}

#[derive(Debug, Clone)]
pub(super) struct BubbleCorridor {
    pub rows: Vec<CorridorRow>,
    pub top_y: u32,
    pub bottom_y: u32,
    pub min_left: u32,
    pub max_right: u32,
}

impl BubbleCorridor {
    pub(super) fn row_bounds_at(&self, y: u32) -> Option<(u32, u32)> {
        let idx = self.rows.binary_search_by_key(&y, |row| row.y).ok()?;
        let row = self.rows[idx];
        Some((row.wall_left, row.wall_right))
    }
}

fn pixel_luma_and_chroma(px: Rgb<u8>) -> (u8, u8) {
    let [r, g, b] = px.0;
    let max_c = r.max(g).max(b);
    let min_c = r.min(g).min(b);
    let luma = ((r as u16 * 77 + g as u16 * 150 + b as u16 * 29) >> 8) as u8;
    (luma, max_c - min_c)
}

fn is_bright_interior(px: Rgb<u8>) -> bool {
    let (luma, chroma) = pixel_luma_and_chroma(px);
    luma >= 210 && chroma <= 26
}

pub(super) fn is_dark_ink(px: Rgb<u8>) -> bool {
    let (luma, _) = pixel_luma_and_chroma(px);
    luma < 155
}

fn median_u32(values: &mut [u32]) -> Option<u32> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    Some(values[values.len() / 2])
}

pub(super) fn is_dialogue_text_line(line: &LineInfo) -> bool {
    !line.text.trim().is_empty() && !is_interior_cleanup_line(line)
}

pub(super) fn median_line_height(bubble: &Bubble) -> f32 {
    let mut heights: Vec<f32> = bubble
        .lines
        .iter()
        .filter(|line| is_dialogue_text_line(line))
        .map(|line| line.true_h.max(8.0))
        .collect();
    if heights.is_empty() {
        return bubble.h.clamp(14.0, 40.0);
    }
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    heights[heights.len() / 2]
}

fn has_bright_bubble_interior(img: &RgbImage, bubble: &Bubble) -> bool {
    let (w, h) = (img.width(), img.height());
    if w < 16 || h < 16 {
        return false;
    }

    let x1 = (bubble.x1.floor() as i32 - 4).clamp(0, w as i32 - 1) as u32;
    let y1 = (bubble.y1.floor() as i32 - 4).clamp(0, h as i32 - 1) as u32;
    let x2 = (bubble.x2.ceil() as i32 + 4).clamp(0, w as i32 - 1) as u32;
    let y2 = (bubble.y2.ceil() as i32 + 4).clamp(0, h as i32 - 1) as u32;
    if x2 <= x1 || y2 <= y1 {
        return false;
    }

    let step_x = ((x2 - x1) / 7).max(2);
    let step_y = ((y2 - y1) / 7).max(2);
    let mut total = 0u32;
    let mut bright = 0u32;

    let mut y = y1;
    while y <= y2 {
        let mut x = x1;
        while x <= x2 {
            total += 1;
            if is_bright_interior(*img.get_pixel(x, y)) {
                bright += 1;
            }
            x += step_x;
        }
        y += step_y;
    }

    bright >= 8 && (bright as f32 / total.max(1) as f32) >= 0.45
}

fn has_bright_run_right(img: &RgbImage, y: u32, start_x: u32, max_x: u32, needed: u32) -> bool {
    let mut count = 0u32;
    let end_x = (start_x + needed + 3).min(max_x);
    for x in start_x..=end_x {
        if is_bright_interior(*img.get_pixel(x, y)) {
            count += 1;
            if count >= needed {
                return true;
            }
        }
    }
    false
}

fn has_bright_run_left(img: &RgbImage, y: u32, start_x: u32, min_x: u32, needed: u32) -> bool {
    let mut count = 0u32;
    let begin_x = start_x.saturating_sub(needed + 3).max(min_x);
    for x in (begin_x..=start_x).rev() {
        if is_bright_interior(*img.get_pixel(x, y)) {
            count += 1;
            if count >= needed {
                return true;
            }
        }
    }
    false
}

fn find_initial_row_walls(img: &RgbImage, y: u32, bubble: &Bubble) -> Option<(u32, u32)> {
    let w = img.width();
    let max_reach = 95i32;

    let mut seed_left = (bubble.x1.floor() as i32 - 3).clamp(1, w as i32 - 2);
    while seed_left > (bubble.x1 as i32 - 12).max(1)
        && !is_bright_interior(*img.get_pixel(seed_left as u32, y))
    {
        seed_left -= 1;
    }
    if !is_bright_interior(*img.get_pixel(seed_left as u32, y)) {
        return None;
    }

    let mut seed_right = (bubble.x2.ceil() as i32 + 3).clamp(1, w as i32 - 2);
    while seed_right < (bubble.x2 as i32 + 12).min(w as i32 - 2)
        && !is_bright_interior(*img.get_pixel(seed_right as u32, y))
    {
        seed_right += 1;
    }
    if !is_bright_interior(*img.get_pixel(seed_right as u32, y)) {
        return None;
    }

    let left_limit = (seed_left - max_reach).max(0);
    let mut wall_left = None;
    for x in (left_limit..seed_left).rev() {
        if is_dark_ink(*img.get_pixel(x as u32, y))
            && has_bright_run_right(img, y, x as u32 + 1, seed_left as u32, 3)
        {
            wall_left = Some(x as u32);
            break;
        }
    }

    let right_limit = (seed_right + max_reach).min(w as i32 - 1);
    let mut wall_right = None;
    for x in (seed_right + 1)..=right_limit {
        if is_dark_ink(*img.get_pixel(x as u32, y))
            && has_bright_run_left(img, y, x as u32 - 1, seed_right as u32, 3)
        {
            wall_right = Some(x as u32);
            break;
        }
    }

    let (wl, wr) = (wall_left?, wall_right?);
    if wr.saturating_sub(wl) < MIN_INTERIOR_WIDTH {
        return None;
    }
    Some((wl, wr))
}

fn locate_anchor_walls(img: &RgbImage, bubble: &Bubble) -> Option<(u32, u32, u32)> {
    let h = img.height();
    let y_start = (bubble.y1.floor() as i32 - 2).clamp(0, h as i32 - 1) as u32;
    let y_end = (bubble.y2.ceil() as i32 + 2).clamp(0, h as i32 - 1) as u32;
    let step = ((y_end - y_start) / 8).max(1);

    let mut lefts = Vec::new();
    let mut rights = Vec::new();
    let mut ys = Vec::new();

    let mut y = y_start;
    while y <= y_end {
        if let Some((wl, wr)) = find_initial_row_walls(img, y, bubble) {
            lefts.push(wl);
            rights.push(wr);
            ys.push(y);
        }
        y += step;
    }

    let anchor_left = median_u32(&mut lefts)?;
    let anchor_right = median_u32(&mut rights)?;
    if anchor_right.saturating_sub(anchor_left) < MIN_INTERIOR_WIDTH {
        return None;
    }
    let anchor_y = ys[ys.len() / 2];
    Some((anchor_y, anchor_left, anchor_right))
}

fn track_left_wall(img: &RgbImage, y: u32, prev_left: u32, prev_right: u32) -> Option<u32> {
    let w = img.width() as i32;
    let search_min = (prev_left as i32 - WALL_SEARCH_OUT).max(0);
    let search_max = (prev_left as i32 + WALL_SEARCH_IN)
        .min(prev_right as i32 - MIN_INTERIOR_WIDTH as i32 / 2)
        .min(w - 2);
    if search_max <= search_min {
        return None;
    }

    let mut best_x = None;
    let mut best_dist = i32::MAX;
    for x in search_min..=search_max {
        if is_dark_ink(*img.get_pixel(x as u32, y))
            && has_bright_run_right(img, y, x as u32 + 1, prev_right, 3)
        {
            let dist = (x - prev_left as i32).abs();
            if dist < best_dist {
                best_dist = dist;
                best_x = Some(x as u32);
            }
        }
    }
    best_x
}

fn track_right_wall(img: &RgbImage, y: u32, prev_left: u32, prev_right: u32) -> Option<u32> {
    let w = img.width() as i32;
    let search_min = (prev_right as i32 - WALL_SEARCH_IN)
        .max(prev_left as i32 + MIN_INTERIOR_WIDTH as i32 / 2)
        .max(1);
    let search_max = (prev_right as i32 + WALL_SEARCH_OUT).min(w - 1);
    if search_max <= search_min {
        return None;
    }

    let mut best_x = None;
    let mut best_dist = i32::MAX;
    for x in search_min..=search_max {
        if is_dark_ink(*img.get_pixel(x as u32, y))
            && has_bright_run_left(img, y, x as u32 - 1, prev_left, 3)
        {
            let dist = (x - prev_right as i32).abs();
            if dist < best_dist {
                best_dist = dist;
                best_x = Some(x as u32);
            }
        }
    }
    best_x
}

fn row_is_horizontal_cap_or_empty(
    img: &RgbImage,
    y: u32,
    wall_left: u32,
    wall_right: u32,
) -> (bool, bool) {
    let safe_x1 = wall_left + WALL_INNER_MARGIN;
    let safe_x2 = wall_right.saturating_sub(WALL_INNER_MARGIN);
    if safe_x2 <= safe_x1 + 4 {
        return (true, false);
    }

    let span = (safe_x2 - safe_x1 + 1) as f32;
    let mut dark = 0u32;
    for x in safe_x1..=safe_x2 {
        if is_dark_ink(*img.get_pixel(x, y)) {
            dark += 1;
        }
    }
    let is_cap = (dark as f32 / span) > 0.52;
    let is_empty_white = dark == 0;
    (is_cap, is_empty_white)
}

fn scan_corridor_direction(
    img: &RgbImage,
    start_y: u32,
    init_left: u32,
    init_right: u32,
    step_y: i32,
    limit_y: u32,
    bubble_edge_y: u32,
    gap_limit: u32,
) -> Vec<CorridorRow> {
    let mut rows = Vec::new();
    let mut prev_left = init_left;
    let mut prev_right = init_right;
    let mut missing_both = 0u32;
    let mut missing_left = 0u32;
    let mut missing_right = 0u32;
    let mut empty_white_run = 0u32;
    let mut y = start_y as i32;

    loop {
        if (step_y < 0 && y < limit_y as i32) || (step_y > 0 && y > limit_y as i32) {
            break;
        }
        let yu = y as u32;

        let tracked_l = track_left_wall(img, yu, prev_left, prev_right);
        let tracked_r = track_right_wall(img, yu, prev_left, prev_right);

        match (tracked_l, tracked_r) {
            (Some(wl), Some(wr)) => {
                missing_both = 0;
                missing_left = 0;
                missing_right = 0;
                prev_left = wl;
                prev_right = wr;
            }
            (Some(wl), None) => {
                missing_left = 0;
                missing_right += 1;
                if missing_right > 14 {
                    break;
                }
                prev_left = wl;
            }
            (None, Some(wr)) => {
                missing_right = 0;
                missing_left += 1;
                if missing_left > 14 {
                    break;
                }
                prev_right = wr;
            }
            (None, None) => {
                missing_both += 1;
                if missing_both >= 8 {
                    break;
                }
            }
        }

        if prev_right.saturating_sub(prev_left) < MIN_INTERIOR_WIDTH {
            break;
        }

        let (is_cap, is_empty_white) =
            row_is_horizontal_cap_or_empty(img, yu, prev_left, prev_right);
        if is_cap {
            break;
        }

        let past_bubble_edge =
            (step_y < 0 && yu < bubble_edge_y) || (step_y > 0 && yu > bubble_edge_y);
        if past_bubble_edge {
            if is_empty_white {
                empty_white_run += 1;
                if empty_white_run >= gap_limit {
                    break;
                }
            } else {
                empty_white_run = 0;
            }
        }

        rows.push(CorridorRow {
            y: yu,
            wall_left: prev_left,
            wall_right: prev_right,
        });

        y += step_y;
    }

    rows
}

pub(super) fn trace_bubble_corridor(img: &RgbImage, bubble: &Bubble) -> Option<BubbleCorridor> {
    if !has_bright_bubble_interior(img, bubble) {
        return None;
    }
    let (anchor_y, anchor_left, anchor_right) = locate_anchor_walls(img, bubble)?;
    let line_h = median_line_height(bubble);
    let max_extend_y = (line_h * 3.8).clamp(70.0, 170.0) as u32;
    let gap_limit = (line_h * 2.2).clamp(28.0, 72.0) as u32;

    let min_y = (bubble.y1.floor() as u32).saturating_sub(max_extend_y);
    let max_y = ((bubble.y2.ceil() as u32) + max_extend_y).min(img.height().saturating_sub(1));
    let bubble_top = bubble.y1.floor().max(0.0) as u32;
    let bubble_bottom = (bubble.y2.ceil() as u32).min(img.height().saturating_sub(1));

    let mut up_rows = if anchor_y > min_y {
        scan_corridor_direction(
            img,
            anchor_y - 1,
            anchor_left,
            anchor_right,
            -1,
            min_y,
            bubble_top,
            gap_limit,
        )
    } else {
        Vec::new()
    };
    up_rows.reverse();

    let down_rows = scan_corridor_direction(
        img,
        anchor_y,
        anchor_left,
        anchor_right,
        1,
        max_y,
        bubble_bottom,
        gap_limit,
    );

    let mut rows = Vec::with_capacity(up_rows.len() + down_rows.len());
    rows.extend(up_rows);
    rows.extend(down_rows);
    if rows.is_empty() {
        return None;
    }

    let top_y = rows[0].y;
    let bottom_y = rows[rows.len() - 1].y;
    let min_left = rows.iter().map(|r| r.wall_left).min().unwrap_or(anchor_left);
    let max_right = rows.iter().map(|r| r.wall_right).max().unwrap_or(anchor_right);

    Some(BubbleCorridor {
        rows,
        top_y,
        bottom_y,
        min_left,
        max_right,
    })
}
