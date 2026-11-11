use crate::model::Bubble;
use ab_glyph::{Font, FontArc, PxScale, PxScaleFont, ScaleFont};
use image::{Rgb, RgbImage};
use std::collections::HashMap;

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
            let target_text = match translations.get(&b.id) {
                Some(t) if !t.trim().is_empty() => t.trim().to_uppercase(),
                _ => continue,
            };

            let center_x = (b.x1 + b.x2) / 2.0;
            let center_y = (b.y1 + b.y2) / 2.0;
            let box_w = b.w.max(40.0);
            let box_h = b.h.max(25.0);

            let safe_w = (box_w * 1.25).max(70.0);
            let safe_h = (box_h * 1.25).max(35.0);

            let words: Vec<&str> = target_text.split_whitespace().collect();
            if words.is_empty() {
                continue;
            }

            let candidate_sizes = [
                38.0, 34.0, 30.0, 28.0, 26.0, 24.0, 22.0, 20.0, 18.0, 16.5, 15.0, 14.0, 13.0, 12.0, 11.0, 10.5, 10.0, 9.5, 9.0, 8.5, 8.0,
            ];

            let mut best_size = 9.0;
            let mut best_lines: Vec<String> = Vec::new();

            for &size in &candidate_sizes {
                let scale = PxScale::from(size);
                let scaled_font = self.font.as_scaled(scale);

                let mut lines = Vec::new();
                let mut cur_line = Vec::new();

                for word in &words {
                    let mut test_words = cur_line.clone();
                    test_words.push(*word);
                    let test_str = test_words.join(" ");

                    let line_w = self.measure_line_width(&scaled_font, &test_str);
                    if line_w <= safe_w {
                        cur_line.push(*word);
                    } else if !cur_line.is_empty() {
                        lines.push(cur_line.join(" "));
                        cur_line = vec![*word];
                    } else {
                        lines.push(word.to_string());
                        cur_line.clear();
                    }
                }
                if !cur_line.is_empty() {
                    lines.push(cur_line.join(" "));
                }

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
                best_size = 8.5;
                let scale = PxScale::from(best_size);
                let scaled_font = self.font.as_scaled(scale);
                let mut lines = Vec::new();
                let mut cur_line = Vec::new();

                for word in &words {
                    let mut test_words = cur_line.clone();
                    test_words.push(*word);
                    let test_str = test_words.join(" ");

                    let line_w = self.measure_line_width(&scaled_font, &test_str);
                    if line_w <= safe_w * 1.15 {
                        cur_line.push(*word);
                    } else if !cur_line.is_empty() {
                        lines.push(cur_line.join(" "));
                        cur_line = vec![*word];
                    } else {
                        lines.push(word.to_string());
                        cur_line.clear();
                    }
                }
                if !cur_line.is_empty() {
                    lines.push(cur_line.join(" "));
                }
                best_lines = lines;
            }

            let scale = PxScale::from(best_size);
            let scaled_font = self.font.as_scaled(scale);
            let line_height = scaled_font.height() * 1.12;
            let total_h = (best_lines.len() as f32) * line_height;

            let mut start_y = center_y - total_h / 2.0;
            if start_y < b.y1 - 6.0 {
                start_y = b.y1 - 6.0;
            }
            if start_y + total_h > b.y2 + 8.0 {
                start_y = (b.y2 + 8.0 - total_h).max(b.y1 - 6.0);
            }

            for (idx, line) in best_lines.iter().enumerate() {
                let line_w = self.measure_line_width(&scaled_font, line);
                let line_x = center_x - line_w / 2.0;
                let line_y = start_y + (idx as f32) * line_height;

                self.draw_stroked_text(
                    img,
                    &scaled_font,
                    line,
                    line_x,
                    line_y,
                    Rgb([12, 12, 12]),
                    Rgb([255, 255, 255]),
                );
            }
        }
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

    fn draw_stroked_text(
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
            (-1, -1), (1, 1), (-1, 1), (1, -1)
        ];

        for (ox, oy) in offsets {
            let mut pen_x = x + (ox as f32);
            let pen_y = y + (oy as f32) + scaled_font.ascent();
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
                        if c > 0.35 {
                            let px = bounds.min.x as i32 + gx as i32;
                            let py = bounds.min.y as i32 + gy as i32;
                            if px >= 0 && px < img_w && py >= 0 && py < img_h {
                                img.put_pixel(px as u32, py as u32, stroke_color);
                            }
                        }
                    });
                }

                pen_x += scaled_font.h_advance(glyph_id);
                last_glyph = Some(glyph_id);
            }
        }

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
                    if c > 0.05 {
                        let px = bounds.min.x as i32 + gx as i32;
                        let py = bounds.min.y as i32 + gy as i32;
                        if px >= 0 && px < img_w && py >= 0 && py < img_h {
                            let current = img.get_pixel(px as u32, py as u32);
                            let alpha = c;
                            let r = ((1.0 - alpha) * (current[0] as f32) + alpha * (fill_color[0] as f32)) as u8;
                            let g = ((1.0 - alpha) * (current[1] as f32) + alpha * (fill_color[1] as f32)) as u8;
                            let b = ((1.0 - alpha) * (current[2] as f32) + alpha * (fill_color[2] as f32)) as u8;
                            img.put_pixel(px as u32, py as u32, Rgb([r, g, b]));
                        }
                    }
                });
            }

            pen_x += scaled_font.h_advance(glyph_id);
            last_glyph = Some(glyph_id);
        }
    }
}
