use crate::model::OcrLineItem;
use image::RgbImage;
use rapidocr_core::config::ExecutionProvider;
use rapidocr_core::model::{ModelCache, ModelDownloadMode, PPOCRV4_EN_MOBILE};
use rapidocr_core::RapidOcr;
use std::path::Path;
use tracing::info;

pub struct OcrManager {
    ocr: RapidOcr,
}

impl OcrManager {
    pub fn new(_port: u16) -> Result<Self, String> {
        let model_dir = if Path::new("models").exists() {
            "models"
        } else {
            "Server-rs/models"
        };

        info!("Initializing Native Rust RapidOCR from {}...", model_dir);
        let cache = ModelCache::new(model_dir);
        cache
            .ensure_model_set(&PPOCRV4_EN_MOBILE, ModelDownloadMode::Missing)
            .map_err(|e| format!("Failed to ensure OCR models: {}", e))?;

        let mut cfg = cache.config_for(&PPOCRV4_EN_MOBILE);
        cfg.pipeline.use_cls = false;
        cfg.text_score = 0.58;
        if let Some(ref mut det) = cfg.det {
            det.box_thresh = 0.55;
            det.unclip_ratio = 1.50;
        }
        #[cfg(target_os = "windows")]
        {
            cfg.inference.execution_provider = ExecutionProvider::DirectMl;
        }
        #[cfg(not(target_os = "windows"))]
        {
            cfg.inference.execution_provider = ExecutionProvider::Cpu;
        }

        let ocr = match RapidOcr::new(cfg.clone()) {
            Ok(engine) => {
                info!("Native Rust RapidOCR initialized with GPU/default acceleration.");
                engine
            }
            Err(_) => {
                info!("Primary provider unavailable, falling back to Native Rust CPU OCR.");
                cfg.inference.execution_provider = ExecutionProvider::Cpu;
                RapidOcr::new(cfg).map_err(|e| format!("Failed to initialize Native OCR: {}", e))?
            }
        };

        Ok(Self { ocr })
    }

    pub fn run_ocr(&mut self, img: &RgbImage) -> Result<Vec<OcrLineItem>, String> {
        let (orig_w, orig_h) = (img.width(), img.height());
        let scale = if orig_w < 500 && orig_h < 500 { 2.0f32 } else { 1.0f32 };

        let scaled_img;
        let target_img = if scale > 1.0 {
            scaled_img = image::imageops::resize(
                img,
                (orig_w as f32 * scale).round() as u32,
                (orig_h as f32 * scale).round() as u32,
                image::imageops::FilterType::CatmullRom,
            );
            &scaled_img
        } else {
            img
        };

        let res = self
            .ocr
            .run_image(target_img)
            .map_err(|e| format!("Native OCR execution failed: {}", e))?;

        let lines = res
            .lines
            .into_iter()
            .map(|l| {
                let scaled_box = if scale > 1.0 {
                    [
                        [l.bbox.points[0][0] / scale, l.bbox.points[0][1] / scale],
                        [l.bbox.points[1][0] / scale, l.bbox.points[1][1] / scale],
                        [l.bbox.points[2][0] / scale, l.bbox.points[2][1] / scale],
                        [l.bbox.points[3][0] / scale, l.bbox.points[3][1] / scale],
                    ]
                } else {
                    l.bbox.points
                };

                OcrLineItem {
                    r#box: scaled_box,
                    text: l.text,
                    score: l.score,
                }
            })
            .collect();

        Ok(lines)
    }
}
