use super::point_in_expanded_quad;
use super::solver::{
    absorb_contrasting_halo, compute_row_ring_bg, estimate_dominant_bg, line_axes,
    line_has_trailing_punct, rgb_distance, trailing_tail_extension, RoiBuffers,
};
use super::SEED_PAD;
use crate::model::{Bubble, LineInfo};
use std::collections::VecDeque;

const INK_BINS: usize = 8;

struct TailComponent {
    pixels: Vec<usize>,
    u_min: f32,
    u_max: f32,
}

struct LineInkProfile {
    v_min: f32,
    v_span: f32,
    bg_rgb: [f32; 3],
    bins: [[f32; 3]; INK_BINS],
}

impl LineInkProfile {
    fn ink_at(&self, pv: f32) -> [f32; 3] {
        let t = ((pv - self.v_min) / self.v_span.max(1.0)).clamp(0.0, 0.9999);
        let idx = (t * INK_BINS as f32) as usize;
        self.bins[idx.min(INK_BINS - 1)]
    }
}

fn median_ink_from_samples(samples: &[(u8, [f32; 3])], is_dark_ink: bool) -> [f32; 3] {
    let mut sorted = samples.to_vec();
    sorted.sort_by_key(|(lum, _)| *lum);
    let slice_len = (sorted.len() * 30 / 100).max(2).min(sorted.len());
    let ink_slice = if is_dark_ink {
        &sorted[..slice_len]
    } else {
        &sorted[sorted.len() - slice_len..]
    };

    let mut rs: Vec<f32> = ink_slice.iter().map(|(_, c)| c[0]).collect();
    let mut gs: Vec<f32> = ink_slice.iter().map(|(_, c)| c[1]).collect();
    let mut bs: Vec<f32> = ink_slice.iter().map(|(_, c)| c[2]).collect();
    rs.sort_by(|a, b| a.total_cmp(b));
    gs.sort_by(|a, b| a.total_cmp(b));
    bs.sort_by(|a, b| a.total_cmp(b));
    let mid = slice_len / 2;
    [rs[mid], gs[mid], bs[mid]]
}

fn estimate_line_ink_profile(
    roi: &RoiBuffers,
    line: &LineInfo,
    mask: &[bool],
    any_allow: &[bool],
    any_outer: &[bool],
    v: [f32; 2],
    v_min: f32,
    v_max: f32,
) -> Option<LineInkProfile> {
    let mut masked_samples: Vec<(u8, f32, [f32; 3])> = Vec::new();
    let mut ring_samples: Vec<(u8, [f32; 3])> = Vec::new();

    for dy in 0..roi.h {
        let py = (roi.y1 + dy as u32) as f32 + 0.5;
        for dx in 0..roi.w {
            let idx = dy * roi.w + dx;
            let px = (roi.x1 + dx as u32) as f32 + 0.5;
            if mask[idx] && point_in_expanded_quad(px, py, &line.quad, SEED_PAD) {
                let pv = px * v[0] + py * v[1];
                masked_samples.push((roi.lums[idx], pv, roi.rgb_at(idx)));
            } else if any_outer[idx] && !any_allow[idx] {
                ring_samples.push((roi.lums[idx], roi.rgb_at(idx)));
            }
        }
    }

    if masked_samples.len() < 8 {
        return None;
    }

    masked_samples.sort_by_key(|(lum, _, _)| *lum);
    let bg_rgb = if ring_samples.is_empty() {
        masked_samples[masked_samples.len() / 2].2
    } else {
        ring_samples.sort_by_key(|(lum, _)| *lum);
        ring_samples[ring_samples.len() / 2].1
    };
    let bg_lum = (0.299 * bg_rgb[0] + 0.587 * bg_rgb[1] + 0.114 * bg_rgb[2]) as u8;

    let p15_lum = masked_samples[masked_samples.len() * 15 / 100].0;
    let p85_lum = masked_samples[masked_samples.len() * 85 / 100].0;
    let is_dark_ink = bg_lum.saturating_sub(p15_lum) >= p85_lum.saturating_sub(bg_lum);

    let flat_samples: Vec<(u8, [f32; 3])> = masked_samples
        .iter()
        .map(|(lum, _, rgb)| (*lum, *rgb))
        .collect();
    let global_ink = median_ink_from_samples(&flat_samples, is_dark_ink);
    if rgb_distance(global_ink, bg_rgb) <= 22.0 {
        return None;
    }

    let v_span = (v_max - v_min).max(1.0);
    let mut bin_samples: [Vec<(u8, [f32; 3])>; INK_BINS] = Default::default();
    for &(lum, pv, rgb) in &masked_samples {
        let t = ((pv - v_min) / v_span).clamp(0.0, 0.9999);
        let b = ((t * INK_BINS as f32) as usize).min(INK_BINS - 1);
        bin_samples[b].push((lum, rgb));
    }

    let mut bins = [global_ink; INK_BINS];
    let mut populated = [false; INK_BINS];
    for b in 0..INK_BINS {
        if bin_samples[b].len() >= 4 {
            let candidate = median_ink_from_samples(&bin_samples[b], is_dark_ink);
            if rgb_distance(candidate, bg_rgb) > 22.0 {
                bins[b] = candidate;
                populated[b] = true;
            }
        }
    }
    for b in 0..INK_BINS {
        if !populated[b] {
            if let Some(nearest) = (0..INK_BINS)
                .filter(|&k| populated[k])
                .min_by_key(|&k| k.abs_diff(b))
            {
                bins[b] = bins[nearest];
            }
        }
    }

    Some(LineInkProfile {
        v_min,
        v_span,
        bg_rgb,
        bins,
    })
}

fn collect_tail_components(
    roi: &RoiBuffers,
    line: &LineInfo,
    profile: &LineInkProfile,
    mask: &[bool],
    u: [f32; 2],
    v: [f32; 2],
    u_start: f32,
    u_end: f32,
    v_start: f32,
    v_end: f32,
) -> Vec<TailComponent> {
    let len = roi.w * roi.h;
    let mut cand = vec![false; len];
    for dy in 0..roi.h {
        let py = (roi.y1 + dy as u32) as f32 + 0.5;
        for dx in 0..roi.w {
            let idx = dy * roi.w + dx;
            if mask[idx] {
                continue;
            }
            let px = (roi.x1 + dx as u32) as f32 + 0.5;
            let pu = px * u[0] + py * u[1];
            let pv = px * v[0] + py * v[1];
            let rgb = roi.rgb_at(idx);
            if pu >= u_start
                && pu <= u_end
                && pv >= v_start
                && pv <= v_end
                && rgb_distance(rgb, profile.bg_rgb) > 28.0
                && rgb_distance(rgb, profile.ink_at(pv)) <= 64.0
            {
                cand[idx] = true;
            }
        }
    }

    let mut visited = vec![false; len];
    let mut components = Vec::new();
    for start_y in 0..roi.h {
        for start_x in 0..roi.w {
            let start_idx = start_y * roi.w + start_x;
            if !cand[start_idx] || visited[start_idx] {
                continue;
            }
            visited[start_idx] = true;
            let mut queue = VecDeque::new();
            queue.push_back((start_x, start_y));
            let mut pixels = Vec::new();
            let mut cu_min = f32::INFINITY;
            let mut cu_max = f32::NEG_INFINITY;
            let mut cv_min = f32::INFINITY;
            let mut cv_max = f32::NEG_INFINITY;
            let mut touches_edge = false;

            while let Some((cx, cy)) = queue.pop_front() {
                let cidx = cy * roi.w + cx;
                pixels.push(cidx);
                let px = (roi.x1 + cx as u32) as f32 + 0.5;
                let py = (roi.y1 + cy as u32) as f32 + 0.5;
                let pu = px * u[0] + py * u[1];
                let pv = px * v[0] + py * v[1];
                cu_min = cu_min.min(pu);
                cu_max = cu_max.max(pu);
                cv_min = cv_min.min(pv);
                cv_max = cv_max.max(pv);
                if pv <= v_start + 1.5 || pv >= v_end - 1.5 || pu >= u_end - 2.0 {
                    touches_edge = true;
                }

                for ny in cy.saturating_sub(1)..=(cy + 1).min(roi.h - 1) {
                    for nx in cx.saturating_sub(1)..=(cx + 1).min(roi.w - 1) {
                        let nidx = ny * roi.w + nx;
                        if cand[nidx] && !visited[nidx] {
                            visited[nidx] = true;
                            queue.push_back((nx, ny));
                        }
                    }
                }
            }

            let comp_w = cu_max - cu_min;
            let comp_h = cv_max - cv_min;
            if !touches_edge
                && pixels.len() >= 5
                && comp_h <= line.true_h * 0.96
                && comp_w <= line.true_h * 1.05
            {
                components.push(TailComponent {
                    pixels,
                    u_min: cu_min,
                    u_max: cu_max,
                });
            }
        }
    }

    components.sort_by(|a, b| a.u_min.total_cmp(&b.u_min));
    components
}

fn mark_tail_allow_band(
    roi: &RoiBuffers,
    protected_lineart: &[bool],
    u: [f32; 2],
    v: [f32; 2],
    allow_u1: f32,
    allow_u2: f32,
    allow_v1: f32,
    allow_v2: f32,
    tail_allow: &mut [bool],
    dilate_allow: &mut [bool],
) {
    for dy in 0..roi.h {
        let py = (roi.y1 + dy as u32) as f32 + 0.5;
        for dx in 0..roi.w {
            let idx = dy * roi.w + dx;
            if protected_lineart[idx] {
                continue;
            }
            let px = (roi.x1 + dx as u32) as f32 + 0.5;
            let pu = px * u[0] + py * u[1];
            let pv = px * v[0] + py * v[1];
            if pu >= allow_u1 && pu <= allow_u2 && pv >= allow_v1 && pv <= allow_v2 {
                tail_allow[idx] = true;
                dilate_allow[idx] = true;
            }
        }
    }
}

fn clear_protection_near_tail_component(
    roi: &RoiBuffers,
    pixels: &[usize],
    protected_lineart: &mut [bool],
) {
    for &idx in pixels {
        let dy = idx / roi.w;
        let dx = idx % roi.w;
        for ny in dy.saturating_sub(1)..=(dy + 1).min(roi.h - 1) {
            for nx in dx.saturating_sub(1)..=(dx + 1).min(roi.w - 1) {
                protected_lineart[ny * roi.w + nx] = false;
            }
        }
    }
}

fn collect_vertical_bottom_tail_pixels(
    roi: &RoiBuffers,
    line: &LineInfo,
    bg_rgb: [f32; 3],
    combined_mask: &[bool],
) -> Vec<usize> {
    let x_min_f = line.x1 - 4.0;
    let x_max_f = line.x2 + 4.0;
    let y_min_f = line.y2 - 2.0;
    let y_max_f = line.y2 + line.w * 1.15;

    let sx1 = ((x_min_f.floor() as i32 - roi.x1 as i32).max(0) as usize).min(roi.w - 1);
    let sy1 = ((y_min_f.floor() as i32 - roi.y1 as i32).max(0) as usize).min(roi.h - 1);
    let sx2 = (((x_max_f.ceil() as i32 - roi.x1 as i32).max(0) as usize)).min(roi.w - 1);
    let sy2 = (((y_max_f.ceil() as i32 - roi.y1 as i32).max(0) as usize)).min(roi.h - 1);
    if sx2 <= sx1 + 3 || sy2 <= sy1 + 3 {
        return Vec::new();
    }

    let len = roi.w * roi.h;
    let mut visited = vec![false; len];
    let mut accepted = Vec::new();

    for start_y in sy1..=sy2 {
        for start_x in sx1..=sx2 {
            let sidx = start_y * roi.w + start_x;
            if visited[sidx] || combined_mask[sidx] {
                continue;
            }
            if rgb_distance(roi.rgb_at(sidx), bg_rgb) <= 28.0 {
                continue;
            }

            visited[sidx] = true;
            let mut queue = VecDeque::new();
            queue.push_back((start_x, start_y));
            let mut pixels = Vec::new();
            let mut min_x = start_x;
            let mut max_x = start_x;
            let mut min_y = start_y;
            let mut max_y = start_y;
            let mut touches_boundary = false;

            while let Some((cx, cy)) = queue.pop_front() {
                let cidx = cy * roi.w + cx;
                pixels.push(cidx);
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);

                if cx <= sx1 + 1 || cx + 1 >= sx2 || cy + 1 >= sy2 {
                    touches_boundary = true;
                }

                for ny in cy.saturating_sub(1).max(sy1)..=(cy + 1).min(sy2) {
                    for nx in cx.saturating_sub(1).max(sx1)..=(cx + 1).min(sx2) {
                        let nidx = ny * roi.w + nx;
                        if visited[nidx]
                            || combined_mask[nidx]
                            || rgb_distance(roi.rgb_at(nidx), bg_rgb) <= 28.0
                        {
                            continue;
                        }
                        visited[nidx] = true;
                        queue.push_back((nx, ny));
                    }
                }
            }

            let comp_w = (max_x - min_x + 1) as f32;
            let comp_h = (max_y - min_y + 1) as f32;
            if !touches_boundary
                && pixels.len() >= 4
                && comp_w <= line.w * 0.85
                && comp_h <= line.w * 1.05
            {
                accepted.extend(pixels);
            }
        }
    }

    accepted
}

pub(super) fn mask_trailing_punctuation_tails(
    bubble: &Bubble,
    roi: &RoiBuffers,
    any_allow: &[bool],
    any_outer: &[bool],
    protected_lineart: &mut [bool],
    line_solid_bg: &[Option<[f32; 3]>],
    combined_mask: &[bool],
    diffuse_mask: &mut [bool],
    diffuse_dilate_allow: &mut [bool],
    solid_bg: &mut [Option<[f32; 3]>],
    solid_dilate_allow: &mut [bool],
) {
    let len = roi.w * roi.h;
    let mut diffuse_tail_mask = vec![false; len];
    let mut diffuse_tail_allow = vec![false; len];
    let mut any_diffuse_accepted = false;

    let global_ring: Vec<[f32; 3]> = (0..len)
        .filter(|&i| any_outer[i] && !any_allow[i])
        .map(|i| roi.rgb_at(i))
        .collect();
    let fallback_bg = estimate_dominant_bg(&global_ring)
        .map(|(rgb, _)| rgb)
        .unwrap_or([245.0, 245.0, 245.0]);
    let row_bg = compute_row_ring_bg(roi, any_allow, any_outer, fallback_bg);

    for (line_idx, line) in bubble.lines.iter().enumerate() {
        let line_solid = line_solid_bg.get(line_idx).copied().flatten();
        let is_vertical_line = line.h > line.w * 1.35 || line.true_h > line.true_w * 1.35;

        if is_vertical_line {
            let bg_ref = line_solid.unwrap_or_else(|| {
                let ry = ((line.y2 as i32 - roi.y1 as i32).clamp(0, roi.h as i32 - 1)) as usize;
                row_bg[ry]
            });
            let v_pixels = collect_vertical_bottom_tail_pixels(roi, line, bg_ref, combined_mask);
            if !v_pixels.is_empty() {
                clear_protection_near_tail_component(roi, &v_pixels, protected_lineart);
                for &idx in &v_pixels {
                    let dy = idx / roi.w;
                    let dx = idx % roi.w;
                    for ny in dy.saturating_sub(1)..=(dy + 1).min(roi.h - 1) {
                        for nx in dx.saturating_sub(1)..=(dx + 1).min(roi.w - 1) {
                            let nidx = ny * roi.w + nx;
                            if protected_lineart[nidx] {
                                continue;
                            }
                            if let Some(bg_rgb) = line_solid {
                                solid_bg[nidx] = Some(bg_rgb);
                                solid_dilate_allow[nidx] = true;
                            } else {
                                diffuse_mask[nidx] = true;
                                diffuse_dilate_allow[nidx] = true;
                            }
                        }
                    }
                }
            }
        }

        if !line_has_trailing_punct(line) {
            continue;
        }
        let (u, v, _, u_max, v_min, v_max) = line_axes(&line.quad);
        let profile = match estimate_line_ink_profile(
            roi,
            line,
            combined_mask,
            any_allow,
            any_outer,
            v,
            v_min,
            v_max,
        ) {
            Some(p) => p,
            None => continue,
        };

        let tail_ext = trailing_tail_extension(line);
        let u_start = u_max - 2.0;
        let u_end = u_max + tail_ext;
        let v_start = v_min - 2.0;
        let v_end = v_max + 2.0;

        let components = collect_tail_components(
            roi,
            line,
            &profile,
            combined_mask,
            u,
            v,
            u_start,
            u_end,
            v_start,
            v_end,
        );

        let mut frontier = u_max + line.true_h * 0.65;
        let mut solid_line_tail_mask = if line_solid.is_some() {
            vec![false; len]
        } else {
            Vec::new()
        };
        let mut solid_line_tail_allow = if line_solid.is_some() {
            vec![false; len]
        } else {
            Vec::new()
        };
        let mut line_solid_accepted = false;

        for comp in components {
            if comp.u_min > frontier {
                continue;
            }
            frontier = frontier.max(comp.u_max + line.true_h * 0.65);
            clear_protection_near_tail_component(roi, &comp.pixels, protected_lineart);

            let mut comp_v_min = f32::INFINITY;
            let mut comp_v_max = f32::NEG_INFINITY;
            for &idx in &comp.pixels {
                if line_solid.is_some() {
                    solid_line_tail_mask[idx] = true;
                    line_solid_accepted = true;
                } else {
                    diffuse_tail_mask[idx] = true;
                    diffuse_mask[idx] = true;
                    any_diffuse_accepted = true;
                }
                let dy = idx / roi.w;
                let dx = idx % roi.w;
                let px = (roi.x1 + dx as u32) as f32 + 0.5;
                let py = (roi.y1 + dy as u32) as f32 + 0.5;
                let pv = px * v[0] + py * v[1];
                comp_v_min = comp_v_min.min(pv);
                comp_v_max = comp_v_max.max(pv);
            }

            if !comp_v_min.is_finite() || !comp_v_max.is_finite() {
                continue;
            }
            let allow_u1 = (comp.u_min - 4.0).max(u_start);
            let allow_u2 = (comp.u_max + 4.0).min(u_end + 2.0);
            let allow_v1 = (comp_v_min - 4.0).max(v_start - 2.0);
            let allow_v2 = (comp_v_max + 4.0).min(v_end + 2.0);

            if line_solid.is_some() {
                mark_tail_allow_band(
                    roi,
                    protected_lineart,
                    u,
                    v,
                    allow_u1,
                    allow_u2,
                    allow_v1,
                    allow_v2,
                    &mut solid_line_tail_allow,
                    solid_dilate_allow,
                );
            } else {
                mark_tail_allow_band(
                    roi,
                    protected_lineart,
                    u,
                    v,
                    allow_u1,
                    allow_u2,
                    allow_v1,
                    allow_v2,
                    &mut diffuse_tail_allow,
                    diffuse_dilate_allow,
                );
            }
        }

        if let Some(bg_rgb) = line_solid {
            if line_solid_accepted {
                let solid_row_bg = vec![bg_rgb; roi.h];
                absorb_contrasting_halo(
                    &mut solid_line_tail_mask,
                    &solid_line_tail_allow,
                    roi,
                    &solid_row_bg,
                    4,
                );
                for (idx, &m) in solid_line_tail_mask.iter().enumerate() {
                    if m && !protected_lineart[idx] {
                        solid_bg[idx] = Some(bg_rgb);
                    }
                }
            }
        }
    }

    if any_diffuse_accepted {
        absorb_contrasting_halo(&mut diffuse_tail_mask, &diffuse_tail_allow, roi, &row_bg, 4);
        for (dst, (src, prot)) in diffuse_mask
            .iter_mut()
            .zip(diffuse_tail_mask.iter().zip(protected_lineart.iter()))
        {
            if *src && !*prot {
                *dst = true;
            }
        }
    }
}
