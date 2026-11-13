pub mod clustering;
#[cfg(feature = "free-mode")]
pub mod free_mode;
pub mod inpainting;
pub mod llm_client;
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
use image::{ImageFormat, RgbImage};
use std::io::Cursor;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

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

        let mut img: RgbImage = image::load_from_memory(&image_bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?
            .to_rgb8();

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
            };
            return Ok((None, ImgMap { boxes: Vec::new() }, timings));
        }

        let t_tr = std::time::Instant::now();
        let translations = self
            .translator
            .translate_bubbles(&bubbles, source_lang, target_lang)
            .await?;
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
        };
        info!("Total pipeline processing time: {:?}", t_start.elapsed());
        Ok((base64_image, ImgMap { boxes }, timings))
    }
}
