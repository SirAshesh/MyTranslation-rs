#[path = "renderer_bounds.rs"]
mod renderer_bounds;

use crate::model::Bubble;
use ab_glyph::{Font, FontArc, PxScale, PxScaleFont, ScaleFont};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use renderer_bounds::{
    compute_block_start_y, detect_horizontal_bubble_bounds, detect_vertical_bubble_bounds,
    measure_block_cap_safe_span,
};
use std::collections::HashMap;

const MIN_TILT_RAD: f32 = 0.026;

enum RenderTarget<'a> {
    Rgb(&'a mut RgbImage),
    Rgba(&'a mut RgbaImage),
}

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

    pub fn renderable_text(
        &self,
        bubble: &Bubble,
        translations: &HashMap<usize, String>,
    ) -> Option<String> {
        let raw_target = match translations.get(&bubble.id) {
            Some(t) if !t.trim().is_empty() => t.trim().to_uppercase(),
            _ => return None,
        };
        let target_text: String = raw_target
            .chars()
            .filter(|&ch| ch.is_whitespace() || self.font.glyph_id(ch).0 != 0)
            .collect();
        let trimmed = target_text.trim();
        if trimmed.is_empty() {
            return None;
        }
        Some(trimmed.to_string())
    }

    pub fn render_bubbles(
        &self,
        img: &mut RgbImage,
        bubbles: &[Bubble],
        translations: &HashMap<usize, String>,
    ) {
        let img_w = img.width() as f32;
        let img_h = img.height() as f32;
        let edge_margin = 4.0_f32;

        for b in bubbles {
            let target_text = match self.renderable_text(b, translations) {
                Some(text) => text,
                None => continue,
            };

            let orig_cx = if b.oriented_w > 1.0 { b.cx } else { (b.x1 + b.x2) * 0.5 };
            let orig_cy = if b.oriented_h > 1.0 { b.cy } else { (b.y1 + b.y2) * 0.5 };
            let mut center_x = orig_cx;
            let mut center_y = orig_cy;
            let base_w = if b.oriented_w > 1.0 { b.oriented_w } else { b.w };
            let base_h = if b.oriented_h > 1.0 { b.oriented_h } else { b.h };

            let mut safe_w = base_w.max(28.0);
            let mut safe_h = (base_h * 1.15).max(14.0);
            let mut has_bubble_bounds = false;
            let mut vert_bounds: Option<(f32, f32)> = None;
            if b.angle_rad.abs() < MIN_TILT_RAD {
                if let Some((max_left_border_x, min_right_border_x)) =
                    detect_horizontal_bubble_bounds(img, orig_cx, orig_cy, base_w, base_h)
                {
                    has_bubble_bounds = true;
                    let left_safe = max_left_border_x + 7.0;
                    let right_safe = min_right_border_x - 7.0;
                    let measured_w = (right_safe - left_safe).max(24.0);
                    let raw_cx = (left_safe + right_safe) * 0.5;
                    let max_shift = b.w * 0.10;
                    center_x = raw_cx.clamp(orig_cx - max_shift, orig_cx + max_shift);

                    let clamped_span =
                        (center_x - left_safe).min(right_safe - center_x).max(12.0) * 2.0;
                    let target_w = if b.h > b.w * 1.25 && measured_w > b.w {
                        measured_w.min(b.h * 0.80)
                    } else {
                        measured_w.clamp(24.0, (base_w * 1.30).max(24.0))
                    };
                    safe_w = target_w.min(clamped_span).max(24.0);

                    if let Some((top_b, bot_b)) =
                        detect_vertical_bubble_bounds(img, center_x, orig_cy, base_h)
                    {
                        let top_safe = top_b + 6.0;
                        let bot_safe = bot_b - 6.0;
                        center_y = ((top_safe + bot_safe) * 0.5)
                            .clamp(orig_cy - base_h * 0.18, orig_cy + base_h * 0.18);
                        let max_span_h =
                            ((center_y - top_safe).min(bot_safe - center_y).max(7.0) * 2.0)
                                .min(bot_safe - top_safe);
                        safe_h = safe_h.min(max_span_h).max(14.0);
                        vert_bounds = Some((top_safe, bot_safe));
                    }
                }
            }

            let words: Vec<&str> = target_text.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }

            let candidate_sizes = [
                38.0, 34.0, 30.0, 28.0, 26.0, 24.0, 22.0, 20.0, 18.0, 16.5, 15.0, 14.0, 13.0, 12.0,
                11.0, 10.5, 10.0, 9.5, 9.0, 8.5, 8.0, 7.5, 7.0, 6.5,
            ];
            let (bound_min_y, bound_max_y) = vert_bounds.unwrap_or((b.y1 - 6.0, b.y2 + 8.0));
            let mut best_size = 6.5;
            let mut best_lines: Vec<String> = Vec::new();
            for &size in &candidate_sizes {
                let scaled_font = self.font.as_scaled(PxScale::from(size));
                let lines = self.wrap_words(&scaled_font, &words, safe_w);
                let total_h = (lines.len() as f32) * (scaled_font.height() * 1.12);
                let max_w = lines.iter().map(|l| self.measure_line_width(&scaled_font, l)).fold(0.0_f32, f32::max);
                let cap_ok = if !has_bubble_bounds {
                    true
                } else {
                    let cand_start_y = compute_block_start_y(
                        center_y,
                        total_h,
                        bound_min_y,
                        bound_max_y,
                        edge_margin,
                        img_h,
                    );
                    measure_block_cap_safe_span(
                        img,
                        center_x,
                        base_w,
                        base_h,
                        cand_start_y + 2.0,
                        cand_start_y + total_h - 4.0,
                    )
                    .map_or(true, |cap_span| max_w <= cap_span)
                };
                if total_h <= safe_h && max_w <= safe_w && cap_ok {
                    best_size = size;
                    best_lines = lines;
                    break;
                }
            }
            if best_lines.is_empty() {
                let scaled_font = self.font.as_scaled(PxScale::from(best_size));
                best_lines = self.wrap_words(&scaled_font, &words, safe_w);
            }

            let scaled_font = self.font.as_scaled(PxScale::from(best_size));
            let line_height = scaled_font.height() * 1.12;
            let total_h = (best_lines.len() as f32) * line_height;
            let max_line_w = best_lines.iter().map(|l| self.measure_line_width(&scaled_font, l)).fold(0.0_f32, f32::max);
            let block_left = -(max_line_w.min(safe_w)) * 0.5;

            if b.angle_rad.abs() < MIN_TILT_RAD {
                let start_y = compute_block_start_y(
                    center_y,
                    total_h,
                    bound_min_y,
                    bound_max_y,
                    edge_margin,
                    img_h,
                );

                for (idx, line) in best_lines.iter().enumerate() {
                    let line_w = self.measure_line_width(&scaled_font, line);
                    let raw_line_x = if b.is_left_aligned {
                        center_x + block_left
                    } else {
                        center_x - line_w / 2.0
                    };
                    let line_x = raw_line_x
                        .clamp(edge_margin, (img_w - line_w - edge_margin).max(edge_margin));
                    let line_y = start_y + (idx as f32) * line_height;

                    self.draw_stroked_text(
                        RenderTarget::Rgb(img),
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

                    self.draw_stroked_text(
                        RenderTarget::Rgba(&mut overlay),
                        &scaled_font,
                        line,
                        line_x,
                        line_y,
                        Rgb([12, 12, 12]),
                        Rgb([255, 255, 255]),
                    );
                }

                let cos_a = b.angle_rad.cos().abs();
                let sin_a = b.angle_rad.sin().abs();
                let radius_x = buf_w as f32 * 0.5 * cos_a + buf_h as f32 * 0.5 * sin_a;
                let radius_y = buf_w as f32 * 0.5 * sin_a + buf_h as f32 * 0.5 * cos_a;
                let clamped_cx = center_x.clamp(
                    radius_x + edge_margin,
                    (img_w - radius_x - edge_margin).max(radius_x + edge_margin),
                );
                let clamped_cy = center_y.clamp(
                    radius_y + edge_margin,
                    (img_h - radius_y - edge_margin).max(radius_y + edge_margin),
                );
                composite_rotated_overlay(img, &overlay, clamped_cx, clamped_cy, b.angle_rad);
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

    #[allow(clippy::too_many_arguments)]
    fn draw_stroked_text(
        &self,
        mut target: RenderTarget<'_>,
        scaled_font: &PxScaleFont<&FontArc>,
        text: &str,
        x: f32,
        y: f32,
        fill_color: Rgb<u8>,
        stroke_color: Rgb<u8>,
    ) {
        let (width, height) = match &target {
            RenderTarget::Rgb(img) => (img.width() as i32, img.height() as i32),
            RenderTarget::Rgba(overlay) => (overlay.width() as i32, overlay.height() as i32),
        };
        let offsets = [
            (-1, 0), (1, 0), (0, -1), (0, 1),
            (-1, -1), (1, 1), (-1, 1), (1, -1),
        ];
        for (ox, oy) in offsets {
            self.rasterize_text(scaled_font, text, x + ox as f32, y + oy as f32, |px, py, c| {
                if c > 0.35 && px >= 0 && px < width && py >= 0 && py < height {
                    match &mut target {
                        RenderTarget::Rgb(img) => img.put_pixel(px as u32, py as u32, stroke_color),
                        RenderTarget::Rgba(ov) => ov.put_pixel(
                            px as u32,
                            py as u32,
                            Rgba([stroke_color[0], stroke_color[1], stroke_color[2], 255]),
                        ),
                    }
                }
            });
        }
        self.rasterize_text(scaled_font, text, x, y, |px, py, c| {
            if c > 0.05 && px >= 0 && px < width && py >= 0 && py < height {
                let (ux, uy) = (px as u32, py as u32);
                match &mut target {
                    RenderTarget::Rgb(img) => {
                        let cur = img.get_pixel(ux, uy);
                        let r = ((1.0 - c) * cur[0] as f32 + c * fill_color[0] as f32) as u8;
                        let g = ((1.0 - c) * cur[1] as f32 + c * fill_color[1] as f32) as u8;
                        let b = ((1.0 - c) * cur[2] as f32 + c * fill_color[2] as f32) as u8;
                        img.put_pixel(ux, uy, Rgb([r, g, b]));
                    }
                    RenderTarget::Rgba(ov) => {
                        let dst = ov.get_pixel(ux, uy);
                        if dst[3] == 0 {
                            let a = (c.clamp(0.0, 1.0) * 255.0).round() as u8;
                            ov.put_pixel(ux, uy, Rgba([fill_color[0], fill_color[1], fill_color[2], a]));
                        } else {
                            let r = ((1.0 - c) * dst[0] as f32 + c * fill_color[0] as f32).round() as u8;
                            let g = ((1.0 - c) * dst[1] as f32 + c * fill_color[1] as f32).round() as u8;
                            let b = ((1.0 - c) * dst[2] as f32 + c * fill_color[2] as f32).round() as u8;
                            ov.put_pixel(ux, uy, Rgba([r, g, b, 255]));
                        }
                    }
                }
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
#[path = "renderer_tests.rs"]
mod tests;
