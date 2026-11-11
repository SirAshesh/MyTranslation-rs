use crate::models::Bubble;
use image::{Rgb, RgbImage};

pub fn clean_image_text(img: &mut RgbImage, bubbles: &[Bubble]) {
    let (width, height) = (img.width(), img.height());

    for b in bubbles {
        let bx1 = (b.x1 as u32).min(width - 1);
        let by1 = (b.y1 as u32).min(height - 1);
        let bx2 = (b.x2 as u32).min(width - 1);
        let by2 = (b.y2 as u32).min(height - 1);

        if bx2 <= bx1 || by2 <= by1 {
            continue;
        }

        let pad = 4;
        let sx1 = bx1.saturating_sub(pad);
        let sy1 = by1.saturating_sub(pad);
        let sx2 = (bx2 + pad).min(width - 1);
        let sy2 = (by2 + pad).min(height - 1);

        let mut perimeter_lums = Vec::with_capacity(((sx2 - sx1 + sy2 - sy1) * 2 + 8) as usize);
        let mut perimeter_r = Vec::new();
        let mut perimeter_g = Vec::new();
        let mut perimeter_b = Vec::new();

        for x in sx1..=sx2 {
            for y in [sy1, sy2] {
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
                perimeter_lums.push(lum);
                perimeter_r.push(p[0]);
                perimeter_g.push(p[1]);
                perimeter_b.push(p[2]);
            }
        }
        for y in sy1..=sy2 {
            for x in [sx1, sx2] {
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
                perimeter_lums.push(lum);
                perimeter_r.push(p[0]);
                perimeter_g.push(p[1]);
                perimeter_b.push(p[2]);
            }
        }

        if perimeter_lums.is_empty() {
            continue;
        }

        perimeter_lums.sort_unstable();
        let median_lum = perimeter_lums[perimeter_lums.len() / 2];
        let p75_lum = perimeter_lums[(perimeter_lums.len() * 3) / 4];
        let p90_lum = perimeter_lums[(perimeter_lums.len() * 9) / 10];

        perimeter_r.sort_unstable();
        perimeter_g.sort_unstable();
        perimeter_b.sort_unstable();
        let bg_rgb = Rgb([
            perimeter_r[perimeter_r.len() / 2],
            perimeter_g[perimeter_g.len() / 2],
            perimeter_b[perimeter_b.len() / 2],
        ]);

        let is_white_bubble = p75_lum >= 205 && p90_lum >= 235;
        let is_dark_bubble = p75_lum <= 65;

        if is_white_bubble {
            clean_white_bubble(img, b, bg_rgb, median_lum);
        } else if is_dark_bubble {
            clean_dark_bubble(img, b, bg_rgb);
        } else {
            clean_screentone_or_art(img, b);
        }
    }
}

fn clean_white_bubble(img: &mut RgbImage, b: &Bubble, bg_rgb: Rgb<u8>, median_lum: u8) {
    let (width, height) = (img.width(), img.height());

    let avg_line_h = if !b.lines.is_empty() {
        b.lines.iter().map(|l| l.h).sum::<f32>() / (b.lines.len() as f32)
    } else {
        18.0
    };

    let min_x = (b.x1 - 6.0).max(0.0) as u32;
    let max_x = (b.x2 + 6.0).min(width as f32 - 1.0) as u32;
    let min_y = (b.y1 - (avg_line_h * 1.25)).max(0.0) as u32;
    let max_y = (b.y2 + 8.0).min(height as f32 - 1.0) as u32;

    if max_x <= min_x || max_y <= min_y {
        return;
    }

    let box_w = (max_x - min_x + 1) as usize;
    let box_h = (max_y - min_y + 1) as usize;
    let mut mask = vec![false; box_w * box_h];

    let ink_thresh = median_lum.saturating_sub(38).min(175);

    for dy in 0..box_h {
        let y = min_y + dy as u32;
        for dx in 0..box_w {
            let x = min_x + dx as u32;
            let p = img.get_pixel(x, y);
            let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;

            if lum < ink_thresh {
                let inside_line = b.lines.iter().any(|l| {
                    let lx1 = (l.x1 - 5.0).max(0.0) as u32;
                    let ly1 = (l.y1 - 4.0).max(0.0) as u32;
                    let lx2 = (l.x2 + 5.0).min(width as f32 - 1.0) as u32;
                    let ly2 = (l.y2 + 4.0).min(height as f32 - 1.0) as u32;
                    x >= lx1 && x <= lx2 && y >= ly1 && y <= ly2
                });

                let is_upper_bubble_ink = y < b.y1 as u32 && {
                    let p_left = img.get_pixel(min_x, y);
                    let p_right = img.get_pixel(max_x, y);
                    let lum_l = (0.299 * p_left[0] as f32 + 0.587 * p_left[1] as f32 + 0.114 * p_left[2] as f32) as u8;
                    let lum_r = (0.299 * p_right[0] as f32 + 0.587 * p_right[1] as f32 + 0.114 * p_right[2] as f32) as u8;
                    lum_l >= 210 && lum_r >= 210
                };

                if inside_line || is_upper_bubble_ink {
                    mask[dy * box_w + dx] = true;
                }
            }
        }
    }

    let mut dilated = mask;
    for _ in 0..2 {
        let mut next_dilated = dilated.clone();
        for dy in 0..box_h {
            for dx in 0..box_w {
                if dilated[dy * box_w + dx] {
                    for ndy in dy.saturating_sub(1)..=(dy + 1).min(box_h - 1) {
                        for ndx in dx.saturating_sub(1)..=(dx + 1).min(box_w - 1) {
                            next_dilated[ndy * box_w + ndx] = true;
                        }
                    }
                }
            }
        }
        dilated = next_dilated;
    }

    let clean_white = median_lum >= 240;

    for dy in 0..box_h {
        let y = min_y + dy as u32;
        for dx in 0..box_w {
            if dilated[dy * box_w + dx] {
                let x = min_x + dx as u32;
                if clean_white {
                    img.put_pixel(x, y, Rgb([255, 255, 255]));
                } else {
                    let mut left_rgb = None;
                    for lx in (0..dx).rev() {
                        if !dilated[dy * box_w + lx] {
                            left_rgb = Some(*img.get_pixel(min_x + lx as u32, y));
                            break;
                        }
                    }
                    let mut right_rgb = None;
                    for rx in (dx + 1)..box_w {
                        if !dilated[dy * box_w + rx] {
                            right_rgb = Some(*img.get_pixel(min_x + rx as u32, y));
                            break;
                        }
                    }
                    let fill_p = match (left_rgb, right_rgb) {
                        (Some(l), Some(r)) => Rgb([
                            ((l[0] as u16 + r[0] as u16) / 2) as u8,
                            ((l[1] as u16 + r[1] as u16) / 2) as u8,
                            ((l[2] as u16 + r[2] as u16) / 2) as u8,
                        ]),
                        (Some(l), None) => l,
                        (None, Some(r)) => r,
                        (None, None) => bg_rgb,
                    };
                    img.put_pixel(x, y, fill_p);
                }
            }
        }
    }
}

fn clean_dark_bubble(img: &mut RgbImage, b: &Bubble, bg_rgb: Rgb<u8>) {
    let (width, height) = (img.width(), img.height());

    for line in &b.lines {
        let lx1 = (line.x1 - 4.0).max(0.0) as u32;
        let ly1 = (line.y1 - 3.0).max(0.0) as u32;
        let lx2 = (line.x2 + 4.0).min(width as f32 - 1.0) as u32;
        let ly2 = (line.y2 + 3.0).min(height as f32 - 1.0) as u32;

        if lx2 <= lx1 || ly2 <= ly1 {
            continue;
        }

        let box_w = (lx2 - lx1 + 1) as usize;
        let box_h = (ly2 - ly1 + 1) as usize;
        let mut mask = vec![false; box_w * box_h];

        for dy in 0..box_h {
            let y = ly1 + dy as u32;
            for dx in 0..box_w {
                let x = lx1 + dx as u32;
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;
                if lum > 105 {
                    mask[dy * box_w + dx] = true;
                }
            }
        }

        let mut dilated = mask;
        for _ in 0..2 {
            let mut next_dilated = dilated.clone();
            for dy in 0..box_h {
                for dx in 0..box_w {
                    if dilated[dy * box_w + dx] {
                        for ndy in dy.saturating_sub(1)..=(dy + 1).min(box_h - 1) {
                            for ndx in dx.saturating_sub(1)..=(dx + 1).min(box_w - 1) {
                                next_dilated[ndy * box_w + ndx] = true;
                            }
                        }
                    }
                }
            }
            dilated = next_dilated;
        }

        for dy in 0..box_h {
            let y = ly1 + dy as u32;
            for dx in 0..box_w {
                if dilated[dy * box_w + dx] {
                    let x = lx1 + dx as u32;
                    img.put_pixel(x, y, bg_rgb);
                }
            }
        }
    }
}

fn clean_screentone_or_art(img: &mut RgbImage, b: &Bubble) {
    let (width, height) = (img.width(), img.height());

    for line in &b.lines {
        let lx1 = (line.x1 - 2.0).max(0.0) as u32;
        let ly1 = (line.y1 - 2.0).max(0.0) as u32;
        let lx2 = (line.x2 + 2.0).min(width as f32 - 1.0) as u32;
        let ly2 = (line.y2 + 2.0).min(height as f32 - 1.0) as u32;

        if lx2 <= lx1 || ly2 <= ly1 {
            continue;
        }

        let box_w = (lx2 - lx1 + 1) as usize;
        let box_h = (ly2 - ly1 + 1) as usize;

        let mut mask = vec![false; box_w * box_h];

        for dy in 0..box_h {
            let y = ly1 + dy as u32;
            for dx in 0..box_w {
                let x = lx1 + dx as u32;
                let p = img.get_pixel(x, y);
                let lum = (0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32) as u8;

                if lum < 50 || lum > 225 {
                    mask[dy * box_w + dx] = true;
                }
            }
        }

        let mut dilated = mask;
        for _ in 0..1 {
            let mut next_dilated = dilated.clone();
            for dy in 0..box_h {
                for dx in 0..box_w {
                    if dilated[dy * box_w + dx] {
                        for ndy in dy.saturating_sub(1)..=(dy + 1).min(box_h - 1) {
                            for ndx in dx.saturating_sub(1)..=(dx + 1).min(box_w - 1) {
                                next_dilated[ndy * box_w + ndx] = true;
                            }
                        }
                    }
                }
            }
            dilated = next_dilated;
        }

        let mut filled_pixels = Vec::new();

        for dy in 0..box_h {
            let y = ly1 + dy as u32;
            for dx in 0..box_w {
                if dilated[dy * box_w + dx] {
                    let x = lx1 + dx as u32;

                    let mut sum_w = 0.0_f32;
                    let mut sum_r = 0.0_f32;
                    let mut sum_g = 0.0_f32;
                    let mut sum_b = 0.0_f32;

                    for sdy in -3..=3_i32 {
                        let ny = y as i32 + sdy;
                        if ny < 0 || ny >= height as i32 {
                            continue;
                        }
                        for sdx in -3..=3_i32 {
                            if sdx == 0 && sdy == 0 {
                                continue;
                            }
                            let nx = x as i32 + sdx;
                            if nx < 0 || nx >= width as i32 {
                                continue;
                            }

                            let in_box = nx >= lx1 as i32
                                && nx <= lx2 as i32
                                && ny >= ly1 as i32
                                && ny <= ly2 as i32;

                            let is_masked = if in_box {
                                let bdx = (nx - lx1 as i32) as usize;
                                let bdy = (ny - ly1 as i32) as usize;
                                dilated[bdy * box_w + bdx]
                            } else {
                                false
                            };

                            if !is_masked {
                                let d_sq = (sdx * sdx + sdy * sdy) as f32;
                                let w = 1.0 / d_sq;
                                let np = img.get_pixel(nx as u32, ny as u32);
                                sum_w += w;
                                sum_r += np[0] as f32 * w;
                                sum_g += np[1] as f32 * w;
                                sum_b += np[2] as f32 * w;
                            }
                        }
                    }

                    if sum_w > 0.0 {
                        let fr = (sum_r / sum_w).round().clamp(0.0, 255.0) as u8;
                        let fg = (sum_g / sum_w).round().clamp(0.0, 255.0) as u8;
                        let fb = (sum_b / sum_w).round().clamp(0.0, 255.0) as u8;
                        filled_pixels.push((x, y, Rgb([fr, fg, fb])));
                    }
                }
            }
        }

        for (x, y, p) in filled_pixels {
            img.put_pixel(x, y, p);
        }
    }
}
