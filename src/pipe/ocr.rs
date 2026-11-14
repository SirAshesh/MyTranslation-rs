use crate::model::OcrLineItem;
use image::RgbImage;
use rapidocr_core::config::{
    DetInputLimits, DetOverflowBehavior, ExecutionProvider, LimitType, RapidOcrConfig,
};
use rapidocr_core::model::{ModelCache, ModelDownloadMode, PPOCRV4_EN_MOBILE, PPOCRV6_SMALL};
use rapidocr_core::RapidOcr;
use std::borrow::Cow;
use std::path::Path;
use tracing::info;

pub struct OcrManager {
    ocr: RapidOcr,
}

fn cpu_intra_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 4)
}

fn configure_ocr_profile(cfg: &mut RapidOcrConfig, model_dir: &Path, cpu_mode: bool) {
    cfg.pipeline.use_cls = false;
    cfg.text_score = 0.40;
    if let Some(ref mut det) = cfg.det {
        det.model_path = model_dir.join("en_PP-OCRv3_det_mobile.onnx");
        det.limit_type = LimitType::Min;
        det.box_thresh = 0.33;
        det.unclip_ratio = 1.70;
        if cpu_mode {
            det.limit_side_len = 736;
            det.input_limits = DetInputLimits {
                max_side_len: Some(2560),
                max_pixels: Some(1440 * 1440),
                overflow_behavior: DetOverflowBehavior::Downscale,
            };
        } else {
            det.limit_side_len = 960;
            det.input_limits = DetInputLimits {
                max_side_len: Some(3200),
                max_pixels: Some(2048 * 2048),
                overflow_behavior: DetOverflowBehavior::Downscale,
            };
        }
    }

    if let Some(ref mut rec) = cfg.rec {
        rec.batch_size = if cpu_mode { 16 } else { 8 };
    }

    if cpu_mode {
        cfg.inference.execution_provider = ExecutionProvider::Cpu;
        cfg.inference.intra_threads = cpu_intra_threads();
        cfg.inference.inter_threads = 1;
        cfg.inference.parallel_execution = false;
        cfg.inference.enable_cpu_mem_arena = true;
    } else {
        cfg.inference.execution_provider = ExecutionProvider::DirectMl;
        cfg.inference.intra_threads = 1;
        cfg.inference.inter_threads = 1;
        cfg.inference.parallel_execution = false;
        cfg.inference.enable_cpu_mem_arena = true;
    }
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
        let prefer_cpu = !cfg!(target_os = "windows");
        configure_ocr_profile(&mut cfg, model_dir, prefer_cpu);

        let ocr = match RapidOcr::new(cfg.clone()) {
            Ok(engine) => {
                info!(
                    "Native Rust RapidOCR initialized (cpu_mode={}, intra_threads={}).",
                    prefer_cpu, cfg.inference.intra_threads
                );
                engine
            }
            Err(_) => {
                info!("Primary provider unavailable, falling back to Native Rust CPU OCR.");
                configure_ocr_profile(&mut cfg, model_dir, true);
                let threads = cfg.inference.intra_threads;
                let engine = RapidOcr::new(cfg)
                    .map_err(|e| format!("Failed to initialize Native OCR: {}", e))?;
                info!("Native Rust CPU OCR initialized (intra_threads={}).", threads);
                engine
            }
        };

        Ok(Self { ocr })
    }

    pub fn run_ocr(&mut self, img: &RgbImage) -> Result<Vec<OcrLineItem>, String> {
        let (orig_w, orig_h) = (img.width(), img.height());
        let max_dim = orig_w.max(orig_h);

        let scale = if max_dim < 736 {
            (960.0 / max_dim as f32).clamp(1.2, 1.8)
        } else {
            1.0f32
        };

        let processed: Cow<'_, RgbImage> = if scale > 1.0 {
            let target_w = (orig_w as f32 * scale).round() as u32;
            let target_h = (orig_h as f32 * scale).round() as u32;
            Cow::Owned(image::imageops::resize(
                img,
                target_w,
                target_h,
                image::imageops::FilterType::Triangle,
            ))
        } else {
            Cow::Borrowed(img)
        };

        let res = self
            .ocr
            .run_image(processed.as_ref())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configures_cpu_profile_for_multicore_and_bounded_det_pixels() {
        let cache = ModelCache::new("models");
        let mut cfg = cache.config_for(&PPOCRV6_SMALL);
        configure_ocr_profile(&mut cfg, Path::new("models"), true);

        assert_eq!(cfg.inference.execution_provider, ExecutionProvider::Cpu);
        assert!(cfg.inference.intra_threads >= 1 && cfg.inference.intra_threads <= 4);
        assert!(cfg.inference.enable_cpu_mem_arena);
        let det = cfg.det.as_ref().expect("det config present");
        assert_eq!(det.limit_type, LimitType::Min);
        assert_eq!(det.limit_side_len, 736);
        assert_eq!(det.input_limits.max_pixels, Some(1440 * 1440));
        assert!(cfg.validate().is_ok());
    }
}
