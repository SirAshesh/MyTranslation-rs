use crate::model::Bubble;
use image::{Rgb, RgbImage};

pub(super) fn pixel_distance(px: &Rgb<u8>, bg: [f32; 3]) -> f32 {
    let dr = px[0] as f32 - bg[0];
    let dg = px[1] as f32 - bg[1];
    let db = px[2] as f32 - bg[2];
    (dr * dr + dg * dg + db * db).sqrt()
}

pub(super) fn pixel_luminance(px: &Rgb<u8>) -> f32 {
    0.299 * px[0] as f32 + 0.587 * px[1] as f32 + 0.114 * px[2] as f32
}

pub(super) fn compute_block_start_y(
    center_y: f32,
    total_h: f32,
    min_y: f32,
    max_y: f32,
    edge_margin: f32,
    img_h: f32,
) -> f32 {
    let mut start_y = (center_y - total_h * 0.5).max(min_y);
    if start_y + total_h > max_y {
        start_y = (max_y - total_h).max(min_y);
    }
    start_y.clamp(edge_margin, (img_h - total_h - edge_margin).max(edge_margin))
}

fn scan_row_borders(
    img: &RgbImage,
    cx: i32,
    sy: u32,
    min_x: i32,
    max_x: i32,
) -> (Option<f32>, bool, Option<f32>, bool) {
    let w = img.width() as i32;
    let left = (min_x..=cx)
        .rev()
        .find(|&sx| pixel_luminance(img.get_pixel(sx as u32, sy)) < 150.0)
        .map(|sx| sx as f32 + 1.0);
    let right = (cx..=max_x)
        .find(|&sx| pixel_luminance(img.get_pixel(sx as u32, sy)) < 150.0)
        .map(|sx| sx as f32);
    let (l_val, l_real) = match left {
        Some(lx) => (Some(lx), true),
        None if min_x == 0 => (Some(0.0), false),
        None => (None, false),
    };
    let (r_val, r_real) = match right {
        Some(rx) => (Some(rx), true),
        None if max_x == w - 1 => (Some(w as f32), false),
        None => (None, false),
    };
    (l_val, l_real, r_val, r_real)
}

pub(super) fn measure_block_cap_safe_span(
    img: &RgbImage,
    center_x: f32,
    base_w: f32,
    base_h: f32,
    top_y: f32,
    bot_y: f32,
) -> Option<f32> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let cx = (center_x.round() as i32).clamp(0, w - 1);
    let max_scan = (base_w.max(base_h * 0.5) * 1.25 + 48.0).min(w as f32 * 0.45) as i32;
    let (min_x, max_x) = ((cx - max_scan).max(0), (cx + max_scan).min(w - 1));
    let mut min_span = f32::INFINITY;
    for y_f in [top_y, bot_y] {
        let sy = (y_f.round() as i32).clamp(0, h - 1) as u32;
        if pixel_luminance(img.get_pixel(cx as u32, sy)) < 200.0 {
            continue;
        }
        let (lx_opt, _, rx_opt, _) = scan_row_borders(img, cx, sy, min_x, max_x);
        if let (Some(lx), Some(rx)) = (lx_opt, rx_opt) {
            let span = ((center_x - (lx + 7.0)).min((rx - 7.0) - center_x) * 2.0).max(24.0);
            min_span = min_span.min(span);
        }
    }
    min_span.is_finite().then_some(min_span)
}

pub(super) fn detect_horizontal_bubble_bounds(
    img: &RgbImage,
    center_x: f32,
    center_y: f32,
    base_w: f32,
    base_h: f32,
) -> Option<(f32, f32)> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    if w < 16 || h < 16 {
        return None;
    }

    let cx = (center_x.round() as i32).clamp(0, w - 1);
    let cy = (center_y.round() as i32).clamp(0, h - 1);
    let mut sum = [0.0f32; 3];
    let mut samples = Vec::with_capacity(25);
    for dy in -2..=2 {
        let sy = (cy + dy).clamp(0, h - 1) as u32;
        for dx in -2..=2 {
            let p = *img.get_pixel((cx + dx).clamp(0, w - 1) as u32, sy);
            sum[0] += p[0] as f32;
            sum[1] += p[1] as f32;
            sum[2] += p[2] as f32;
            samples.push(p);
        }
    }
    let center_bg = [sum[0] / 25.0, sum[1] / 25.0, sum[2] / 25.0];
    if samples.iter().any(|p| pixel_distance(p, center_bg) > 25.0) {
        return None;
    }

    let center_lum = 0.299 * center_bg[0] + 0.587 * center_bg[1] + 0.114 * center_bg[2];
    let center_chroma = center_bg[0].max(center_bg[1]).max(center_bg[2])
        - center_bg[0].min(center_bg[1]).min(center_bg[2]);
    if center_lum < 228.0 || center_chroma > 18.0 {
        return None;
    }

    let max_scan = (base_w.max(base_h * 0.5) * 1.25 + 48.0).min(w as f32 * 0.45) as i32;
    let (min_x, max_x) = ((cx - max_scan).max(0), (cx + max_scan).min(w - 1));
    let (mut max_left, mut min_right) = (0.0_f32, w as f32);
    let (mut valid_rows, mut real_left, mut real_right) = (0usize, 0usize, 0usize);

    let find_interior_sy = |sign: i32| -> Option<i32> {
        for frac in [0.40_f32, 0.35, 0.28, 0.20] {
            let sy = (cy + sign * (base_h * frac).round() as i32).clamp(0, h - 1);
            let px = img.get_pixel(cx as u32, sy as u32);
            if pixel_luminance(px) >= 200.0 && pixel_distance(px, center_bg) <= 35.0 {
                return Some(sy);
            }
        }
        None
    };

    for sy_i in [find_interior_sy(-1), Some(cy), find_interior_sy(1)]
        .into_iter()
        .flatten()
    {
        let (lx_opt, lr, rx_opt, rr) = scan_row_borders(img, cx, sy_i as u32, min_x, max_x);
        if let (Some(lx), Some(rx)) = (lx_opt, rx_opt) {
            max_left = max_left.max(lx);
            min_right = min_right.min(rx);
            valid_rows += 1;
            real_left += usize::from(lr);
            real_right += usize::from(rr);
        }
    }

    if valid_rows < 3 || real_left < 2 || real_right < 2 || (min_right - max_left - 14.0) < 24.0
    {
        return None;
    }
    Some((max_left, min_right))
}

pub(super) fn detect_vertical_bubble_bounds(
    img: &RgbImage,
    center_x: f32,
    center_y: f32,
    base_h: f32,
) -> Option<(f32, f32)> {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let cx = (center_x.round() as i32).clamp(0, w - 1);
    let cy = (center_y.round() as i32).clamp(0, h - 1);
    let max_scan = ((base_h * 3.0).max(280.0)).min(h as f32 * 0.65) as i32;
    let (min_y, max_y) = ((cy - max_scan).max(0), (cy + max_scan).min(h - 1));
    let (mut max_top_border_y, mut min_bot_border_y, mut hit_both) = (0.0_f32, h as f32, 0usize);

    for dx in [-6_i32, 0, 6] {
        let sx = (cx + dx).clamp(0, w - 1) as u32;
        if pixel_luminance(img.get_pixel(sx, cy as u32)) < 200.0 {
            continue;
        }
        let top_hit = (min_y..=cy)
            .rev()
            .find(|&sy| pixel_luminance(img.get_pixel(sx, sy as u32)) < 150.0)
            .map(|sy| sy as f32 + 1.0);
        let bot_hit = (cy..=max_y)
            .find(|&sy| pixel_luminance(img.get_pixel(sx, sy as u32)) < 150.0)
            .map(|sy| sy as f32);
        if let (Some(ty), Some(by)) = (top_hit, bot_hit) {
            max_top_border_y = max_top_border_y.max(ty);
            min_bot_border_y = min_bot_border_y.min(by);
            hit_both += 1;
        }
    }

    if hit_both < 3 || (min_bot_border_y - max_top_border_y - 12.0) < 20.0 {
        return None;
    }
    Some((max_top_border_y, min_bot_border_y))
}

pub(super) fn clamp_bounds_against_neighbors(
    target: &Bubble,
    all_bubbles: &[Bubble],
    left_limit: &mut f32,
    right_limit: &mut f32,
    top_limit: &mut f32,
    bot_limit: &mut f32,
) {
    for other in all_bubbles {
        if other.id == target.id {
            continue;
        }
        let y_overlap = (other.y1 - 8.0) < target.y2 && (other.y2 + 8.0) > target.y1;
        if y_overlap {
            if other.x1 >= target.x2 - 8.0 {
                let mid = (target.x2 + other.x1) * 0.5;
                *right_limit = (*right_limit).min((mid - 2.0).max(target.x2));
            } else if other.x2 <= target.x1 + 8.0 {
                let mid = (other.x2 + target.x1) * 0.5;
                *left_limit = (*left_limit).max((mid + 2.0).min(target.x1));
            }
        }

        let x_overlap = (other.x1 - 8.0) < target.x2 && (other.x2 + 8.0) > target.x1;
        if x_overlap {
            if other.y1 >= target.y2 - 8.0 {
                let mid = (target.y2 + other.y1) * 0.5;
                *bot_limit = (*bot_limit).min((mid - 2.0).max(target.y2));
            } else if other.y2 <= target.y1 + 8.0 {
                let mid = (other.y2 + target.y1) * 0.5;
                *top_limit = (*top_limit).max((mid + 2.0).min(target.y1));
            }
        }
    }
}

pub fn resolve_bubble_interior_rect(
    img: &RgbImage,
    bubble: &Bubble,
    all_bubbles: &[Bubble],
) -> crate::model::Geometry {
    let fallback = crate::model::Geometry {
        x: bubble.x1.round() as i32,
        y: bubble.y1.round() as i32,
        width: (bubble.w.round() as i32).max(24),
        height: (bubble.h.round() as i32).max(20),
    };
    if bubble.angle_rad.abs() >= 0.026 {
        return fallback;
    }

    let orig_cx = if bubble.oriented_w > 1.0 {
        bubble.cx
    } else {
        (bubble.x1 + bubble.x2) * 0.5
    };
    let orig_cy = if bubble.oriented_h > 1.0 {
        bubble.cy
    } else {
        (bubble.y1 + bubble.y2) * 0.5
    };
    let base_w = if bubble.oriented_w > 1.0 {
        bubble.oriented_w
    } else {
        bubble.w
    };
    let base_h = if bubble.oriented_h > 1.0 {
        bubble.oriented_h
    } else {
        bubble.h
    };

    let (mut left_limit, mut right_limit) = (0.0_f32, img.width() as f32);
    let (mut top_limit, mut bot_limit) = (0.0_f32, img.height() as f32);
    clamp_bounds_against_neighbors(
        bubble,
        all_bubbles,
        &mut left_limit,
        &mut right_limit,
        &mut top_limit,
        &mut bot_limit,
    );

    let Some((left_b, right_b)) =
        detect_horizontal_bubble_bounds(img, orig_cx, orig_cy, base_w, base_h)
    else {
        return fallback;
    };
    let left_safe = (left_b + 8.0).max(left_limit);
    let right_safe = (right_b - 8.0).min(right_limit);
    if right_safe < left_safe + 24.0 {
        return fallback;
    }
    let center_x = (left_safe + right_safe) * 0.5;
    let mut box_w = (right_safe - left_safe).max(24.0);

    let (top_safe, bot_safe) = if let Some((top_b, bot_b)) =
        detect_vertical_bubble_bounds(img, center_x, orig_cy, base_h.max(base_w))
    {
        let ts = (top_b + 8.0).max(top_limit);
        let bs = (bot_b - 8.0).min(bot_limit);
        if bs >= ts + 20.0 {
            (ts, bs)
        } else {
            (bubble.y1.max(top_limit), bubble.y2.min(bot_limit))
        }
    } else {
        (bubble.y1.max(top_limit), bubble.y2.min(bot_limit))
    };

    let box_h = (bot_safe - top_safe).max(20.0);
    let mid_y = (top_safe + bot_safe) * 0.5;
    if let Some((mid_lb, mid_rb)) =
        detect_horizontal_bubble_bounds(img, center_x, mid_y, base_w.max(box_h * 0.5), box_h)
    {
        let ml = (mid_lb + 8.0).max(left_limit);
        let mr = (mid_rb - 8.0).min(right_limit);
        if mr >= ml + 24.0 {
            box_w = (mr - ml).max(24.0);
        }
    }
    if let Some(cap_span) = measure_block_cap_safe_span(
        img,
        center_x,
        box_w.max(base_w),
        box_h.max(base_h),
        top_safe + box_h * 0.22,
        top_safe + box_h * 0.78,
    ) {
        box_w = box_w.min(cap_span).max(24.0);
    }

    crate::model::Geometry {
        x: (center_x - box_w * 0.5).round().max(0.0) as i32,
        y: top_safe.round().max(0.0) as i32,
        width: box_w.round().max(24.0) as i32,
        height: box_h.round().max(20.0) as i32,
    }
}
