#[cfg(test)]
mod tests;
mod tiling;

use self::tiling::{
    box_bounds, compute_tile_scale, compute_vertical_tile_spans, deduplicate_seam_detections,
    interior_edge_clearance, should_tile_vertically, translate_tile_box, TileDetection,
    VERTICAL_TILE_HEIGHT, VERTICAL_TILE_OVERLAP,
};
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
    max_det_side: u32,
    max_det_pixels: u64,
}

fn cpu_intra_threads() -> usize {
    std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 4)
}

fn det_limits_from_config(cfg: &RapidOcrConfig) -> (u32, u64) {
    let det = cfg.det.as_ref();
    let max_side = det
        .and_then(|d| d.input_limits.max_side_len)
        .unwrap_or(2560) as u32;
    let max_pixels = det
        .and_then(|d| d.input_limits.max_pixels)
        .unwrap_or(1440 * 1440) as u64;
    (max_side, max_pixels)
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

        let (ocr, max_det_side, max_det_pixels) = match RapidOcr::new(cfg.clone()) {
            Ok(engine) => {
                let (max_side, max_pixels) = det_limits_from_config(&cfg);
                info!(
                    "Native Rust RapidOCR initialized (cpu_mode={}, intra_threads={}).",
                    prefer_cpu, cfg.inference.intra_threads
                );
                (engine, max_side, max_pixels)
            }
            Err(_) => {
                info!("Primary provider unavailable, falling back to Native Rust CPU OCR.");
                configure_ocr_profile(&mut cfg, model_dir, true);
                let threads = cfg.inference.intra_threads;
                let (max_side, max_pixels) = det_limits_from_config(&cfg);
                let engine = RapidOcr::new(cfg)
                    .map_err(|e| format!("Failed to initialize Native OCR: {}", e))?;
                info!("Native Rust CPU OCR initialized (intra_threads={}).", threads);
                (engine, max_side, max_pixels)
            }
        };

        Ok(Self {
            ocr,
            max_det_side,
            max_det_pixels,
        })
    }

    fn detect_tile_lines(
        &mut self,
        tile_img: &RgbImage,
        scale: f32,
        y_offset: u32,
    ) -> Result<Vec<OcrLineItem>, String> {
        let res = self
            .ocr
            .run_image(tile_img)
            .map_err(|e| format!("Native OCR execution failed: {}", e))?;

        Ok(res
            .lines
            .into_iter()
            .map(|l| OcrLineItem {
                r#box: translate_tile_box(l.bbox.points, scale, y_offset),
                text: l.text,
                score: l.score,
            })
            .collect())
    }

    pub fn run_ocr(&mut self, img: &RgbImage) -> Result<Vec<OcrLineItem>, String> {
        let (orig_w, orig_h) = (img.width(), img.height());
        if orig_w == 0 || orig_h == 0 {
            return Ok(Vec::new());
        }

        if !should_tile_vertically(orig_w, orig_h) {
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

            return self.detect_tile_lines(processed.as_ref(), scale, 0);
        }

        let spans = compute_vertical_tile_spans(orig_h, VERTICAL_TILE_HEIGHT, VERTICAL_TILE_OVERLAP);
        let mut detections = Vec::new();

        for (tile_idx, span) in spans.iter().enumerate() {
            let tile_h = span.y_end - span.y_start;
            let scale = compute_tile_scale(orig_w, tile_h, self.max_det_side, self.max_det_pixels);
            let tile_view = image::imageops::crop_imm(img, 0, span.y_start, orig_w, tile_h);

            let tile_img = if scale > 1.0 {
                let target_w = (orig_w as f32 * scale).round() as u32;
                let target_h = (tile_h as f32 * scale).round() as u32;
                image::imageops::resize(
                    &*tile_view,
                    target_w,
                    target_h,
                    image::imageops::FilterType::Triangle,
                )
            } else {
                tile_view.to_image()
            };

            let tile_lines = self.detect_tile_lines(&tile_img, scale, span.y_start)?;
            for item in tile_lines {
                let bounds = box_bounds(&item.r#box);
                let edge_clearance = interior_edge_clearance(&bounds, *span, orig_h);
                detections.push(TileDetection {
                    item,
                    bounds,
                    tile_idx,
                    edge_clearance,
                });
            }
        }

        Ok(deduplicate_seam_detections(detections))
    }
}
