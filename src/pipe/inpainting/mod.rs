mod guard;
mod solver;
mod tail;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_polish;

use crate::model::{Bubble, LineInfo};
use crate::pipe::clustering::is_interior_cleanup_line;
use guard::{
    build_any_core_mask, estimate_line_background, line_roi_bounds,
    protect_boundary_connected_lineart, rgb_luminance,
};
use image::{Rgb, RgbImage};
use rayon::prelude::*;
use solver::{
    absorb_contrasting_halo, absorb_contrasting_halo_with_threshold, compute_local_extrema,
    compute_row_ring_bg, diffuse_masked_region, dilate_mask, estimate_dominant_bg, rgb_distance,
    RoiBuffers,
};
use std::collections::VecDeque;
use tail::mask_trailing_punctuation_tails;

const SEED_PAD: f32 = 3.5;
const ALLOW_PAD_UNIFORM: f32 = 5.0;
const ALLOW_PAD_ARTWORK: f32 = 6.5;
const RING_OUTER_PAD: f32 = 11.5;

pub(super) fn point_in_anisotropic_quad(
    px: f32,
    py: f32,
    quad: &[[f32; 2]; 4],
    pad_x: f32,
    pad_y: f32,
) -> bool {
    let e1x = quad[1][0] - quad[0][0];
    let e1y = quad[1][1] - quad[0][1];
    let e2x = quad[2][0] - quad[0][0];
    let e2y = quad[2][1] - quad[0][1];
    let area2 = e1x * e2y - e1y * e2x;
    if area2.abs() < 1e-3 {
        return false;
    }
    let sign = if area2 > 0.0 { 1.0 } else { -1.0 };

    for i in 0..4 {
        let a = quad[i];
        let b = quad[(i + 1) % 4];
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let edge_len = dx.hypot(dy).max(1e-3);
        let depth = sign * (dx * (py - a[1]) - dy * (px - a[0])) / edge_len;
        let pad = if i % 2 == 0 { pad_y } else { pad_x };
        if depth < -pad {
            return false;
        }
    }
    true
}

fn point_in_expanded_quad(px: f32, py: f32, quad: &[[f32; 2]; 4], pad: f32) -> bool {
    point_in_anisotropic_quad(px, py, quad, pad, pad)
}

pub fn clean_image_text(img: &mut RgbImage, bubbles: &[Bubble]) {
    let (width, height) = (img.width(), img.height());
    let cleaned_rois: Vec<(RoiBuffers, Vec<bool>)> = bubbles
        .par_iter()
        .filter(|b| !b.lines.is_empty())
        .filter_map(|b| compute_cleaned_bubble(img, b, width, height))
        .collect();

    for (roi, final_mask) in cleaned_rois {
        for dy in 0..roi.h {
            let y = roi.y1 + dy as u32;
            for dx in 0..roi.w {
                let idx = dy * roi.w + dx;
                if final_mask[idx] {
                    let x = roi.x1 + dx as u32;
                    img.put_pixel(
                        x,
                        y,
                        Rgb([
                            roi.r[idx].round().clamp(0.0, 255.0) as u8,
                            roi.g[idx].round().clamp(0.0, 255.0) as u8,
                            roi.b[idx].round().clamp(0.0, 255.0) as u8,
                        ]),
                    );
                }
            }
        }
    }
}

fn add_artwork_line_masks(
    roi: &RoiBuffers,
    artwork_lines: &[(&LineInfo, usize, usize, usize, usize)],
    any_allow: &[bool],
    any_outer: &[bool],
    protected_lineart: &[bool],
    mask: &mut [bool],
    dilate_allow: &mut [bool],
) {
    let len = roi.w * roi.h;
    let (local_min, local_max) = compute_local_extrema(&roi.lums, roi.w, roi.h, 10);
    let global_ring_samples: Vec<[f32; 3]> = (0..len)
        .filter(|&i| any_outer[i] && !any_allow[i])
        .map(|i| roi.rgb_at(i))
        .collect();
    let fallback_bg = estimate_dominant_bg(&global_ring_samples)
        .map(|(rgb, _)| rgb)
        .unwrap_or([128.0, 128.0, 128.0]);
    let row_bg = compute_row_ring_bg(roi, any_allow, any_outer, fallback_bg);

    let mut artwork_mask = vec![false; len];
    let mut artwork_allow = vec![false; len];

    for &(line, lx1, ly1, lx2, ly2) in artwork_lines {
        let mut inner_lums = Vec::new();
        let mut ring_lums = Vec::new();
        for dy in ly1..=ly2 {
            let py = (roi.y1 + dy as u32) as f32 + 0.5;
            for dx in lx1..=lx2 {
                let px = (roi.x1 + dx as u32) as f32 + 0.5;
                let idx = dy * roi.w + dx;
                if !protected_lineart[idx]
                    && point_in_expanded_quad(px, py, &line.quad, ALLOW_PAD_ARTWORK)
                {
                    artwork_allow[idx] = true;
                    dilate_allow[idx] = true;
                }
                if point_in_expanded_quad(px, py, &line.quad, SEED_PAD) {
                    inner_lums.push(roi.lums[idx]);
                } else if !any_allow[idx]
                    && point_in_expanded_quad(px, py, &line.quad, RING_OUTER_PAD)
                {
                    ring_lums.push(roi.lums[idx]);
                }
            }
        }
        if inner_lums.is_empty() {
            continue;
        }
        inner_lums.sort_unstable();
        let p10 = inner_lums[inner_lums.len() / 10];
        let p50 = inner_lums[inner_lums.len() / 2];
        let p90 = inner_lums[(inner_lums.len() * 9) / 10];
        let bg_ref_lum = if ring_lums.is_empty() {
            p50
        } else {
            ring_lums.sort_unstable();
            ring_lums[ring_lums.len() / 2]
        };

        let dark_strength = bg_ref_lum.saturating_sub(p10).max(p50.saturating_sub(p10));
        let light_strength = p90.saturating_sub(bg_ref_lum).max(p90.saturating_sub(p50));
        let is_dark_text = dark_strength > light_strength;

        for dy in ly1..=ly2 {
            let py = (roi.y1 + dy as u32) as f32 + 0.5;
            for dx in lx1..=lx2 {
                let idx = dy * roi.w + dx;
                if protected_lineart[idx] {
                    continue;
                }
                let px = (roi.x1 + dx as u32) as f32 + 0.5;
                if !point_in_expanded_quad(px, py, &line.quad, SEED_PAD) {
                    continue;
                }
                let lum = roi.lums[idx];
                let is_core = if is_dark_text {
                    local_max[idx].saturating_sub(lum) > 20
                        && lum <= bg_ref_lum.saturating_sub(8).max(p50)
                } else {
                    lum.saturating_sub(local_min[idx]) > 20
                        && lum >= bg_ref_lum.saturating_add(8).min(p50)
                };
                if is_core || rgb_distance(roi.rgb_at(idx), row_bg[dy]) > 14.0 {
                    artwork_mask[idx] = true;
                }
            }
        }
    }

    absorb_contrasting_halo(&mut artwork_mask, &artwork_allow, roi, &row_bg, 7);
    for (dst, (src, prot)) in mask
        .iter_mut()
        .zip(artwork_mask.iter().zip(protected_lineart.iter()))
    {
        if *src && !*prot {
            *dst = true;
        }
    }
}

fn dilate_solid_bg(
    solid_bg: &mut [Option<[f32; 3]>],
    allow: &[bool],
    protected_lineart: &[bool],
    w: usize,
    h: usize,
    steps: usize,
) {
    for _ in 0..steps {
        let prev = solid_bg.to_vec();
        for dy in 0..h {
            for dx in 0..w {
                let rgb = match prev[dy * w + dx] {
                    Some(c) => c,
                    None => continue,
                };
                for ndy in dy.saturating_sub(1)..=(dy + 1).min(h - 1) {
                    for ndx in dx.saturating_sub(1)..=(dx + 1).min(w - 1) {
                        let nidx = ndy * w + ndx;
                        if solid_bg[nidx].is_none()
                            && allow[nidx]
                            && !protected_lineart[nidx]
                        {
                            solid_bg[nidx] = Some(rgb);
                        }
                    }
                }
            }
        }
    }
}

fn add_interior_cleanup_mask(
    roi: &RoiBuffers,
    bubble: &Bubble,
    cleanup_line: &LineInfo,
    lx1: usize,
    ly1: usize,
    lx2: usize,
    ly2: usize,
    line_backgrounds: &[Option<guard::LineBackground>],
    protected_lineart: &[bool],
    cleanup_bg: [f32; 3],
    solid_bg: &mut [Option<[f32; 3]>],
    solid_dilate_allow: &mut [bool],
) {
    if rgb_luminance(cleanup_bg) < 215.0 {
        return;
    }
    let len = roi.w * roi.h;
    let mut in_cleanup_quad = vec![false; len];
    for dy in ly1..=ly2 {
        let py = (roi.y1 + dy as u32) as f32 + 0.5;
        for dx in lx1..=lx2 {
            let idx = dy * roi.w + dx;
            if protected_lineart[idx] {
                continue;
            }
            let px = (roi.x1 + dx as u32) as f32 + 0.5;
            if point_in_expanded_quad(px, py, &cleanup_line.quad, 0.0) {
                in_cleanup_quad[idx] = true;
            }
        }
    }

    let mut cleanup_allow = vec![false; len];
    let mut queue = VecDeque::new();
    for (line, bg_opt) in bubble.lines.iter().zip(line_backgrounds.iter()) {
        if !matches!(bg_opt, Some(bg) if bg.is_uniform_interior) {
            continue;
        }
        let (rlx1, rly1, rlx2, rly2) = line_roi_bounds(line, roi, 0.0);
        for dy in rly1..=rly2 {
            let py = (roi.y1 + dy as u32) as f32 + 0.5;
            for dx in rlx1..=rlx2 {
                let idx = dy * roi.w + dx;
                if !in_cleanup_quad[idx] || cleanup_allow[idx] {
                    continue;
                }
                let px = (roi.x1 + dx as u32) as f32 + 0.5;
                if point_in_expanded_quad(px, py, &line.quad, 0.0) {
                    cleanup_allow[idx] = true;
                    queue.push_back((dx, dy));
                }
            }
        }
    }

    while let Some((cx, cy)) = queue.pop_front() {
        let neighbors = [
            (cx > lx1).then(|| (cx - 1, cy)),
            (cx < lx2).then(|| (cx + 1, cy)),
            (cy > ly1).then(|| (cx, cy - 1)),
            (cy < ly2).then(|| (cx, cy + 1)),
        ];
        for (nx, ny) in neighbors.into_iter().flatten() {
            let nidx = ny * roi.w + nx;
            if in_cleanup_quad[nidx] && !cleanup_allow[nidx] {
                cleanup_allow[nidx] = true;
                queue.push_back((nx, ny));
            }
        }
    }

    let mut line_mask = vec![false; len];
    for idx in 0..len {
        if cleanup_allow[idx] {
            solid_dilate_allow[idx] = true;
            if rgb_distance(roi.rgb_at(idx), cleanup_bg) > 10.0 {
                line_mask[idx] = true;
            }
        }
    }
    let mode_row_bg = vec![cleanup_bg; roi.h];
    absorb_contrasting_halo_with_threshold(&mut line_mask, &cleanup_allow, roi, &mode_row_bg, 7.0, 10);
    for (idx, &m) in line_mask.iter().enumerate() {
        if m && !protected_lineart[idx] && solid_bg[idx].is_none() {
            solid_bg[idx] = Some(cleanup_bg);
        }
    }
}

fn compute_cleaned_bubble(
    img: &RgbImage,
    bubble: &Bubble,
    img_w: u32,
    img_h: u32,
) -> Option<(RoiBuffers, Vec<bool>)> {
    let mut roi = RoiBuffers::from_image(img, bubble, img_w, img_h)?;

    let len = roi.w * roi.h;
    let mut any_allow = vec![false; len];
    let mut any_outer = vec![false; len];

    let mut line_bounds = Vec::with_capacity(bubble.lines.len());
    for line in &bubble.lines {
        let bounds = line_roi_bounds(line, &roi, RING_OUTER_PAD);
        if !is_interior_cleanup_line(line) {
            let (lx1, ly1, lx2, ly2) = bounds;
            for dy in ly1..=ly2 {
                let py = (roi.y1 + dy as u32) as f32 + 0.5;
                for dx in lx1..=lx2 {
                    let px = (roi.x1 + dx as u32) as f32 + 0.5;
                    let idx = dy * roi.w + dx;
                    if !any_allow[idx]
                        && point_in_expanded_quad(px, py, &line.quad, ALLOW_PAD_ARTWORK)
                    {
                        any_allow[idx] = true;
                        any_outer[idx] = true;
                    } else if !any_outer[idx]
                        && point_in_expanded_quad(px, py, &line.quad, RING_OUTER_PAD)
                    {
                        any_outer[idx] = true;
                    }
                }
            }
        }
        line_bounds.push(bounds);
    }

    let mut line_backgrounds = Vec::with_capacity(bubble.lines.len());
    for (line, &bounds) in bubble.lines.iter().zip(line_bounds.iter()) {
        let bg = if is_interior_cleanup_line(line) {
            None
        } else {
            estimate_line_background(
                &roi, line, &any_allow, bounds.0, bounds.1, bounds.2, bounds.3,
            )
        };
        line_backgrounds.push(bg);
    }
    let any_core = build_any_core_mask(&roi, &bubble.lines, &line_backgrounds);

    let uniform_bgs: Vec<[f32; 3]> = line_backgrounds
        .iter()
        .filter_map(|opt| {
            opt.as_ref()
                .filter(|bg| bg.is_uniform_interior)
                .map(|bg| bg.bg_rgb)
        })
        .collect();
    let has_artwork_real = line_backgrounds.iter().any(|opt| {
        matches!(opt, Some(bg) if !bg.is_uniform_interior && bg.uniform_frac < 0.70)
    });
    let allow_cleanup = !uniform_bgs.is_empty() && !has_artwork_real;
    let cleanup_bg = if allow_cleanup {
        let count = uniform_bgs.len() as f32;
        Some([
            uniform_bgs.iter().map(|c| c[0]).sum::<f32>() / count,
            uniform_bgs.iter().map(|c| c[1]).sum::<f32>() / count,
            uniform_bgs.iter().map(|c| c[2]).sum::<f32>() / count,
        ])
    } else {
        None
    };
    let cleanup_target = cleanup_bg.and_then(|cbg| {
        bubble
            .lines
            .iter()
            .find(|l| is_interior_cleanup_line(l))
            .map(|l| (l, cbg))
    });

    let mut protected_lineart = vec![false; len];
    protect_boundary_connected_lineart(
        &roi,
        bubble,
        &line_backgrounds,
        &any_core,
        cleanup_target,
        &mut protected_lineart,
    );

    let mut diffuse_mask = vec![false; len];
    let mut diffuse_dilate_allow = vec![false; len];
    let mut solid_bg: Vec<Option<[f32; 3]>> = vec![None; len];
    let mut solid_dilate_allow = vec![false; len];
    let mut line_solid_bg: Vec<Option<[f32; 3]>> = vec![None; bubble.lines.len()];
    let mut artwork_lines = Vec::new();
    let mut cleanup_lines = Vec::new();

    for (line_idx, ((line, &(lx1, ly1, lx2, ly2)), bg_opt)) in bubble
        .lines
        .iter()
        .zip(line_bounds.iter())
        .zip(line_backgrounds.iter())
        .enumerate()
    {
        if is_interior_cleanup_line(line) {
            if allow_cleanup {
                cleanup_lines.push((line, lx1, ly1, lx2, ly2));
            }
            continue;
        }
        if let Some(bg) = bg_opt {
            if bg.is_uniform_interior {
                let mut line_mask = vec![false; len];
                let mut line_allow = vec![false; len];
                for dy in ly1..=ly2 {
                    let py = (roi.y1 + dy as u32) as f32 + 0.5;
                    for dx in lx1..=lx2 {
                        let idx = dy * roi.w + dx;
                        if protected_lineart[idx] {
                            continue;
                        }
                        let px = (roi.x1 + dx as u32) as f32 + 0.5;
                        if point_in_expanded_quad(px, py, &line.quad, ALLOW_PAD_UNIFORM) {
                            line_allow[idx] = true;
                            solid_dilate_allow[idx] = true;
                            if rgb_distance(roi.rgb_at(idx), bg.bg_rgb) > 10.0 {
                                line_mask[idx] = true;
                            }
                        }
                    }
                }
                let mode_row_bg = vec![bg.bg_rgb; roi.h];
                absorb_contrasting_halo_with_threshold(
                    &mut line_mask,
                    &line_allow,
                    &roi,
                    &mode_row_bg,
                    7.0,
                    10,
                );
                for (idx, &m) in line_mask.iter().enumerate() {
                    if m && !protected_lineart[idx] {
                        solid_bg[idx] = Some(bg.bg_rgb);
                    }
                }
                line_solid_bg[line_idx] = Some(bg.bg_rgb);
                continue;
            }

            if bg.uniform_frac >= 0.70 {
                let mut line_mask = vec![false; len];
                let mut line_allow = vec![false; len];
                for dy in ly1..=ly2 {
                    let py = (roi.y1 + dy as u32) as f32 + 0.5;
                    for dx in lx1..=lx2 {
                        let idx = dy * roi.w + dx;
                        if protected_lineart[idx] {
                            continue;
                        }
                        let px = (roi.x1 + dx as u32) as f32 + 0.5;
                        if point_in_expanded_quad(px, py, &line.quad, ALLOW_PAD_UNIFORM) {
                            line_allow[idx] = true;
                            diffuse_dilate_allow[idx] = true;
                        }
                        if point_in_expanded_quad(px, py, &line.quad, SEED_PAD)
                            && rgb_distance(roi.rgb_at(idx), bg.bg_rgb) > 9.0
                        {
                            line_mask[idx] = true;
                        }
                    }
                }
                let mode_row_bg = vec![bg.bg_rgb; roi.h];
                absorb_contrasting_halo_with_threshold(
                    &mut line_mask,
                    &line_allow,
                    &roi,
                    &mode_row_bg,
                    7.0,
                    10,
                );
                for (dst, (src, prot)) in diffuse_mask
                    .iter_mut()
                    .zip(line_mask.iter().zip(protected_lineart.iter()))
                {
                    if *src && !*prot {
                        *dst = true;
                    }
                }
                continue;
            }
        }

        artwork_lines.push((line, lx1, ly1, lx2, ly2));
    }

    if let Some(cbg) = cleanup_bg {
        for (line, lx1, ly1, lx2, ly2) in cleanup_lines {
            add_interior_cleanup_mask(
                &roi,
                bubble,
                line,
                lx1,
                ly1,
                lx2,
                ly2,
                &line_backgrounds,
                &protected_lineart,
                cbg,
                &mut solid_bg,
                &mut solid_dilate_allow,
            );
        }
    }

    if !artwork_lines.is_empty() {
        add_artwork_line_masks(
            &roi,
            &artwork_lines,
            &any_allow,
            &any_outer,
            &protected_lineart,
            &mut diffuse_mask,
            &mut diffuse_dilate_allow,
        );
    }

    let combined_mask: Vec<bool> = (0..len)
        .map(|i| diffuse_mask[i] || solid_bg[i].is_some())
        .collect();
    mask_trailing_punctuation_tails(
        bubble,
        &roi,
        &any_allow,
        &any_outer,
        &mut protected_lineart,
        &line_solid_bg,
        &combined_mask,
        &mut diffuse_mask,
        &mut diffuse_dilate_allow,
        &mut solid_bg,
        &mut solid_dilate_allow,
    );

    dilate_mask(&mut diffuse_mask, &diffuse_dilate_allow, roi.w, roi.h, 3);
    dilate_solid_bg(&mut solid_bg, &solid_dilate_allow, &protected_lineart, roi.w, roi.h, 2);

    let mut final_mask = vec![false; len];
    for idx in 0..len {
        if !protected_lineart[idx] && (diffuse_mask[idx] || solid_bg[idx].is_some()) {
            final_mask[idx] = true;
        }
    }

    if final_mask.iter().any(|&m| m) {
        diffuse_masked_region(&final_mask, &protected_lineart, &solid_bg, &mut roi, 90);
        Some((roi, final_mask))
    } else {
        None
    }
}
