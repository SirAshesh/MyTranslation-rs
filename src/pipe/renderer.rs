use crate::model::Bubble;
use ab_glyph::{Font, FontArc, PxScale, PxScaleFont, ScaleFont};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use std::collections::HashMap;

const MIN_TILT_RAD: f32 = 0.026;

pub struct ComicRenderer {
    font: FontArc,
}

impl ComicRenderer {
    pub fn new(font_path: &str) -> Result<Self, String> {
        let font_data = std::fs::read(font_path)
            .map_err(|e| format!("Failed to read font file {}: {}", font_path, e))?;
        let font = FontArc::try_from_vec(font_data)
            .map_err(|e| format!("Failed to parse font: {}", e))?;
        Ok(Self { font })
    }

    pub fn render_bubbles(
        &self,
        img: &mut RgbImage,
        bubbles: &[Bubble],
        translations: &HashMap<usize, String>,
    ) {
        for b in bubbles {
            let raw_target = match translations.get(&b.id) {
                Some(t) if !t.trim().is_empty() => t.trim().to_uppercase(),
                _ => continue,
            };
            let target_text: String = raw_target
                .chars()
                .filter(|&ch| ch.is_whitespace() || self.font.glyph_id(ch).0 != 0)
                .collect();
            let target_text = target_text.trim();
            if target_text.is_empty() {
                continue;
            }

            let center_x = if b.oriented_w > 1.0 {
                b.cx
            } else {
                (b.x1 + b.x2) / 2.0
            };
            let center_y = if b.oriented_h > 1.0 {
                b.cy
            } else {
                (b.y1 + b.y2) / 2.0
            };
            let base_w = if b.oriented_w > 1.0 { b.oriented_w } else { b.w };
            let base_h = if b.oriented_h > 1.0 { b.oriented_h } else { b.h };

            let safe_w = (base_w * 1.15).max(28.0);
            let safe_h = (base_h * 1.22).max(14.0);

            let words: Vec<&str> = target_text.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }

            let candidate_sizes = [
                38.0, 34.0, 30.0, 28.0, 26.0, 24.0, 22.0, 20.0, 18.0, 16.5, 15.0, 14.0, 13.0, 12.0,
                11.0, 10.5, 10.0, 9.5, 9.0, 8.5, 8.0, 7.5,
            ];

            let mut best_size = 8.0;
            let mut best_lines: Vec<String> = Vec::new();

            for &size in &candidate_sizes {
                let scale = PxScale::from(size);
                let scaled_font = self.font.as_scaled(scale);
                let lines = self.wrap_words(&scaled_font, &words, safe_w);

                let line_height = scaled_font.height() * 1.12;
                let total_h = (lines.len() as f32) * line_height;
                let max_w = lines
                    .iter()
                    .map(|l| self.measure_line_width(&scaled_font, l))
                    .fold(0.0_f32, f32::max);

                if total_h <= safe_h && max_w <= safe_w * 1.08 {
                    best_size = size;
                    best_lines = lines;
                    break;
                }
            }

            if best_lines.is_empty() {
                best_size = 7.5;
                let scale = PxScale::from(best_size);
                let scaled_font = self.font.as_scaled(scale);
                best_lines = self.wrap_words(&scaled_font, &words, safe_w * 1.15);
            }

            let scale = PxScale::from(best_size);
            let scaled_font = self.font.as_scaled(scale);
            let line_height = scaled_font.height() * 1.12;
            let total_h = (best_lines.len() as f32) * line_height;
            let max_line_w = best_lines
                .iter()
                .map(|l| self.measure_line_width(&scaled_font, l))
                .fold(0.0_f32, f32::max);
            let block_left = -(max_line_w.max(base_w).min(safe_w)) * 0.5;

            if b.angle_rad.abs() < MIN_TILT_RAD {
                let mut start_y = center_y - total_h / 2.0;
                if start_y < b.y1 - 6.0 {
                    start_y = b.y1 - 6.0;
                }
                if start_y + total_h > b.y2 + 8.0 {
                    start_y = (b.y2 + 8.0 - total_h).max(b.y1 - 6.0);
                }

                for (idx, line) in best_lines.iter().enumerate() {
                    let line_w = self.measure_line_width(&scaled_font, line);
                    let line_x = if b.is_left_aligned {
                        center_x + block_left
                    } else {
                        center_x - line_w / 2.0
                    };
                    let line_y = start_y + (idx as f32) * line_height;

                    self.draw_stroked_text_rgb(
                        img,
                        &scaled_font,
                        line,
                        line_x,
                        line_y,
                        Rgb([12, 12, 12]),
                        Rgb([255, 255, 255]),
                    );
                }
            } else {
                let pad = 6.0_f32;
                let block_w = max_line_w.max(base_w).min(safe_w);
                let buf_w = ((block_w.max(max_line_w) + pad * 2.0).ceil() as u32).max(16);
                let buf_h = ((total_h + pad * 2.0).ceil() as u32).max(16);
                let mut overlay = RgbaImage::new(buf_w, buf_h);

                let local_cx = buf_w as f32 * 0.5;
                let local_start_y = (buf_h as f32 - total_h) * 0.5;

                for (idx, line) in best_lines.iter().enumerate() {
                    let line_w = self.measure_line_width(&scaled_font, line);
                    let line_x = if b.is_left_aligned {
                        local_cx + block_left
                    } else {
                        local_cx - line_w / 2.0
                    };
                    let line_y = local_start_y + (idx as f32) * line_height;

                    self.draw_stroked_text_rgba(
                        &mut overlay,
                        &scaled_font,
                        line,
                        line_x,
                        line_y,
                        Rgb([12, 12, 12]),
                        Rgb([255, 255, 255]),
                    );
                }

                composite_rotated_overlay(img, &overlay, center_x, center_y, b.angle_rad);
            }
        }
    }

    fn wrap_words(&self, scaled_font: &PxScaleFont<&FontArc>, words: &[&str], max_w: f32) -> Vec<String> {
        let mut lines = Vec::new();
        let mut cur_line = Vec::new();

        for word in words {
            let mut test_words = cur_line.clone();
            test_words.push(*word);
            let test_str = test_words.join(" ");

            let line_w = self.measure_line_width(scaled_font, &test_str);
            if line_w <= max_w {
                cur_line.push(*word);
            } else if !cur_line.is_empty() {
                lines.push(cur_line.join(" "));
                cur_line = vec![*word];
            } else {
                lines.push((*word).to_string());
                cur_line.clear();
            }
        }
        if !cur_line.is_empty() {
            lines.push(cur_line.join(" "));
        }
        lines
    }

    fn measure_line_width(&self, scaled_font: &PxScaleFont<&FontArc>, text: &str) -> f32 {
        let mut width = 0.0;
        let mut last_glyph: Option<ab_glyph::GlyphId> = None;

        for ch in text.chars() {
            let glyph_id = scaled_font.glyph_id(ch);
            if let Some(last) = last_glyph {
                width += scaled_font.kern(last, glyph_id);
            }
            width += scaled_font.h_advance(glyph_id);
            last_glyph = Some(glyph_id);
        }
        width
    }

    fn draw_stroked_text_rgba(
        &self,
        overlay: &mut RgbaImage,
        scaled_font: &PxScaleFont<&FontArc>,
        text: &str,
        x: f32,
        y: f32,
        fill_color: Rgb<u8>,
        stroke_color: Rgb<u8>,
    ) {
        let (buf_w, buf_h) = (overlay.width() as i32, overlay.height() as i32);
        let offsets = [
            (-1, 0), (1, 0), (0, -1), (0, 1),
            (-1, -1), (1, 1), (-1, 1), (1, -1),
        ];

        for (ox, oy) in offsets {
            self.rasterize_text(scaled_font, text, x + ox as f32, y + oy as f32, |px, py, c| {
                if c > 0.35 && px >= 0 && px < buf_w && py >= 0 && py < buf_h {
                    overlay.put_pixel(px as u32, py as u32, Rgba([stroke_color[0], stroke_color[1], stroke_color[2], 255]));
                }
            });
        }

        self.rasterize_text(scaled_font, text, x, y, |px, py, c| {
            if c > 0.05 && px >= 0 && px < buf_w && py >= 0 && py < buf_h {
                let dst = overlay.get_pixel(px as u32, py as u32);
                if dst[3] == 0 {
                    let a = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                    overlay.put_pixel(px as u32, py as u32, Rgba([fill_color[0], fill_color[1], fill_color[2], a]));
                } else {
                    let r = ((1.0 - c) * dst[0] as f32 + c * fill_color[0] as f32).round() as u8;
                    let g = ((1.0 - c) * dst[1] as f32 + c * fill_color[1] as f32).round() as u8;
                    let b = ((1.0 - c) * dst[2] as f32 + c * fill_color[2] as f32).round() as u8;
                    overlay.put_pixel(px as u32, py as u32, Rgba([r, g, b, 255]));
                }
            }
        });
    }

    fn draw_stroked_text_rgb(
        &self,
        img: &mut RgbImage,
        scaled_font: &PxScaleFont<&FontArc>,
        text: &str,
        x: f32,
        y: f32,
        fill_color: Rgb<u8>,
        stroke_color: Rgb<u8>,
    ) {
        let (img_w, img_h) = (img.width() as i32, img.height() as i32);
        let offsets = [
            (-1, 0), (1, 0), (0, -1), (0, 1),
            (-1, -1), (1, 1), (-1, 1), (1, -1),
        ];

        for (ox, oy) in offsets {
            self.rasterize_text(scaled_font, text, x + ox as f32, y + oy as f32, |px, py, c| {
                if c > 0.35 && px >= 0 && px < img_w && py >= 0 && py < img_h {
                    img.put_pixel(px as u32, py as u32, stroke_color);
                }
            });
        }

        self.rasterize_text(scaled_font, text, x, y, |px, py, c| {
            if c > 0.05 && px >= 0 && px < img_w && py >= 0 && py < img_h {
                let current = img.get_pixel(px as u32, py as u32);
                let r = ((1.0 - c) * current[0] as f32 + c * fill_color[0] as f32) as u8;
                let g = ((1.0 - c) * current[1] as f32 + c * fill_color[1] as f32) as u8;
                let b = ((1.0 - c) * current[2] as f32 + c * fill_color[2] as f32) as u8;
                img.put_pixel(px as u32, py as u32, Rgb([r, g, b]));
            }
        });
    }

    fn rasterize_text<F>(
        &self,
        scaled_font: &PxScaleFont<&FontArc>,
        text: &str,
        x: f32,
        y: f32,
        mut plot: F,
    ) where
        F: FnMut(i32, i32, f32),
    {
        let mut pen_x = x;
        let pen_y = y + scaled_font.ascent();
        let mut last_glyph: Option<ab_glyph::GlyphId> = None;

        for ch in text.chars() {
            let glyph_id = scaled_font.glyph_id(ch);
            if let Some(last) = last_glyph {
                pen_x += scaled_font.kern(last, glyph_id);
            }

            let glyph = glyph_id.with_scale_and_position(scaled_font.scale(), ab_glyph::point(pen_x, pen_y));
            if let Some(outlined) = self.font.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                outlined.draw(|gx, gy, c| {
                    plot(bounds.min.x as i32 + gx as i32, bounds.min.y as i32 + gy as i32, c);
                });
            }

            pen_x += scaled_font.h_advance(glyph_id);
            last_glyph = Some(glyph_id);
        }
    }
}

fn composite_rotated_overlay(
    img: &mut RgbImage,
    overlay: &RgbaImage,
    center_x: f32,
    center_y: f32,
    angle_rad: f32,
) {
    let (buf_w, buf_h) = (overlay.width() as f32, overlay.height() as f32);
    let half_w = buf_w * 0.5;
    let half_h = buf_h * 0.5;
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let radius_x = (half_w * cos_a.abs() + half_h * sin_a.abs()).ceil();
    let radius_y = (half_w * sin_a.abs() + half_h * cos_a.abs()).ceil();

    let min_x = ((center_x - radius_x).floor() as i32).max(0);
    let max_x = ((center_x + radius_x).ceil() as i32).min(img.width() as i32 - 1);
    let min_y = ((center_y - radius_y).floor() as i32).max(0);
    let max_y = ((center_y + radius_y).ceil() as i32).min(img.height() as i32 - 1);

    let max_ox = overlay.width() as i32 - 1;
    let max_oy = overlay.height() as i32 - 1;

    for py in min_y..=max_y {
        let dy = py as f32 + 0.5 - center_y;
        for px in min_x..=max_x {
            let dx = px as f32 + 0.5 - center_x;
            let lx = dx * cos_a + dy * sin_a + half_w - 0.5;
            let ly = -dx * sin_a + dy * cos_a + half_h - 0.5;

            if lx < -1.0 || ly < -1.0 || lx > max_ox as f32 + 1.0 || ly > max_oy as f32 + 1.0 {
                continue;
            }

            let x0 = lx.floor() as i32;
            let y0 = ly.floor() as i32;
            let fx = lx - x0 as f32;
            let fy = ly - y0 as f32;

            let sample_premul = |sx: i32, sy: i32| -> (f32, f32, f32, f32) {
                if sx < 0 || sy < 0 || sx > max_ox || sy > max_oy {
                    return (0.0, 0.0, 0.0, 0.0);
                }
                let p = overlay.get_pixel(sx as u32, sy as u32);
                let a = p[3] as f32 / 255.0;
                (p[0] as f32 * a, p[1] as f32 * a, p[2] as f32 * a, a)
            };

            let (r00, g00, b00, a00) = sample_premul(x0, y0);
            let (r10, g10, b10, a10) = sample_premul(x0 + 1, y0);
            let (r01, g01, b01, a01) = sample_premul(x0, y0 + 1);
            let (r11, g11, b11, a11) = sample_premul(x0 + 1, y0 + 1);

            let w00 = (1.0 - fx) * (1.0 - fy);
            let w10 = fx * (1.0 - fy);
            let w01 = (1.0 - fx) * fy;
            let w11 = fx * fy;

            let a = a00 * w00 + a10 * w10 + a01 * w01 + a11 * w11;
            if a <= 0.01 {
                continue;
            }

            let pr = r00 * w00 + r10 * w10 + r01 * w01 + r11 * w11;
            let pg = g00 * w00 + g10 * w10 + g01 * w01 + g11 * w11;
            let pb = b00 * w00 + b10 * w10 + b01 * w01 + b11 * w11;

            let bg = img.get_pixel(px as u32, py as u32);
            let inv_a = (1.0 - a).max(0.0);
            let out_r = (pr + inv_a * bg[0] as f32).round().clamp(0.0, 255.0) as u8;
            let out_g = (pg + inv_a * bg[1] as f32).round().clamp(0.0, 255.0) as u8;
            let out_b = (pb + inv_a * bg[2] as f32).round().clamp(0.0, 255.0) as u8;
            img.put_pixel(px as u32, py as u32, Rgb([out_r, out_g, out_b]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composite_rotated_overlay_draws_at_tilted_endpoints() {
        let mut img = RgbImage::from_pixel(200, 200, Rgb([100, 140, 180]));
        let mut overlay = RgbaImage::new(80, 20);
        for y in 6..14 {
            for x in 6..74 {
                overlay.put_pixel(x, y, Rgba([12, 12, 12, 255]));
            }
        }

        let angle = -0.28_f32;
        composite_rotated_overlay(&mut img, &overlay, 100.0, 100.0, angle);

        let right_up = img.get_pixel(130, 91);
        let horizontal_far = img.get_pixel(130, 100);
        assert!(right_up[0] < 40, "expected rotated ink at (130, 91), got {:?}", right_up);
        assert_eq!(*horizontal_far, Rgb([100, 140, 180]));
    }
}
