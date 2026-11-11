use crate::models::OcrLineItem;
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
        cfg.inference.execution_provider = ExecutionProvider::DirectMl;

        let ocr = match RapidOcr::new(cfg.clone()) {
            Ok(engine) => {
                info!("Native Rust RapidOCR initialized with DirectML GPU acceleration.");
                engine
            }
            Err(_) => {
                info!("DirectML unavailable, falling back to Native Rust CPU OCR.");
                cfg.inference.execution_provider = ExecutionProvider::Cpu;
                RapidOcr::new(cfg).map_err(|e| format!("Failed to initialize Native OCR: {}", e))?
            }
        };

        Ok(Self { ocr })
    }

    pub fn run_ocr(&mut self, img: &RgbImage) -> Result<Vec<OcrLineItem>, String> {
        let res = self
            .ocr
            .run_image(img)
            .map_err(|e| format!("Native OCR execution failed: {}", e))?;

        let lines = res
            .lines
            .into_iter()
            .map(|l| OcrLineItem {
                r#box: l.bbox.points,
                text: l.text,
                score: l.score,
            })
            .collect();

        Ok(lines)
    }
}
