use crate::model::OcrLineItem;
use image::RgbImage;
use rapidocr_core::config::ExecutionProvider;
use rapidocr_core::model::{ModelCache, ModelDownloadMode, PPOCRV4_EN_MOBILE, PPOCRV6_SMALL};
use rapidocr_core::RapidOcr;
use std::path::Path;
use tracing::info;

pub struct OcrManager {
    ocr: RapidOcr,
}

impl OcrManager {
    pub fn new(_port: u16) -> Result<Self, String> {
        let model_dir_str = if Path::new("models").exists() {
            "models"
        } else {
            "Server-rs/models"
        };
        let model_dir = Path::new(model_dir_str);

        info!("Initializing Native Rust RapidOCR from {}...", model_dir_str);
        let cache = ModelCache::new(model_dir);
        cache
            .ensure_model_set(&PPOCRV6_SMALL, ModelDownloadMode::Missing)
            .map_err(|e| format!("Failed to ensure OCR models: {}", e))?;
        cache
            .ensure_model_set(&PPOCRV4_EN_MOBILE, ModelDownloadMode::Missing)
            .map_err(|e| format!("Failed to ensure OCR det model: {}", e))?;

        let mut cfg = cache.config_for(&PPOCRV6_SMALL);
        cfg.pipeline.use_cls = false;
        cfg.text_score = 0.40;
        if let Some(ref mut det) = cfg.det {
            det.model_path = model_dir.join("en_PP-OCRv3_det_mobile.onnx");
            det.limit_side_len = 2800;
            det.box_thresh = 0.33;
            det.unclip_ratio = 1.70;
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
        let max_dim = orig_w.max(orig_h);
        let min_dim = orig_w.min(orig_h);

        let scale = if max_dim < 1200 {
            (1600.0 / max_dim as f32).clamp(1.2, 2.0)
        } else if min_dim < 960 && max_dim <= 2400 {
            let max_safe_scale = (2800.0 / max_dim as f32).max(1.0);
            (1024.0 / min_dim as f32).clamp(1.15, 1.5).min(max_safe_scale)
        } else {
            1.0f32
        };

        let processed = if scale > 1.0 {
            let target_w = (orig_w as f32 * scale).round() as u32;
            let target_h = (orig_h as f32 * scale).round() as u32;
            image::imageops::resize(
                img,
                target_w,
                target_h,
                image::imageops::FilterType::CatmullRom,
            )
        } else {
            img.clone()
        };

        let res = self
            .ocr
            .run_image(&processed)
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
