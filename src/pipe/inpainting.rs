use crate::model::Bubble;
use image::{Rgb, RgbImage};

fn point_in_expanded_quad(px: f32, py: f32, quad: &[[f32; 2]; 4], pad: f32) -> bool {
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
        if depth < -pad {
            return false;
        }
    }
    true
}

fn pixel_luminance(p: &Rgb<u8>) -> u8 {
    (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8
}

pub fn clean_image_text(img: &mut RgbImage, bubbles: &[Bubble]) {
    let (width, height) = (img.width(), img.height());

    for b in bubbles {
        let pad = 3.0_f32;
        let bx1 = (b.x1 - pad).max(0.0) as u32;
        let by1 = (b.y1 - pad).max(0.0) as u32;
        let bx2 = (b.x2 + pad).min(width as f32 - 1.0) as u32;
        let by2 = (b.y2 + pad).min(height as f32 - 1.0) as u32;

        if bx2 <= bx1 || by2 <= by1 {
            continue;
        }

        let mut sampled_pixels = 0usize;
        let mut light_count = 0usize;
        let mut dark_count = 0usize;
        let mut light_lums = Vec::new();

        for y in by1..=by2 {
            let py = y as f32 + 0.5;
            for x in bx1..=bx2 {
                let px = x as f32 + 0.5;
                let inside_quad = b.lines.is_empty()
                    || b.lines
                        .iter()
                        .any(|line| point_in_expanded_quad(px, py, &line.quad, pad));
                if !inside_quad {
                    continue;
                }

                sampled_pixels += 1;
                let lum = pixel_luminance(img.get_pixel(x, y));
                if lum >= 180 {
                    light_count += 1;
                    light_lums.push(lum);
                } else if lum < 80 {
                    dark_count += 1;
                }
            }
        }

        let is_white_bubble = if sampled_pixels > 0 && !light_lums.is_empty() {
            let total = sampled_pixels as f32;
            let light_fraction = light_count as f32 / total;
            let dark_fraction = dark_count as f32 / total;
            light_lums.sort_unstable();
            let p10_light = light_lums[light_lums.len() / 10];
            light_fraction >= 0.70 && dark_fraction <= 0.22 && p10_light >= 235
        } else {
            false
        };

        if is_white_bubble {
            let bg_color = Rgb([255, 255, 255]);
            for line in &b.lines {
                let lx1 = (line.x1 - 3.0).max(0.0) as u32;
                let ly1 = (line.y1 - 3.0).max(0.0) as u32;
                let lx2 = (line.x2 + 3.0).min(width as f32 - 1.0) as u32;
                let ly2 = (line.y2 + 3.0).min(height as f32 - 1.0) as u32;

                if lx2 <= lx1 || ly2 <= ly1 {
                    continue;
                }
                clean_bubble_line(img, &line.quad, lx1, ly1, lx2, ly2, bg_color);
            }
        } else {
            inpaint_artwork_bubble(img, b, width, height);
        }
    }
}

fn clean_bubble_line(
    img: &mut RgbImage,
    quad: &[[f32; 2]; 4],
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    bg_color: Rgb<u8>,
) {
    let box_w = (x2 - x1 + 1) as usize;
    let box_h = (y2 - y1 + 1) as usize;

    let mut lums = vec![0u8; box_w * box_h];
    let mut is_ink = vec![false; box_w * box_h];
    let mut inside_quad = vec![false; box_w * box_h];

    for dy in 0..box_h {
        let y = y1 + dy as u32;
        let py = y as f32 + 0.5;
        for dx in 0..box_w {
            let x = x1 + dx as u32;
            let px = x as f32 + 0.5;
            let idx = dy * box_w + dx;
            let in_q = point_in_expanded_quad(px, py, quad, 3.0);
            inside_quad[idx] = in_q;
            let lum = pixel_luminance(img.get_pixel(x, y));
            lums[idx] = lum;
            if in_q && lum < 175 {
                is_ink[idx] = true;
            }
        }
    }

    let mut dilated = is_ink.clone();
    for _ in 0..2 {
        let mut next = dilated.clone();
        for dy in 0..box_h {
            for dx in 0..box_w {
                if !dilated[dy * box_w + dx] {
                    continue;
                }
                for ndy in dy.saturating_sub(1)..=(dy + 1).min(box_h - 1) {
                    for ndx in dx.saturating_sub(1)..=(dx + 1).min(box_w - 1) {
                        let nidx = ndy * box_w + ndx;
                        if inside_quad[nidx] && (lums[nidx] >= 135 || is_ink[nidx]) {
                            next[nidx] = true;
                        }
                    }
                }
            }
        }
        dilated = next;
    }

    for dy in 0..box_h {
        let y = y1 + dy as u32;
        for dx in 0..box_w {
            if dilated[dy * box_w + dx] {
                let x = x1 + dx as u32;
                img.put_pixel(x, y, bg_color);
            }
        }
    }
}

fn compute_local_extrema(lums: &[u8], box_w: usize, box_h: usize, radius: usize) -> (Vec<u8>, Vec<u8>) {
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

fn inpaint_artwork_bubble(img: &mut RgbImage, bubble: &Bubble, img_w: u32, img_h: u32) {
    let pad = 12.0_f32;
    let x1 = (bubble.x1 - pad).max(0.0) as u32;
    let y1 = (bubble.y1 - pad).max(0.0) as u32;
    let x2 = (bubble.x2 + pad).min(img_w as f32 - 1.0) as u32;
    let y2 = (bubble.y2 + pad).min(img_h as f32 - 1.0) as u32;

    let box_w = (x2 - x1 + 1) as usize;
    let box_h = (y2 - y1 + 1) as usize;
    if box_w <= 4 || box_h <= 4 {
        return;
    }

    let mut lums = vec![0u8; box_w * box_h];
    let mut r_buf = vec![0.0_f32; box_w * box_h];
    let mut g_buf = vec![0.0_f32; box_w * box_h];
    let mut b_buf = vec![0.0_f32; box_w * box_h];

    for dy in 0..box_h {
        let y = y1 + dy as u32;
        for dx in 0..box_w {
            let x = x1 + dx as u32;
            let p = img.get_pixel(x, y);
            let idx = dy * box_w + dx;
            lums[idx] = pixel_luminance(p);
            r_buf[idx] = p[0] as f32;
            g_buf[idx] = p[1] as f32;
            b_buf[idx] = p[2] as f32;
        }
    }

    let (local_min, local_max) = compute_local_extrema(&lums, box_w, box_h, 6);
    let mut mask = vec![false; box_w * box_h];
    let mut allow_dilate = vec![false; box_w * box_h];

    for line in &bubble.lines {
        let lx1 = (line.x1 - 3.5).max(x1 as f32) as u32;
        let ly1 = (line.y1 - 3.5).max(y1 as f32) as u32;
        let lx2 = (line.x2 + 3.5).min(x2 as f32) as u32;
        let ly2 = (line.y2 + 3.5).min(y2 as f32) as u32;

        let mut quad_lums = Vec::new();
        for ly in ly1..=ly2 {
            let dy = (ly - y1) as usize;
            let py = ly as f32 + 0.5;
            for lx in lx1..=lx2 {
                let dx = (lx - x1) as usize;
                let px = lx as f32 + 0.5;
                if point_in_expanded_quad(px, py, &line.quad, 3.0) {
                    allow_dilate[dy * box_w + dx] = true;
                }
                if point_in_expanded_quad(px, py, &line.quad, 2.0) {
                    quad_lums.push(lums[dy * box_w + dx]);
                }
            }
        }

        if quad_lums.is_empty() {
            continue;
        }
        quad_lums.sort_unstable();
        let p10 = quad_lums[quad_lums.len() / 10];
        let p50 = quad_lums[quad_lums.len() / 2];
        let p90 = quad_lums[(quad_lums.len() * 9) / 10];
        let is_dark_text = (p50.saturating_sub(p10)) >= (p90.saturating_sub(p50));

        for ly in ly1..=ly2 {
            let dy = (ly - y1) as usize;
            let py = ly as f32 + 0.5;
            for lx in lx1..=lx2 {
                let dx = (lx - x1) as usize;
                let px = lx as f32 + 0.5;
                if !point_in_expanded_quad(px, py, &line.quad, 2.0) {
                    continue;
                }
                let idx = dy * box_w + dx;
                let lum = lums[idx];
                let is_text_pixel = if is_dark_text {
                    local_max[idx].saturating_sub(lum) > 22 && lum <= p50.saturating_sub(10)
                } else {
                    lum.saturating_sub(local_min[idx]) > 22 && lum >= p50.saturating_add(10)
                };
                if is_text_pixel {
                    mask[idx] = true;
                }
            }
        }
    }

    let mut dilated = mask.clone();
    for _ in 0..2 {
        let mut next = dilated.clone();
        for dy in 0..box_h {
            for dx in 0..box_w {
                if !dilated[dy * box_w + dx] {
                    continue;
                }
                for ndy in dy.saturating_sub(1)..=(dy + 1).min(box_h - 1) {
                    for ndx in dx.saturating_sub(1)..=(dx + 1).min(box_w - 1) {
                        let nidx = ndy * box_w + ndx;
                        if allow_dilate[nidx] {
                            next[nidx] = true;
                        }
                    }
                }
            }
        }
        dilated = next;
    }

    for _ in 0..90 {
        let mut next_r = r_buf.clone();
        let mut next_g = g_buf.clone();
        let mut next_b = b_buf.clone();

        for dy in 0..box_h {
            for dx in 0..box_w {
                let idx = dy * box_w + dx;
                if !dilated[idx] {
                    continue;
                }

                let mut sum_w = 0.0_f32;
                let mut sum_r = 0.0_f32;
                let mut sum_g = 0.0_f32;
                let mut sum_b = 0.0_f32;

                let neighbors = [
                    (dx > 0, dx.wrapping_sub(1), dy),
                    (dx + 1 < box_w, dx + 1, dy),
                    (dy > 0, dx, dy.wrapping_sub(1)),
                    (dy + 1 < box_h, dx, dy + 1),
                ];

                for (valid, nx, ny) in neighbors {
                    if valid {
                        let nidx = ny * box_w + nx;
                        let weight = if dilated[nidx] { 0.8 } else { 2.5 };
                        sum_w += weight;
                        sum_r += r_buf[nidx] * weight;
                        sum_g += g_buf[nidx] * weight;
                        sum_b += b_buf[nidx] * weight;
                    }
                }

                if sum_w > 0.0 {
                    next_r[idx] = sum_r / sum_w;
                    next_g[idx] = sum_g / sum_w;
                    next_b[idx] = sum_b / sum_w;
                }
            }
        }

        r_buf = next_r;
        g_buf = next_g;
        b_buf = next_b;
    }

    for dy in 0..box_h {
        let y = y1 + dy as u32;
        for dx in 0..box_w {
            let idx = dy * box_w + dx;
            if dilated[idx] {
                let x = x1 + dx as u32;
                img.put_pixel(
                    x,
                    y,
                    Rgb([
                        r_buf[idx].round().clamp(0.0, 255.0) as u8,
                        g_buf[idx].round().clamp(0.0, 255.0) as u8,
                        b_buf[idx].round().clamp(0.0, 255.0) as u8,
                    ]),
                );
            }
        }
    }
}
