use super::solver::{
    estimate_dominant_bg, line_axes, line_has_trailing_punct, rgb_distance,
    trailing_tail_extension, RoiBuffers,
};
use super::{
    point_in_anisotropic_quad, point_in_expanded_quad, ALLOW_PAD_ARTWORK, ALLOW_PAD_UNIFORM,
    RING_OUTER_PAD,
};
use crate::model::{Bubble, LineInfo};
use crate::pipe::clustering::is_interior_cleanup_line;
use std::collections::VecDeque;

const INNER_INK_PAD: f32 = 1.5;
const NEAR_RING_PAD: f32 = 4.0;
const NEAR_RING_OUTER_PAD: f32 = 4.5;
const LINEART_CONTRAST_THRESHOLD: f32 = 26.0;
const SEED_INSET_FROM_ALLOW: f32 = 1.2;

pub(super) struct LineBackground {
    pub(super) bg_rgb: [f32; 3],
    pub(super) uniform_frac: f32,
    pub(super) is_uniform_interior: bool,
    pub(super) allow_pad: f32,
}

fn channel_median(samples: &[[f32; 3]]) -> [f32; 3] {
    let mut rs: Vec<f32> = samples.iter().map(|c| c[0]).collect();
    let mut gs: Vec<f32> = samples.iter().map(|c| c[1]).collect();
    let mut bs: Vec<f32> = samples.iter().map(|c| c[2]).collect();
    rs.sort_by(|a, b| a.total_cmp(b));
    gs.sort_by(|a, b| a.total_cmp(b));
    bs.sort_by(|a, b| a.total_cmp(b));
    let mid = samples.len() / 2;
    [rs[mid], gs[mid], bs[mid]]
}

pub(super) fn rgb_luminance(rgb: [f32; 3]) -> f32 {
    0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2]
}

fn line_core_insets(line: &LineInfo) -> (f32, f32) {
    let line_w = line.true_w.max(1.0);
    let line_h = line.true_h.max(1.0);
    let inset_x = (line_w * 0.08).clamp(2.5, 6.0).min(line_w * 0.25);
    let inset_y = (line_h * 0.22).clamp(3.0, 6.5).min(line_h * 0.25);
    (inset_x, inset_y)
}

pub(super) fn line_roi_bounds(
    line: &LineInfo,
    roi: &RoiBuffers,
    pad: f32,
) -> (usize, usize, usize, usize) {
    let lx1 = ((line.x1 - pad).floor() as i32 - roi.x1 as i32).max(0) as usize;
    let ly1 = ((line.y1 - pad).floor() as i32 - roi.y1 as i32).max(0) as usize;
    let lx2 = (((line.x2 + pad).ceil() as i32 - roi.x1 as i32).max(0) as usize).min(roi.w - 1);
    let ly2 = (((line.y2 + pad).ceil() as i32 - roi.y1 as i32).max(0) as usize).min(roi.h - 1);
    (lx1, ly1, lx2, ly2)
}

pub(super) fn estimate_line_background(
    roi: &RoiBuffers,
    line: &LineInfo,
    any_allow: &[bool],
    lx1: usize,
    ly1: usize,
    lx2: usize,
    ly2: usize,
) -> Option<LineBackground> {
    let mut inner_near_samples: Vec<(u8, [f32; 3])> = Vec::new();
    let mut near_ring_samples: Vec<[f32; 3]> = Vec::new();
    let mut inner_core_lums: Vec<u8> = Vec::new();
    let mut outer_ring_samples: Vec<[f32; 3]> = Vec::new();

    for dy in ly1..=ly2 {
        let py = (roi.y1 + dy as u32) as f32 + 0.5;
        for dx in lx1..=lx2 {
            let idx = dy * roi.w + dx;
            let px = (roi.x1 + dx as u32) as f32 + 0.5;
            let rgb = roi.rgb_at(idx);

            if point_in_expanded_quad(px, py, &line.quad, NEAR_RING_OUTER_PAD) {
                let in_inner_near = point_in_expanded_quad(px, py, &line.quad, NEAR_RING_PAD);
                let in_core_ink = point_in_expanded_quad(px, py, &line.quad, INNER_INK_PAD);
                if in_inner_near {
                    inner_near_samples.push((roi.lums[idx], rgb));
                }
                if in_core_ink {
                    inner_core_lums.push(roi.lums[idx]);
                } else {
                    near_ring_samples.push(rgb);
                }
            }
            if !any_allow[idx] && point_in_expanded_quad(px, py, &line.quad, RING_OUTER_PAD) {
                outer_ring_samples.push(rgb);
            }
        }
    }

    let outer_bg = estimate_dominant_bg(&outer_ring_samples);
    let outer_uniform_frac = outer_bg.map(|(_, frac)| frac).unwrap_or(0.0);

    if inner_near_samples.len() >= 12 {
        inner_near_samples.sort_by_key(|(lum, _)| *lum);
        let top_start = (inner_near_samples.len() * 52) / 100;
        let top_rgbs: Vec<[f32; 3]> = inner_near_samples[top_start..]
            .iter()
            .map(|&(_, rgb)| rgb)
            .collect();

        if !top_rgbs.is_empty() {
            let bright_rgb = channel_median(&top_rgbs);
            let bright_lum = rgb_luminance(bright_rgb);
            let chroma = bright_rgb[0].max(bright_rgb[1]).max(bright_rgb[2])
                - bright_rgb[0].min(bright_rgb[1]).min(bright_rgb[2]);
            let matched_rgbs: Vec<[f32; 3]> = inner_near_samples
                .iter()
                .map(|&(_, rgb)| rgb)
                .filter(|&rgb| rgb_distance(rgb, bright_rgb) <= 18.0)
                .collect();
            let close_frac = matched_rgbs.len() as f32 / inner_near_samples.len() as f32;
            let dark_ink_count = inner_core_lums
                .iter()
                .filter(|&&lum| (lum as f32) < bright_lum - 40.0)
                .count();

            let is_solid_interior = if outer_uniform_frac >= 0.70 {
                let near_ring_bright_ok = if near_ring_samples.is_empty() {
                    true
                } else {
                    let near_close = near_ring_samples
                        .iter()
                        .filter(|&&rgb| rgb_distance(rgb, bright_rgb) <= 22.0)
                        .count();
                    (near_close as f32 / near_ring_samples.len() as f32) >= 0.25
                };
                bright_lum >= 215.0
                    && close_frac >= 0.38
                    && dark_ink_count >= 2
                    && near_ring_bright_ok
            } else {
                let near_ring_close_14 = if near_ring_samples.is_empty() {
                    false
                } else {
                    let count = near_ring_samples
                        .iter()
                        .filter(|&&rgb| rgb_distance(rgb, bright_rgb) <= 14.0)
                        .count();
                    (count as f32 / near_ring_samples.len() as f32) >= 0.56
                };
                let outer_ring_close_16 = if outer_ring_samples.is_empty() {
                    false
                } else {
                    let count = outer_ring_samples
                        .iter()
                        .filter(|&&rgb| rgb_distance(rgb, bright_rgb) <= 16.0)
                        .count();
                    (count as f32 / outer_ring_samples.len() as f32) >= 0.42
                };
                bright_lum >= 232.0
                    && chroma <= 18.0
                    && close_frac >= 0.38
                    && dark_ink_count >= 2
                    && near_ring_close_14
                    && outer_ring_close_16
            };

            if is_solid_interior {
                let bg_rgb = channel_median(&matched_rgbs);
                return Some(LineBackground {
                    bg_rgb,
                    uniform_frac: close_frac.max(outer_uniform_frac),
                    is_uniform_interior: true,
                    allow_pad: ALLOW_PAD_UNIFORM,
                });
            }
        }
    }

    if let Some((mode_rgb, uniform_frac)) = outer_bg {
        if uniform_frac >= 0.70 {
            return Some(LineBackground {
                bg_rgb: mode_rgb,
                uniform_frac,
                is_uniform_interior: false,
                allow_pad: ALLOW_PAD_UNIFORM,
            });
        }
        return Some(LineBackground {
            bg_rgb: mode_rgb,
            uniform_frac,
            is_uniform_interior: false,
            allow_pad: ALLOW_PAD_ARTWORK,
        });
    }

    None
}

pub(super) fn build_any_core_mask(
    roi: &RoiBuffers,
    lines: &[LineInfo],
    line_backgrounds: &[Option<LineBackground>],
) -> Vec<bool> {
    let mut any_core = vec![false; roi.w * roi.h];
    for (line, bg_opt) in lines.iter().zip(line_backgrounds.iter()) {
        if is_interior_cleanup_line(line) {
            continue;
        }
        let is_uniform = matches!(bg_opt, Some(bg) if bg.is_uniform_interior);
        let (inset_x, inset_y) = line_core_insets(line);
        let pad = if is_uniform { 0.0 } else { 1.4 };
        let (lx1, ly1, lx2, ly2) = line_roi_bounds(line, roi, pad);
        for dy in ly1..=ly2 {
            let py = (roi.y1 + dy as u32) as f32 + 0.5;
            for dx in lx1..=lx2 {
                let px = (roi.x1 + dx as u32) as f32 + 0.5;
                let in_core = if is_uniform {
                    point_in_anisotropic_quad(px, py, &line.quad, -inset_x, -inset_y)
                } else {
                    point_in_expanded_quad(px, py, &line.quad, 1.4)
                };
                if in_core {
                    any_core[dy * roi.w + dx] = true;
                }
            }
        }
    }
    any_core
}

fn is_bounded_trailing_punct_component(
    roi: &RoiBuffers,
    line: &LineInfo,
    pixels: &[usize],
) -> bool {
    if !line_has_trailing_punct(line) || pixels.is_empty() {
        return false;
    }
    let (u, v, _, u_max, v_min, v_max) = line_axes(&line.quad);
    let (inset_x, _) = line_core_insets(line);
    let tail_ext = trailing_tail_extension(line);
    let u_start = u_max - inset_x - 1.0;
    let u_end = u_max + tail_ext;
    let v_start = v_min - 2.0;
    let v_end = v_max + 2.0;

    let mut cu_min = f32::INFINITY;
    let mut cu_max = f32::NEG_INFINITY;
    let mut cv_min = f32::INFINITY;
    let mut cv_max = f32::NEG_INFINITY;

    for &idx in pixels {
        let dy = idx / roi.w;
        let dx = idx % roi.w;
        let px = (roi.x1 + dx as u32) as f32 + 0.5;
        let py = (roi.y1 + dy as u32) as f32 + 0.5;
        let pu = px * u[0] + py * u[1];
        let pv = px * v[0] + py * v[1];
        if pu < u_start || pu > u_end || pv < v_start || pv > v_end {
            return false;
        }
        if pv <= v_start + 1.5 || pv >= v_end - 1.5 || pu >= u_end - 2.0 {
            return false;
        }
        cu_min = cu_min.min(pu);
        cu_max = cu_max.max(pu);
        cv_min = cv_min.min(pv);
        cv_max = cv_max.max(pv);
    }

    let comp_w = cu_max - cu_min;
    let comp_h = cv_max - cv_min;
    comp_h <= line.true_h * 0.96 && comp_w <= line.true_h * 1.05
}

fn build_any_line_quad_mask(roi: &RoiBuffers, lines: &[LineInfo]) -> Vec<bool> {
    let mut any_line_quad = vec![false; roi.w * roi.h];
    for line in lines {
        if is_interior_cleanup_line(line) {
            continue;
        }
        let (lx1, ly1, lx2, ly2) = line_roi_bounds(line, roi, 2.0);
        for dy in ly1..=ly2 {
            let py = (roi.y1 + dy as u32) as f32 + 0.5;
            for dx in lx1..=lx2 {
                let px = (roi.x1 + dx as u32) as f32 + 0.5;
                if point_in_expanded_quad(px, py, &line.quad, 1.5) {
                    any_line_quad[dy * roi.w + dx] = true;
                }
            }
        }
    }
    any_line_quad
}

fn is_lineart_pixel(rgb: [f32; 3], bg_rgb: [f32; 3], require_dark_lineart: bool) -> bool {
    if rgb_distance(rgb, bg_rgb) <= LINEART_CONTRAST_THRESHOLD {
        return false;
    }
    if !require_dark_lineart {
        return true;
    }
    let lum = rgb_luminance(rgb);
    let bg_lum = rgb_luminance(bg_rgb);
    lum <= 205.0 && lum < bg_lum - 24.0
}

fn protect_cleanup_boundary_lineart(
    roi: &RoiBuffers,
    cleanup_line: &LineInfo,
    cleanup_bg: [f32; 3],
    any_core: &[bool],
    any_line_quad: &[bool],
    protected_lineart: &mut [bool],
) {
    let len = roi.w * roi.h;
    let (lx1, ly1, lx2, ly2) = line_roi_bounds(cleanup_line, roi, RING_OUTER_PAD);
    let mut visited = vec![false; len];

    for sy in ly1..=ly2 {
        let py = (roi.y1 + sy as u32) as f32 + 0.5;
        for sx in lx1..=lx2 {
            let sidx = sy * roi.w + sx;
            if visited[sidx] || any_core[sidx] || any_line_quad[sidx] {
                continue;
            }
            let px = (roi.x1 + sx as u32) as f32 + 0.5;
            if point_in_expanded_quad(px, py, &cleanup_line.quad, 0.5) {
                continue;
            }
            if !is_lineart_pixel(roi.rgb_at(sidx), cleanup_bg, true) {
                continue;
            }

            visited[sidx] = true;
            let mut queue = VecDeque::new();
            queue.push_back((sx, sy));

            while let Some((cx, cy)) = queue.pop_front() {
                let cidx = cy * roi.w + cx;
                protected_lineart[cidx] = true;

                for ny in cy.saturating_sub(1).max(ly1)..=(cy + 1).min(ly2) {
                    for nx in cx.saturating_sub(1).max(lx1)..=(cx + 1).min(lx2) {
                        let nidx = ny * roi.w + nx;
                        if visited[nidx] || any_core[nidx] {
                            continue;
                        }
                        if is_lineart_pixel(roi.rgb_at(nidx), cleanup_bg, true) {
                            visited[nidx] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn protect_boundary_connected_lineart(
    roi: &RoiBuffers,
    bubble: &Bubble,
    line_backgrounds: &[Option<LineBackground>],
    any_core: &[bool],
    cleanup_target: Option<(&LineInfo, [f32; 3])>,
    protected_lineart: &mut [bool],
) {
    let len = roi.w * roi.h;
    let any_line_quad = build_any_line_quad_mask(roi, &bubble.lines);

    for (line, bg_opt) in bubble.lines.iter().zip(line_backgrounds.iter()) {
        let bg = match bg_opt {
            Some(bg) => bg,
            None => continue,
        };
        let (lx1, ly1, lx2, ly2) = line_roi_bounds(line, roi, RING_OUTER_PAD);
        let (trav_x1, trav_y1, trav_x2, trav_y2) = if line_has_trailing_punct(line) {
            let tail_pad = RING_OUTER_PAD + trailing_tail_extension(line);
            line_roi_bounds(line, roi, tail_pad)
        } else {
            (lx1, ly1, lx2, ly2)
        };
        let seed_pad = (bg.allow_pad - SEED_INSET_FROM_ALLOW).max(0.0);
        let mut visited = vec![false; len];

        for sy in ly1..=ly2 {
            let py = (roi.y1 + sy as u32) as f32 + 0.5;
            for sx in lx1..=lx2 {
                let sidx = sy * roi.w + sx;
                if visited[sidx] || any_core[sidx] || any_line_quad[sidx] {
                    continue;
                }
                if !is_lineart_pixel(roi.rgb_at(sidx), bg.bg_rgb, bg.is_uniform_interior) {
                    continue;
                }
                let px = (roi.x1 + sx as u32) as f32 + 0.5;
                if point_in_expanded_quad(px, py, &line.quad, seed_pad) {
                    continue;
                }

                visited[sidx] = true;
                let mut queue = VecDeque::new();
                queue.push_back((sx, sy));
                let mut component = Vec::new();
                let mut touches_trav_edge = false;

                while let Some((cx, cy)) = queue.pop_front() {
                    let cidx = cy * roi.w + cx;
                    component.push(cidx);
                    if cx == trav_x1 || cx == trav_x2 || cy == trav_y1 || cy == trav_y2 {
                        touches_trav_edge = true;
                    }

                    for ny in cy.saturating_sub(1).max(trav_y1)..=(cy + 1).min(trav_y2) {
                        for nx in cx.saturating_sub(1).max(trav_x1)..=(cx + 1).min(trav_x2) {
                            let nidx = ny * roi.w + nx;
                            if visited[nidx] || any_core[nidx] {
                                continue;
                            }
                            if is_lineart_pixel(roi.rgb_at(nidx), bg.bg_rgb, bg.is_uniform_interior)
                            {
                                visited[nidx] = true;
                                queue.push_back((nx, ny));
                            }
                        }
                    }
                }

                if !touches_trav_edge || is_bounded_trailing_punct_component(roi, line, &component) {
                    continue;
                }
                if let Some((cleanup_line, _)) = cleanup_target {
                    let strictly_inside_cleanup = component.iter().all(|&cidx| {
                        let cy = cidx / roi.w;
                        let cx = cidx % roi.w;
                        if cx == 0 || cx + 1 >= roi.w || cy == 0 || cy + 1 >= roi.h {
                            return false;
                        }
                        let cpx = (roi.x1 + cx as u32) as f32 + 0.5;
                        let cpy = (roi.y1 + cy as u32) as f32 + 0.5;
                        point_in_expanded_quad(cpx, cpy, &cleanup_line.quad, -0.5)
                    });
                    if strictly_inside_cleanup {
                        continue;
                    }
                }
                for cidx in component {
                    protected_lineart[cidx] = true;
                }
            }
        }
    }

    if let Some((cleanup_line, cleanup_bg)) = cleanup_target {
        protect_cleanup_boundary_lineart(
            roi,
            cleanup_line,
            cleanup_bg,
            any_core,
            &any_line_quad,
            protected_lineart,
        );
    }
}
