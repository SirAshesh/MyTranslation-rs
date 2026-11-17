pub mod bubble_recovery;
pub mod clustering;
#[cfg(feature = "free-mode")]
pub mod free_mode;
pub mod google_translate;
pub mod inpainting;
pub mod llm_client;
mod llm_wire;
pub mod local_runtime;
pub mod ocr;
pub mod prompts;
pub mod renderer;
pub mod text_cleaner;
pub mod translator;

pub use clustering::cluster_lines;
pub use inpainting::clean_image_text;
pub use ocr::OcrManager;
pub use renderer::ComicRenderer;
pub use translator::Translator;

use crate::model::{BoxItem, Geometry, ImgMap};
use crate::utilities::PipelineTimings;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::{ImageFormat, ImageReader, Limits, RgbImage};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use tracing::{info, warn};

const MAX_IMAGE_WIDTH: u32 = 16_384;
const MAX_IMAGE_HEIGHT: u32 = 16_384;
const MAX_IMAGE_PIXELS: u64 = 40_000_000;
const MAX_DECODE_ALLOC_BYTES: u64 = 256 * 1024 * 1024;

fn decode_bounded_rgb_image(image_bytes: &[u8]) -> Result<RgbImage, String> {
    let dim_reader = ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|e| format!("Failed to inspect image format: {}", e))?;
    let (width, height) = dim_reader
        .into_dimensions()
        .map_err(|e| format!("Failed to read image dimensions: {}", e))?;

    if width == 0 || height == 0 {
        return Err("Image dimensions must be greater than zero.".to_string());
    }
    let pixels = (width as u64).saturating_mul(height as u64);
    if width > MAX_IMAGE_WIDTH || height > MAX_IMAGE_HEIGHT || pixels > MAX_IMAGE_PIXELS {
        return Err(format!(
            "Image dimensions {}x{} ({} MP) exceed the maximum allowed limit ({}x{}, {} MP).",
            width,
            height,
            pixels / 1_000_000,
            MAX_IMAGE_WIDTH,
            MAX_IMAGE_HEIGHT,
            MAX_IMAGE_PIXELS / 1_000_000
        ));
    }

    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_WIDTH);
    limits.max_image_height = Some(MAX_IMAGE_HEIGHT);
    limits.max_alloc = Some(MAX_DECODE_ALLOC_BYTES);

    let mut decode_reader = ImageReader::new(Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image format: {}", e))?;
    decode_reader.limits(limits);

    Ok(decode_reader
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?
        .to_rgb8())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    Full,
    CleanOnly,
    CleanWithText,
}

impl RenderMode {
    pub fn from_form_value(raw: Option<&str>) -> Self {
        match raw.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
            Some("clean_only") | Some("clean") => Self::CleanOnly,
            Some("clean_with_text") | Some("clean_and_text") | Some("editor") => {
                Self::CleanWithText
            }
            _ => Self::Full,
        }
    }
}

pub struct Pipeline {
    pub ocr: Arc<OcrManager>,
    pub translator: Translator,
    pub renderer: Arc<ComicRenderer>,
}

impl Pipeline {
    pub fn new(
        ocr: Arc<OcrManager>,
        translator: Translator,
        renderer: Arc<ComicRenderer>,
    ) -> Self {
        Self {
            ocr,
            translator,
            renderer,
        }
    }

    pub async fn process_image(
        &self,
        image_bytes: Vec<u8>,
        source_lang: &str,
        target_lang: &str,
        without_image: bool,
        render_mode: RenderMode,
    ) -> Result<(Option<String>, ImgMap, PipelineTimings), String> {
        let t_start = std::time::Instant::now();

        let ocr_arc = Arc::clone(&self.ocr);
        let source_lang_owned = source_lang.to_string();
        let (img, width, height, ocr_ms, effective_source_lang, bubbles) =
            tokio::task::spawn_blocking(move || -> Result<_, String> {
                let img = decode_bounded_rgb_image(&image_bytes)?;
                let (width, height) = (img.width(), img.height());
                info!(
                    "Processing image {}x{}, decode in {:?}",
                    width,
                    height,
                    t_start.elapsed()
                );

                let t_ocr = std::time::Instant::now();
                let ocr_lines = ocr_arc.run_ocr(&img)?;
                let ocr_ms = t_ocr.elapsed().as_millis() as u64;
                info!(
                    "Native OCR detected {} lines in {:?}",
                    ocr_lines.len(),
                    t_ocr.elapsed()
                );

                let effective_source_lang =
                    clustering::detect_dominant_source_lang(&ocr_lines, &source_lang_owned)
                        .to_string();
                if effective_source_lang != source_lang_owned {
                    info!(
                        "Resolved source language '{}' -> '{}'",
                        source_lang_owned, effective_source_lang
                    );
                }

                let mut bubbles =
                    cluster_lines(ocr_lines.clone(), width, height, &effective_source_lang);
                bubble_recovery::recover_missed_bubble_text(
                    &img,
                    ocr_arc.as_ref(),
                    &ocr_lines,
                    &mut bubbles,
                    &effective_source_lang,
                );
                info!("Clustered into {} speech bubbles", bubbles.len());

                Ok((img, width, height, ocr_ms, effective_source_lang, bubbles))
            })
            .await
            .map_err(|e| format!("Image decode/OCR task failed: {}", e))??;

        if bubbles.is_empty() {
            let encoded = if !without_image {
                let encoded = tokio::task::spawn_blocking(move || encode_webp_base64(&img, width, height))
                    .await
                    .map_err(|e| format!("Image encode task failed: {}", e))??;
                Some(encoded)
            } else {
                None
            };
            let timings = PipelineTimings {
                ocr_ms,
                translate_ms: 0,
                render_ms: 0,
                total_ms: t_start.elapsed().as_millis() as u64,
                bubble_count: 0,
                ..PipelineTimings::default()
            };
            return Ok((encoded, ImgMap { boxes: Vec::new() }, timings));
        }

        let (translations, translate_ms, usage) = if render_mode == RenderMode::CleanOnly {
            (Arc::new(HashMap::new()), 0, Default::default())
        } else {
            let t_tr = std::time::Instant::now();
            let llm_output = self
                .translator
                .translate_bubbles(&bubbles, &effective_source_lang, target_lang)
                .await?;
            let translations = Arc::new(llm_output.translations);
            let translate_ms = t_tr.elapsed().as_millis() as u64;
            info!(
                "Translated {} bubbles in {:?}",
                translations.len(),
                t_tr.elapsed()
            );
            (translations, translate_ms, llm_output.usage)
        };

        let t_render = std::time::Instant::now();
        let (base64_image, resolved_geoms) = if !without_image {
            let renderable_bubbles: Vec<_> = if render_mode == RenderMode::CleanOnly {
                bubbles.clone()
            } else {
                let filtered: Vec<_> = bubbles
                    .iter()
                    .filter(|b| self.renderer.renderable_text(b, &translations).is_some())
                    .cloned()
                    .collect();
                if filtered.len() < bubbles.len() {
                    warn!(
                        "Skipping inpainting on {} bubble(s) without renderable translation text ({}/{} renderable)",
                        bubbles.len() - filtered.len(),
                        filtered.len(),
                        bubbles.len()
                    );
                }
                filtered
            };

            let renderer = Arc::clone(&self.renderer);
            let translations_for_render = Arc::clone(&translations);
            let bubbles_for_geom = bubbles.clone();
            let draw_text = render_mode == RenderMode::Full;
            let (encoded, resolved_geoms) =
                tokio::task::spawn_blocking(move || -> Result<(String, HashMap<usize, Geometry>), String> {
                    let mut img = img;
                    let mut resolved_geoms = HashMap::with_capacity(bubbles_for_geom.len());
                    let (inpaint_elapsed, render_elapsed) = if !renderable_bubbles.is_empty() {
                        let t_inpaint = std::time::Instant::now();
                        clean_image_text(&mut img, &renderable_bubbles);
                        let inpaint_elapsed = t_inpaint.elapsed();

                        for b in &bubbles_for_geom {
                            resolved_geoms.insert(
                                b.id,
                                renderer::resolve_bubble_interior_rect(&img, b, &bubbles_for_geom),
                            );
                        }

                        let render_elapsed = if draw_text {
                            let t_draw = std::time::Instant::now();
                            renderer.render_bubbles(
                                &mut img,
                                &renderable_bubbles,
                                &translations_for_render,
                            );
                            t_draw.elapsed()
                        } else {
                            std::time::Duration::ZERO
                        };

                        (inpaint_elapsed, render_elapsed)
                    } else {
                        (std::time::Duration::ZERO, std::time::Duration::ZERO)
                    };

                    let t_encode = std::time::Instant::now();
                    let encoded = encode_webp_base64(&img, width, height)?;
                    let encode_elapsed = t_encode.elapsed();

                    info!(
                        "Inpainting, rendering & encoding completed in {:?} (inpaint: {:?}, render: {:?}, encode: {:?})",
                        t_render.elapsed(),
                        inpaint_elapsed,
                        render_elapsed,
                        encode_elapsed
                    );
                    Ok((encoded, resolved_geoms))
                })
                .await
                .map_err(|e| format!("Image render/encode task failed: {}", e))??;

            (Some(encoded), resolved_geoms)
        } else {
            (None, HashMap::new())
        };
        let render_ms = if !without_image {
            t_render.elapsed().as_millis() as u64
        } else {
            0
        };

        let mut boxes = Vec::new();
        for b in &bubbles {
            let target = if render_mode == RenderMode::CleanOnly {
                String::new()
            } else {
                translations
                    .get(&b.id)
                    .cloned()
                    .unwrap_or_else(|| b.source_text.clone())
            };

            let geom = Geometry {
                x: b.x1 as i32,
                y: b.y1 as i32,
                width: b.w as i32,
                height: b.h as i32,
            };
            let target_geometry = resolved_geoms.get(&b.id).cloned().unwrap_or_else(|| geom.clone());

            boxes.push(BoxItem {
                text: b.source_text.clone(),
                target,
                geometry: geom,
                target_geometry,
                angle_rad: Some(b.angle_rad),
                is_left_aligned: Some(b.is_left_aligned),
            });
        }

        let timings = PipelineTimings {
            ocr_ms,
            translate_ms,
            render_ms,
            total_ms: t_start.elapsed().as_millis() as u64,
            bubble_count: bubbles.len(),
            usage,
        };
        info!("Total pipeline processing time: {:?}", t_start.elapsed());
        Ok((base64_image, ImgMap { boxes }, timings))
    }
}

fn encode_webp_base64(img: &RgbImage, width: u32, height: u32) -> Result<String, String> {
    let mut output_bytes =
        Vec::with_capacity(((width as usize) * (height as usize)).clamp(16_384, 4 * 1024 * 1024));
    let mut cursor = Cursor::new(&mut output_bytes);
    img.write_to(&mut cursor, ImageFormat::WebP)
        .map_err(|e| format!("Failed to encode WebP image: {}", e))?;
    Ok(BASE64.encode(&output_bytes))
}

#[cfg(test)]
mod tests {
    use super::RenderMode;

    #[test]
    fn render_mode_parses_extension_modes() {
        assert_eq!(RenderMode::from_form_value(None), RenderMode::Full);
        assert_eq!(RenderMode::from_form_value(Some("full")), RenderMode::Full);
        assert_eq!(
            RenderMode::from_form_value(Some("clean_only")),
            RenderMode::CleanOnly
        );
        assert_eq!(
            RenderMode::from_form_value(Some(" CLEAN ")),
            RenderMode::CleanOnly
        );
        assert_eq!(
            RenderMode::from_form_value(Some("clean_with_text")),
            RenderMode::CleanWithText
        );
        assert_eq!(
            RenderMode::from_form_value(Some("editor")),
            RenderMode::CleanWithText
        );
    }
}
