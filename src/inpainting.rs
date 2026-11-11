use crate::models::Bubble;
use image::{Rgb, RgbImage};

pub fn clean_image_text(img: &mut RgbImage, bubbles: &[Bubble]) {
    let (width, height) = (img.width(), img.height());

    for b in bubbles {
        for line in &b.lines {
            let lx1 = (line.x1 - 4.0).max(0.0) as u32;
            let ly1 = (line.y1 - 3.0).max(0.0) as u32;
            let lx2 = (line.x2 + 4.0).min(width as f32 - 1.0) as u32;
            let ly2 = (line.y2 + 3.0).min(height as f32 - 1.0) as u32;

            if lx2 <= lx1 || ly2 <= ly1 {
                continue;
            }

            clean_line_region(img, lx1, ly1, lx2, ly2);
        }

        if b.lines.len() >= 2 {
            let avg_h = b.lines.iter().map(|l| l.h).sum::<f32>() / (b.lines.len() as f32);
            let top_y1 = (b.y1 - avg_h * 1.1).max(0.0) as u32;
            let top_y2 = (b.y1 + 2.0).min(height as f32 - 1.0) as u32;
            let top_x1 = (b.x1 - 4.0).max(0.0) as u32;
            let top_x2 = (b.x2 + 4.0).min(width as f32 - 1.0) as u32;

            if top_y2 > top_y1 && top_x2 > top_x1 {
                clean_bubble_upper_resample(img, top_x1, top_y1, top_x2, top_y2);
            }
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

    let mut mask = vec![false; box_w * box_h];

    if median_lum >= 238 {
        for dy in 0..box_h {
            let y = y1 + dy as u32;
            for dx in 0..box_w {
                let x = x1 + dx as u32;
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
                if lum < 185 {
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
                    let is_dark_ink = lum < 135 && lum < median_lum.saturating_sub(25);
                    let is_white_halo = lum > 225 && median_lum < 195;
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

fn clean_bubble_upper_resample(img: &mut RgbImage, x1: u32, y1: u32, x2: u32, y2: u32) {
    let box_w = (x2 - x1 + 1) as usize;
    let box_h = (y2 - y1 + 1) as usize;

    let p_left = img.get_pixel(x1, (y1 + y2) / 2);
    let p_right = img.get_pixel(x2, (y1 + y2) / 2);
    let lum_l = (0.299 * p_left[0] as f32 + 0.587 * p_left[1] as f32 + 0.114 * p_left[2] as f32) as u8;
    let lum_r = (0.299 * p_right[0] as f32 + 0.587 * p_right[1] as f32 + 0.114 * p_right[2] as f32) as u8;

    if lum_l < 210 || lum_r < 210 {
        return;
    }

    let mut mask = vec![false; box_w * box_h];
    let mut has_ink = false;

    for dy in 0..box_h {
        let y = y1 + dy as u32;
        for dx in 0..box_w {
            let x = x1 + dx as u32;
            let p = img.get_pixel(x, y);
            let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
            if lum < 170 {
                mask[dy * box_w + dx] = true;
                has_ink = true;
            }
        }
    }

    if !has_ink {
        return;
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

    for _ in 0..28 {
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
