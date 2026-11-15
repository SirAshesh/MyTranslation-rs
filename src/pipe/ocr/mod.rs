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
use rayon::prelude::*;
use std::borrow::Cow;
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Mutex, TryLockError,
};
use tracing::{info, warn};

pub struct OcrManager {
    engines: Vec<Mutex<RapidOcr>>,
    next_engine: AtomicUsize,
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

        let (primary_engine, max_det_side, max_det_pixels) = match RapidOcr::new(cfg.clone()) {
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
                let engine = RapidOcr::new(cfg.clone())
                    .map_err(|e| format!("Failed to initialize Native OCR: {}", e))?;
                info!("Native Rust CPU OCR initialized (intra_threads={}).", threads);
                (engine, max_side, max_pixels)
            }
        };

        let mut engines = Vec::with_capacity(2);
        engines.push(Mutex::new(primary_engine));
        match RapidOcr::new(cfg) {
            Ok(secondary_engine) => {
                engines.push(Mutex::new(secondary_engine));
                info!("OCR engine pool initialized with {} engines.", engines.len());
            }
            Err(err) => {
                warn!(
                    "Secondary OCR engine unavailable, continuing with single engine: {}",
                    err
                );
            }
        }

        Ok(Self {
            engines,
            next_engine: AtomicUsize::new(0),
            max_det_side,
            max_det_pixels,
        })
    }

    fn detect_tile_lines(
        &self,
        tile_img: &RgbImage,
        scale: f32,
        y_offset: u32,
    ) -> Result<Vec<OcrLineItem>, String> {
        let len = self.engines.len();
        if len == 0 {
            return Err("OCR engine pool is empty".to_string());
        }
        let start = self.next_engine.fetch_add(1, Ordering::Relaxed) % len;
        let mut engine_guard = None;
        for offset in 0..len {
            let idx = (start + offset) % len;
            match self.engines[idx].try_lock() {
                Ok(guard) => {
                    engine_guard = Some(guard);
                    break;
                }
                Err(TryLockError::Poisoned(poisoned)) => {
                    engine_guard = Some(poisoned.into_inner());
                    break;
                }
                Err(TryLockError::WouldBlock) => continue,
            }
        }

        let res = {
            let mut engine = match engine_guard {
                Some(guard) => guard,
                None => self.engines[start]
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()),
            };
            engine
                .run_image(tile_img)
                .map_err(|e| format!("Native OCR execution failed: {}", e))?
        };

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

    pub fn run_ocr(&self, img: &RgbImage) -> Result<Vec<OcrLineItem>, String> {
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
        let tile_batches: Vec<Vec<TileDetection>> = spans
            .par_iter()
            .enumerate()
            .map(|(tile_idx, span)| -> Result<Vec<TileDetection>, String> {
                let tile_h = span.y_end - span.y_start;
                let scale =
                    compute_tile_scale(orig_w, tile_h, self.max_det_side, self.max_det_pixels);
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
                let mut tile_detections = Vec::with_capacity(tile_lines.len());
                for item in tile_lines {
                    let bounds = box_bounds(&item.r#box);
                    let edge_clearance = interior_edge_clearance(&bounds, *span, orig_h);
                    tile_detections.push(TileDetection {
                        item,
                        bounds,
                        tile_idx,
                        edge_clearance,
                    });
                }
                Ok(tile_detections)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let detections: Vec<TileDetection> = tile_batches.into_iter().flatten().collect();
        Ok(deduplicate_seam_detections(detections))
    }

    pub fn run_ocr_region(
        &self,
        img: &RgbImage,
        x1: u32,
        y1: u32,
        x2: u32,
        y2: u32,
    ) -> Result<Vec<OcrLineItem>, String> {
        let clamped_x1 = x1.min(img.width());
        let clamped_y1 = y1.min(img.height());
        let clamped_x2 = x2.min(img.width());
        let clamped_y2 = y2.min(img.height());
        let crop_w = clamped_x2.saturating_sub(clamped_x1);
        let crop_h = clamped_y2.saturating_sub(clamped_y1);
        if crop_w < 8 || crop_h < 8 {
            return Ok(Vec::new());
        }

        let crop_view = image::imageops::crop_imm(img, clamped_x1, clamped_y1, crop_w, crop_h);
        let min_dim = crop_w.min(crop_h) as f32;
        let max_dim = crop_w.max(crop_h) as f32;
        let raw_scale = if min_dim < 420.0 {
            (420.0 / min_dim).clamp(1.6, 2.5)
        } else {
            1.0
        };
        let scale = raw_scale.min(1280.0 / max_dim).max(1.0);

        let crop_img = if scale > 1.05 {
            let target_w = ((crop_w as f32 * scale).round() as u32).max(1);
            let target_h = ((crop_h as f32 * scale).round() as u32).max(1);
            image::imageops::resize(
                &*crop_view,
                target_w,
                target_h,
                image::imageops::FilterType::Triangle,
            )
        } else {
            crop_view.to_image()
        };

        let effective_scale = if scale > 1.05 { scale } else { 1.0 };
        let mut lines = self.detect_tile_lines(&crop_img, effective_scale, clamped_y1)?;
        for item in &mut lines {
            for point in &mut item.r#box {
                point[0] += clamped_x1 as f32;
            }
        }
        Ok(lines)
    }
}
