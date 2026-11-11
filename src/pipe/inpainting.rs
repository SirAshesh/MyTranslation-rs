use crate::model::Bubble;
use image::{Rgb, RgbImage};

pub fn clean_image_text(img: &mut RgbImage, bubbles: &[Bubble]) {
    let (width, height) = (img.width(), img.height());

    for b in bubbles {
        for line in &b.lines {
            let pad_x = (line.h * 0.24).max(6.0).min(24.0);
            let pad_y = (line.h * 0.16).max(4.0).min(14.0);
            let lx1 = (line.x1 - pad_x).max(0.0) as u32;
            let ly1 = (line.y1 - pad_y).max(0.0) as u32;
            let lx2 = (line.x2 + pad_x).min(width as f32 - 1.0) as u32;
            let ly2 = (line.y2 + pad_y).min(height as f32 - 1.0) as u32;

            if lx2 <= lx1 || ly2 <= ly1 {
                continue;
            }

            clean_line_region(img, lx1, ly1, lx2, ly2);
        }
    }
}

fn clean_line_region(img: &mut RgbImage, x1: u32, y1: u32, x2: u32, y2: u32) {
    let box_w = (x2 - x1 + 1) as usize;
    let box_h = (y2 - y1 + 1) as usize;

    let mut border_lums = Vec::with_capacity((box_w + box_h) * 2);
    for x in x1..=x2 {
        for y in [y1, y2] {
            let p = img.get_pixel(x, y);
            let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
            border_lums.push(lum);
        }
    }
    for y in y1..=y2 {
        for x in [x1, x2] {
            let p = img.get_pixel(x, y);
            let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
            border_lums.push(lum);
        }
    }

    if border_lums.is_empty() {
        return;
    }

    border_lums.sort_unstable();
    let median_lum = border_lums[border_lums.len() / 2];

    let total_border = border_lums.len() as f32;
    let dark_border_count = border_lums.iter().filter(|&&l| l < 220).count() as f32;
    let dark_border_fraction = dark_border_count / total_border;

    let border_mean = border_lums.iter().map(|&l| l as f32).sum::<f32>() / total_border;
    let border_var = border_lums
        .iter()
        .map(|&l| {
            let d = l as f32 - border_mean;
            d * d
        })
        .sum::<f32>()
        / total_border;
    let border_std = border_var.sqrt();

    let is_pure_white_bg = median_lum >= 244 && border_std < 8.0 && dark_border_fraction < 0.04;

    let mut mask = vec![false; box_w * box_h];

    if is_pure_white_bg {
        for dy in 0..box_h {
            let y = y1 + dy as u32;
            for dx in 0..box_w {
                let x = x1 + dx as u32;
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
                if lum < 195 {
                    mask[dy * box_w + dx] = true;
                }
            }
        }

        let dilated = dilate_mask(&mask, box_w, box_h, 2);

        for dy in 0..box_h {
            let y = y1 + dy as u32;
            for dx in 0..box_w {
                if dilated[dy * box_w + dx] {
                    let x = x1 + dx as u32;
                    img.put_pixel(x, y, Rgb([255, 255, 255]));
                }
            }
        }
    } else {
        let is_dark_bg = median_lum <= 65;

        for dy in 0..box_h {
            let y = y1 + dy as u32;
            for dx in 0..box_w {
                let x = x1 + dx as u32;
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;

                if is_dark_bg {
                    if lum > 105 {
                        mask[dy * box_w + dx] = true;
                    }
                } else {
                    let is_dark_ink = lum < 135 && lum < median_lum.saturating_sub(20);
                    let is_white_halo = lum > 220 && median_lum < 235;
                    if is_dark_ink || is_white_halo {
                        mask[dy * box_w + dx] = true;
                    }
                }
            }
        }

        let dilated = dilate_mask(&mask, box_w, box_h, 2);
        diffuse_inward(img, x1, y1, x2, y2, &dilated, box_w, box_h);
    }
}

fn dilate_mask(mask: &[bool], w: usize, h: usize, passes: usize) -> Vec<bool> {
    let mut cur = mask.to_vec();
    for _ in 0..passes {
        let mut next = cur.clone();
        for dy in 0..h {
            for dx in 0..w {
                if cur[dy * w + dx] {
                    for ndy in dy.saturating_sub(1)..=(dy + 1).min(h - 1) {
                        for ndx in dx.saturating_sub(1)..=(dx + 1).min(w - 1) {
                            next[ndy * w + ndx] = true;
                        }
                    }
                }
            }
        }
        cur = next;
    }
    cur
}

fn diffuse_inward(
    img: &mut RgbImage,
    x1: u32,
    y1: u32,
    _x2: u32,
    _y2: u32,
    mask: &[bool],
    box_w: usize,
    box_h: usize,
) {
    let (img_w, img_h) = (img.width(), img.height());
    let mut unfilled = mask.to_vec();

    for _ in 0..32 {
        if !unfilled.iter().any(|&v| v) {
            break;
        }

        let mut newly_filled = Vec::new();

        for dy in 0..box_h {
            for dx in 0..box_w {
                let idx = dy * box_w + dx;
                if !unfilled[idx] {
                    continue;
                }

                let x = x1 + dx as u32;
                let y = y1 + dy as u32;

                let mut sum_w = 0.0_f32;
                let mut sum_r = 0.0_f32;
                let mut sum_g = 0.0_f32;
                let mut sum_b = 0.0_f32;

                for sdy in -1..=1_i32 {
                    let ny = y as i32 + sdy;
                    if ny < 0 || ny >= img_h as i32 {
                        continue;
                    }
                    for sdx in -1..=1_i32 {
                        if sdx == 0 && sdy == 0 {
                            continue;
                        }
                        let nx = x as i32 + sdx;
                        if nx < 0 || nx >= img_w as i32 {
                            continue;
                        }

                        let in_box = nx >= x1 as i32
                            && nx < (x1 + box_w as u32) as i32
                            && ny >= y1 as i32
                            && ny < (y1 + box_h as u32) as i32;

                        let is_unfilled = if in_box {
                            let bdx = (nx - x1 as i32) as usize;
                            let bdy = (ny - y1 as i32) as usize;
                            unfilled[bdy * box_w + bdx]
                        } else {
                            false
                        };

                        if !is_unfilled {
                            let w_val = if sdx == 0 || sdy == 0 { 1.0 } else { 0.707 };
                            let np = img.get_pixel(nx as u32, ny as u32);
                            sum_w += w_val;
                            sum_r += np[0] as f32 * w_val;
                            sum_g += np[1] as f32 * w_val;
                            sum_b += np[2] as f32 * w_val;
                        }
                    }
                }

                if sum_w > 0.0 {
                    let fr = (sum_r / sum_w).round().clamp(0.0, 255.0) as u8;
                    let fg = (sum_g / sum_w).round().clamp(0.0, 255.0) as u8;
                    let fb = (sum_b / sum_w).round().clamp(0.0, 255.0) as u8;
                    newly_filled.push((dx, dy, x, y, Rgb([fr, fg, fb])));
                }
            }
        }

        if newly_filled.is_empty() {
            break;
        }

        for (dx, dy, x, y, p) in newly_filled {
            unfilled[dy * box_w + dx] = false;
            img.put_pixel(x, y, p);
        }
    }
}
