pub mod clustering;
#[cfg(feature = "free-mode")]
pub mod free_mode;
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
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

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

pub struct Pipeline {
    pub ocr: Arc<Mutex<OcrManager>>,
    pub translator: Translator,
    pub renderer: Arc<ComicRenderer>,
}

impl Pipeline {
    pub fn new(
        ocr: Arc<Mutex<OcrManager>>,
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
    ) -> Result<(Option<String>, ImgMap, PipelineTimings), String> {
        let t_start = std::time::Instant::now();

        let mut img = decode_bounded_rgb_image(&image_bytes)?;

        let (width, height) = (img.width(), img.height());
        info!("Processing image {}x{}, decode in {:?}", width, height, t_start.elapsed());

        let t_ocr = std::time::Instant::now();
        let ocr_lines = {
            let mut ocr_guard = self.ocr.lock().await;
            ocr_guard.run_ocr(&img)?
        };
        let ocr_ms = t_ocr.elapsed().as_millis() as u64;
        info!("Native OCR detected {} lines in {:?}", ocr_lines.len(), t_ocr.elapsed());

        let bubbles = cluster_lines(ocr_lines, width, height, source_lang);
        info!("Clustered into {} speech bubbles", bubbles.len());

        if bubbles.is_empty() {
            let timings = PipelineTimings {
                ocr_ms,
                translate_ms: 0,
                render_ms: 0,
                total_ms: t_start.elapsed().as_millis() as u64,
                bubble_count: 0,
                ..PipelineTimings::default()
            };
            return Ok((None, ImgMap { boxes: Vec::new() }, timings));
        }

        let t_tr = std::time::Instant::now();
        let llm_output = self
            .translator
            .translate_bubbles(&bubbles, source_lang, target_lang)
            .await?;
        let translations = llm_output.translations;
        let translate_ms = t_tr.elapsed().as_millis() as u64;
        info!("Translated {} bubbles in {:?}", translations.len(), t_tr.elapsed());

        let t_render = std::time::Instant::now();
        let base64_image = if !without_image {
            clean_image_text(&mut img, &bubbles);
            self.renderer.render_bubbles(&mut img, &bubbles, &translations);

            let mut output_bytes = Vec::new();
            let mut cursor = Cursor::new(&mut output_bytes);
            img.write_to(&mut cursor, ImageFormat::WebP)
                .map_err(|e| format!("Failed to encode WebP image: {}", e))?;

            info!("Inpainting, rendering & encoding completed in {:?}", t_render.elapsed());
            Some(BASE64.encode(&output_bytes))
        } else {
            None
        };
        let render_ms = if !without_image {
            t_render.elapsed().as_millis() as u64
        } else {
            0
        };

        let mut boxes = Vec::new();
        for b in &bubbles {
            let target = translations
                .get(&b.id)
                .cloned()
                .unwrap_or_else(|| b.source_text.clone());

            let geom = Geometry {
                x: b.x1 as i32,
                y: b.y1 as i32,
                width: b.w as i32,
                height: b.h as i32,
            };

            boxes.push(BoxItem {
                text: b.source_text.clone(),
                target,
                geometry: geom.clone(),
                target_geometry: geom,
            });
        }

        let timings = PipelineTimings {
            ocr_ms,
            translate_ms,
            render_ms,
            total_ms: t_start.elapsed().as_millis() as u64,
            bubble_count: bubbles.len(),
            usage: llm_output.usage,
        };
        info!("Total pipeline processing time: {:?}", t_start.elapsed());
        Ok((base64_image, ImgMap { boxes }, timings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_decompression_bomb_before_allocating() {
        let mut png_bytes = Vec::new();
        let tiny = RgbImage::new(1, 1);
        tiny.write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
            .expect("encode 1x1 png");
        png_bytes[16..20].copy_from_slice(&20_000u32.to_be_bytes());
        png_bytes[20..24].copy_from_slice(&20_000u32.to_be_bytes());
        png_bytes[29..33].copy_from_slice(&0x6c12d16eu32.to_be_bytes());

        let err = decode_bounded_rgb_image(&png_bytes).unwrap_err();
        assert!(err.contains("exceed the maximum allowed limit"), "{}", err);
    }
}
