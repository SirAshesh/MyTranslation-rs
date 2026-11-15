use crate::model::{Bubble, LineInfo};
use image::{Rgb, RgbImage};
use std::collections::VecDeque;

pub(super) const ROI_PAD: f32 = 14.0;

pub(super) fn line_has_trailing_punct(line: &LineInfo) -> bool {
    let trimmed = line.text.trim_end();
    trimmed.ends_with('?')
        || trimmed.ends_with('!')
        || trimmed.ends_with('.')
        || trimmed.ends_with('？')
        || trimmed.ends_with('！')
        || trimmed.ends_with('…')
}

pub(super) fn trailing_tail_extension(line: &LineInfo) -> f32 {
    (line.true_h * 1.35).clamp(32.0, 180.0)
}

pub(super) fn line_axes(quad: &[[f32; 2]; 4]) -> ([f32; 2], [f32; 2], f32, f32, f32, f32) {
    let ux_raw = quad[1][0] - quad[0][0];
    let uy_raw = quad[1][1] - quad[0][1];
    let u_len = ux_raw.hypot(uy_raw).max(1e-3);
    let u = [ux_raw / u_len, uy_raw / u_len];

    let vx_raw = quad[3][0] - quad[0][0];
    let vy_raw = quad[3][1] - quad[0][1];
    let perp = [-u[1], u[0]];
    let v = if vx_raw * perp[0] + vy_raw * perp[1] >= 0.0 {
        perp
    } else {
        [u[1], -u[0]]
    };

    let mut u_min = f32::INFINITY;
    let mut u_max = f32::NEG_INFINITY;
    let mut v_min = f32::INFINITY;
    let mut v_max = f32::NEG_INFINITY;
    for p in quad {
        let pu = p[0] * u[0] + p[1] * u[1];
        let pv = p[0] * v[0] + p[1] * v[1];
        u_min = u_min.min(pu);
        u_max = u_max.max(pu);
        v_min = v_min.min(pv);
        v_max = v_max.max(pv);
    }
    (u, v, u_min, u_max, v_min, v_max)
}

pub(super) struct RoiBuffers {
    pub(super) x1: u32,
    pub(super) y1: u32,
    pub(super) w: usize,
    pub(super) h: usize,
    pub(super) lums: Vec<u8>,
    pub(super) r: Vec<f32>,
    pub(super) g: Vec<f32>,
    pub(super) b: Vec<f32>,
}

impl RoiBuffers {
    pub(super) fn from_image(
        img: &RgbImage,
        bubble: &Bubble,
        img_w: u32,
        img_h: u32,
    ) -> Option<Self> {
        let mut min_x = bubble.x1 - ROI_PAD;
        let mut min_y = bubble.y1 - ROI_PAD;
        let mut max_x = bubble.x2 + ROI_PAD;
        let mut max_y = bubble.y2 + ROI_PAD;

        for line in &bubble.lines {
            min_x = min_x.min(line.x1 - ROI_PAD);
            min_y = min_y.min(line.y1 - ROI_PAD);
            max_x = max_x.max(line.x2 + ROI_PAD);
            max_y = max_y.max(line.y2 + ROI_PAD);
            if line.h > line.w * 1.35 || line.true_h > line.true_w * 1.35 {
                min_x = min_x.min(line.x1 - 4.0 - ROI_PAD);
                max_x = max_x.max(line.x2 + 4.0 + ROI_PAD);
                max_y = max_y.max(line.y2 + line.w * 1.15 + ROI_PAD);
            }
            if !line_has_trailing_punct(line) {
                continue;
            }
            let tail_ext = trailing_tail_extension(line);
            let (u, _, _, _, _, _) = line_axes(&line.quad);
            for p in [line.quad[1], line.quad[2]] {
                let ex = p[0] + u[0] * tail_ext;
                let ey = p[1] + u[1] * tail_ext;
                min_x = min_x.min(ex - ROI_PAD);
                min_y = min_y.min(ey - ROI_PAD);
                max_x = max_x.max(ex + ROI_PAD);
                max_y = max_y.max(ey + ROI_PAD);
            }
        }

        let x1 = min_x.max(0.0) as u32;
        let y1 = min_y.max(0.0) as u32;
        let x2 = max_x.min(img_w as f32 - 1.0) as u32;
        let y2 = max_y.min(img_h as f32 - 1.0) as u32;
        if x2 <= x1 || y2 <= y1 {
            return None;
        }
        let w = (x2 - x1 + 1) as usize;
        let h = (y2 - y1 + 1) as usize;
        if w <= 4 || h <= 4 {
            return None;
        }

        let len = w * h;
        let mut lums = vec![0u8; len];
        let mut r = vec![0.0_f32; len];
        let mut g = vec![0.0_f32; len];
        let mut b = vec![0.0_f32; len];

        for dy in 0..h {
            let y = y1 + dy as u32;
            for dx in 0..w {
                let x = x1 + dx as u32;
                let p = img.get_pixel(x, y);
                let idx = dy * w + dx;
                lums[idx] = pixel_luminance(p);
                r[idx] = p[0] as f32;
                g[idx] = p[1] as f32;
                b[idx] = p[2] as f32;
            }
        }

        Some(Self {
            x1,
            y1,
            w,
            h,
            lums,
            r,
            g,
            b,
        })
    }

    pub(super) fn rgb_at(&self, idx: usize) -> [f32; 3] {
        [self.r[idx], self.g[idx], self.b[idx]]
    }
}

pub(super) fn pixel_luminance(p: &Rgb<u8>) -> u8 {
    (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8
}

pub(super) fn rgb_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dr = a[0] - b[0];
    let dg = a[1] - b[1];
    let db = a[2] - b[2];
    (dr * dr + dg * dg + db * db).sqrt()
}

pub(super) fn estimate_dominant_bg(samples: &[[f32; 3]]) -> Option<([f32; 3], f32)> {
    if samples.len() < 12 {
        return None;
    }

    let mut counts = [0usize; 4096];
    for &[r, g, b] in samples {
        let br = ((r as usize) >> 4).min(15);
        let bg = ((g as usize) >> 4).min(15);
        let bb = ((b as usize) >> 4).min(15);
        counts[(br << 8) | (bg << 4) | bb] += 1;
    }

    let (best_bin, _) = counts.iter().enumerate().max_by_key(|(_, &c)| c)?;
    let seed_center = [
        ((best_bin >> 8) * 16 + 8) as f32,
        (((best_bin >> 4) & 15) * 16 + 8) as f32,
        ((best_bin & 15) * 16 + 8) as f32,
    ];

    let mut sum = [0.0_f32; 3];
    let mut matched = 0usize;
    for &rgb in samples {
        if rgb_distance(rgb, seed_center) <= 26.0 {
            sum[0] += rgb[0];
            sum[1] += rgb[1];
            sum[2] += rgb[2];
            matched += 1;
        }
    }

    if matched == 0 {
        return None;
    }

    let inv = 1.0 / matched as f32;
    let mode_rgb = [sum[0] * inv, sum[1] * inv, sum[2] * inv];
    let uniform_count = samples
        .iter()
        .filter(|&&rgb| rgb_distance(rgb, mode_rgb) <= 28.0)
        .count();

    Some((mode_rgb, uniform_count as f32 / samples.len() as f32))
}

pub(super) fn compute_row_ring_bg(
    roi: &RoiBuffers,
    any_allow: &[bool],
    any_outer: &[bool],
    fallback: [f32; 3],
) -> Vec<[f32; 3]> {
    let mut row_rgb = vec![fallback; roi.h];
    for dy in 0..roi.h {
        let y_start = dy.saturating_sub(6);
        let y_end = (dy + 6).min(roi.h - 1);
        let mut rs = Vec::new();
        let mut gs = Vec::new();
        let mut bs = Vec::new();
        for ny in y_start..=y_end {
            let row_off = ny * roi.w;
            for dx in 0..roi.w {
                let idx = row_off + dx;
                if any_outer[idx] && !any_allow[idx] {
                    rs.push(roi.r[idx]);
                    gs.push(roi.g[idx]);
                    bs.push(roi.b[idx]);
                }
            }
        }
        if !rs.is_empty() {
            let mid = rs.len() / 2;
            rs.sort_by(|a, b| a.total_cmp(b));
            gs.sort_by(|a, b| a.total_cmp(b));
            bs.sort_by(|a, b| a.total_cmp(b));
            row_rgb[dy] = [rs[mid], gs[mid], bs[mid]];
        }
    }
    row_rgb
}

pub(super) fn compute_local_extrema(
    lums: &[u8],
    box_w: usize,
    box_h: usize,
    radius: usize,
) -> (Vec<u8>, Vec<u8>) {
    let mut row_min = vec![0u8; box_w * box_h];
    let mut row_max = vec![0u8; box_w * box_h];

    for dy in 0..box_h {
        let row_off = dy * box_w;
        for dx in 0..box_w {
            let x_start = dx.saturating_sub(radius);
            let x_end = (dx + radius).min(box_w - 1);
            let mut mn = 255u8;
            let mut mx = 0u8;
            for nx in x_start..=x_end {
                let v = lums[row_off + nx];
                mn = mn.min(v);
                mx = mx.max(v);
            }
            row_min[row_off + dx] = mn;
            row_max[row_off + dx] = mx;
        }
    }

    let mut local_min = vec![0u8; box_w * box_h];
    let mut local_max = vec![0u8; box_w * box_h];

    for dy in 0..box_h {
        let y_start = dy.saturating_sub(radius);
        let y_end = (dy + radius).min(box_h - 1);
        for dx in 0..box_w {
            let mut mn = 255u8;
            let mut mx = 0u8;
            for ny in y_start..=y_end {
                let idx = ny * box_w + dx;
                mn = mn.min(row_min[idx]);
                mx = mx.max(row_max[idx]);
            }
            let out_idx = dy * box_w + dx;
            local_min[out_idx] = mn;
            local_max[out_idx] = mx;
        }
    }

    (local_min, local_max)
}

pub(super) fn dilate_mask(mask: &mut [bool], allow: &[bool], w: usize, h: usize, steps: usize) {
    for _ in 0..steps {
        let prev = mask.to_vec();
        for dy in 0..h {
            for dx in 0..w {
                if !prev[dy * w + dx] {
                    continue;
                }
                for ndy in dy.saturating_sub(1)..=(dy + 1).min(h - 1) {
                    for ndx in dx.saturating_sub(1)..=(dx + 1).min(w - 1) {
                        let nidx = ndy * w + ndx;
                        if allow[nidx] {
                            mask[nidx] = true;
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn absorb_contrasting_halo_with_threshold(
    mask: &mut [bool],
    allow: &[bool],
    roi: &RoiBuffers,
    row_bg: &[[f32; 3]],
    threshold: f32,
    steps: usize,
) {
    for _ in 0..steps {
        let prev = mask.to_vec();
        let mut changed = false;
        for dy in 0..roi.h {
            let bg = row_bg[dy];
            for dx in 0..roi.w {
                if !prev[dy * roi.w + dx] {
                    continue;
                }
                for ndy in dy.saturating_sub(1)..=(dy + 1).min(roi.h - 1) {
                    for ndx in dx.saturating_sub(1)..=(dx + 1).min(roi.w - 1) {
                        let nidx = ndy * roi.w + ndx;
                        if mask[nidx] || !allow[nidx] {
                            continue;
                        }
                        if rgb_distance(roi.rgb_at(nidx), bg) > threshold {
                            mask[nidx] = true;
                            changed = true;
                        }
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
}

pub(super) fn absorb_contrasting_halo(
    mask: &mut [bool],
    allow: &[bool],
    roi: &RoiBuffers,
    row_bg: &[[f32; 3]],
    steps: usize,
) {
    absorb_contrasting_halo_with_threshold(mask, allow, roi, row_bg, 11.0, steps);
}

fn initialize_mask_bfs(
    mask: &[bool],
    uniform_bg: &[Option<[f32; 3]>],
    roi: &mut RoiBuffers,
) {
    let len = roi.w * roi.h;
    let mut filled = vec![false; len];
    let mut queue = VecDeque::new();
    let mut queued = vec![false; len];

    for dy in 0..roi.h {
        for dx in 0..roi.w {
            let idx = dy * roi.w + dx;
            if !mask[idx] {
                filled[idx] = true;
                continue;
            }
            if let Some(bg_rgb) = uniform_bg[idx] {
                roi.r[idx] = bg_rgb[0];
                roi.g[idx] = bg_rgb[1];
                roi.b[idx] = bg_rgb[2];
                filled[idx] = true;
            }
        }
    }

    for dy in 0..roi.h {
        for dx in 0..roi.w {
            let idx = dy * roi.w + dx;
            if filled[idx] || !mask[idx] {
                continue;
            }
            let touches_boundary = [
                (dx > 0).then(|| dy * roi.w + dx - 1),
                (dx + 1 < roi.w).then(|| dy * roi.w + dx + 1),
                (dy > 0).then(|| (dy - 1) * roi.w + dx),
                (dy + 1 < roi.h).then(|| (dy + 1) * roi.w + dx),
            ]
            .into_iter()
            .flatten()
            .any(|nidx| filled[nidx]);
            if touches_boundary {
                queue.push_back((dx, dy));
                queued[idx] = true;
            }
        }
    }

    while let Some((dx, dy)) = queue.pop_front() {
        let idx = dy * roi.w + dx;
        if filled[idx] {
            continue;
        }
        let mut sum_r = 0.0_f32;
        let mut sum_g = 0.0_f32;
        let mut sum_b = 0.0_f32;
        let mut count = 0.0_f32;

        for ndy in dy.saturating_sub(1)..=(dy + 1).min(roi.h - 1) {
            for ndx in dx.saturating_sub(1)..=(dx + 1).min(roi.w - 1) {
                let nidx = ndy * roi.w + ndx;
                if filled[nidx] {
                    let w = if ndx == dx || ndy == dy { 1.0 } else { 0.707 };
                    sum_r += roi.r[nidx] * w;
                    sum_g += roi.g[nidx] * w;
                    sum_b += roi.b[nidx] * w;
                    count += w;
                } else if mask[nidx] && !queued[nidx] {
                    queued[nidx] = true;
                    queue.push_back((ndx, ndy));
                }
            }
        }

        if count > 0.0 {
            roi.r[idx] = sum_r / count;
            roi.g[idx] = sum_g / count;
            roi.b[idx] = sum_b / count;
            filled[idx] = true;
        }
    }
}

pub(super) fn diffuse_masked_region(
    mask: &[bool],
    protected_lineart: &[bool],
    uniform_bg: &[Option<[f32; 3]>],
    roi: &mut RoiBuffers,
    iterations: usize,
) {
    initialize_mask_bfs(mask, uniform_bg, roi);
    let active_coords: Vec<(usize, usize, usize)> = (0..roi.h)
        .flat_map(|dy| {
            let row_off = dy * roi.w;
            (0..roi.w).filter_map(move |dx| {
                let idx = row_off + dx;
                mask[idx].then_some((dx, dy, idx))
            })
        })
        .collect();
    if active_coords.is_empty() {
        return;
    }

    let omega = 1.45_f32;
    let offsets: [(isize, isize, f32); 8] = [
        (-1, 0, 1.0),
        (1, 0, 1.0),
        (0, -1, 1.0),
        (0, 1, 1.0),
        (-1, -1, 0.707),
        (1, -1, 0.707),
        (-1, 1, 0.707),
        (1, 1, 0.707),
    ];

    for _ in 0..iterations {
        let mut max_delta = 0.0_f32;
        for &(dx, dy, idx) in &active_coords {
            let uniform_ref = uniform_bg[idx];
            let mut sum_w = 0.0_f32;
            let mut sum_r = 0.0_f32;
            let mut sum_g = 0.0_f32;
            let mut sum_b = 0.0_f32;

            for (ox, oy, base_w) in offsets {
                let nx = dx as isize + ox;
                let ny = dy as isize + oy;
                if nx < 0 || nx >= roi.w as isize || ny < 0 || ny >= roi.h as isize {
                    continue;
                }
                let nidx = ny as usize * roi.w + nx as usize;
                let weight = if mask[nidx] {
                    base_w
                } else {
                    if let Some(bg_rgb) = uniform_ref {
                        if protected_lineart[nidx]
                            || rgb_distance(roi.rgb_at(nidx), bg_rgb) > 20.0
                        {
                            continue;
                        }
                    }
                    base_w * 1.4
                };
                sum_w += weight;
                sum_r += roi.r[nidx] * weight;
                sum_g += roi.g[nidx] * weight;
                sum_b += roi.b[nidx] * weight;
            }

            if sum_w > 0.0 {
                let avg_r = sum_r / sum_w;
                let avg_g = sum_g / sum_w;
                let avg_b = sum_b / sum_w;
                let new_r = (roi.r[idx] + omega * (avg_r - roi.r[idx])).clamp(0.0, 255.0);
                let new_g = (roi.g[idx] + omega * (avg_g - roi.g[idx])).clamp(0.0, 255.0);
                let new_b = (roi.b[idx] + omega * (avg_b - roi.b[idx])).clamp(0.0, 255.0);
                max_delta = max_delta
                    .max((new_r - roi.r[idx]).abs())
                    .max((new_g - roi.g[idx]).abs())
                    .max((new_b - roi.b[idx]).abs());
                roi.r[idx] = new_r;
                roi.g[idx] = new_g;
                roi.b[idx] = new_b;
            }
        }
        if max_delta < 1e-3 {
            break;
        }
    }
}
