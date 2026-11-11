use crate::models::Bubble;
use image::{Rgb, RgbImage};

pub fn clean_image_text(img: &mut RgbImage, bubbles: &[Bubble]) {
    let (width, height) = (img.width(), img.height());

    for b in bubbles {
        let pad = 6.0;
        let bx1 = ((b.x1 - pad).max(0.0) as u32).min(width - 1);
        let by1 = ((b.y1 - pad).max(0.0) as u32).min(height - 1);
        let bx2 = ((b.x2 + pad).max(0.0) as u32).min(width - 1);
        let by2 = ((b.y2 + pad).max(0.0) as u32).min(height - 1);

        if bx2 > bx1 && by2 > by1 {
            inpaint_box(img, bx1, by1, bx2, by2, 26.0, 2);
        }

        for line in &b.lines {
            let lpad = 4.0;
            let lx1 = ((line.x1 - lpad).max(0.0) as u32).min(width - 1);
            let ly1 = ((line.y1 - lpad).max(0.0) as u32).min(height - 1);
            let lx2 = ((line.x2 + lpad).max(0.0) as u32).min(width - 1);
            let ly2 = ((line.y2 + lpad).max(0.0) as u32).min(height - 1);

            if lx2 > lx1 && ly2 > ly1 {
                inpaint_box(img, lx1, ly1, lx2, ly2, 28.0, 2);
            }
        }
    }
}

fn inpaint_box(
    img: &mut RgbImage,
    x1: u32,
    y1: u32,
    x2: u32,
    y2: u32,
    threshold: f32,
    dilate_passes: usize,
) {
    let box_w = (x2 - x1 + 1) as usize;
    let box_h = (y2 - y1 + 1) as usize;

    let mut perimeter_r = Vec::with_capacity((box_w + box_h) * 2);
    let mut perimeter_g = Vec::with_capacity((box_w + box_h) * 2);
    let mut perimeter_b = Vec::with_capacity((box_w + box_h) * 2);

    for x in x1..=x2 {
        for y in [y1, y1 + 1, y2.saturating_sub(1), y2] {
            if y >= y1 && y <= y2 {
                let p = img.get_pixel(x, y);
                perimeter_r.push(p[0]);
                perimeter_g.push(p[1]);
                perimeter_b.push(p[2]);
            }
        }
    }
    for y in y1..=y2 {
        for x in [x1, x1 + 1, x2.saturating_sub(1), x2] {
            if x >= x1 && x <= x2 {
                let p = img.get_pixel(x, y);
                perimeter_r.push(p[0]);
                perimeter_g.push(p[1]);
                perimeter_b.push(p[2]);
            }
        }
    }

    if perimeter_r.is_empty() {
        return;
    }

    let median_r = median(&mut perimeter_r);
    let median_g = median(&mut perimeter_g);
    let median_b = median(&mut perimeter_b);
    let bg_pixel = Rgb([median_r, median_g, median_b]);

    let threshold_sq = threshold * threshold;
    let mut mask = vec![false; box_w * box_h];

    for dy in 0..box_h {
        for dx in 0..box_w {
            let px = img.get_pixel(x1 + dx as u32, y1 + dy as u32);
            let dr = (px[0] as f32) - (median_r as f32);
            let dg = (px[1] as f32) - (median_g as f32);
            let db = (px[2] as f32) - (median_b as f32);
            let dist_sq = dr * dr + dg * dg + db * db;

            if dist_sq > threshold_sq {
                mask[dy * box_w + dx] = true;
            }
        }
    }

    let mut dilated = mask;
    for _ in 0..dilate_passes {
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
        for dx in 0..box_w {
            if dilated[dy * box_w + dx] {
                img.put_pixel(x1 + dx as u32, y1 + dy as u32, bg_pixel);
            }
        }
    }
}

fn median(v: &mut [u8]) -> u8 {
    v.sort_unstable();
    v[v.len() / 2]
}
