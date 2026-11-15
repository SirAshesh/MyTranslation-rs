use crate::model::OcrLineItem;

pub(super) const VERTICAL_TILE_TRIGGER_HEIGHT: u32 = 2048;
pub(super) const VERTICAL_TILE_ASPECT_TRIGGER_HEIGHT: u32 = 1800;
pub(super) const VERTICAL_TILE_HEIGHT: u32 = 1920;
pub(super) const VERTICAL_TILE_OVERLAP: u32 = 256;
const SEAM_SAFE_CLEARANCE_PX: f32 = 32.0;
const SEAM_MIN_VERTICAL_OVERLAP_RATIO: f32 = 0.45;
const SEAM_MIN_HORIZONTAL_OVERLAP_RATIO: f32 = 0.40;
const CONTAINMENT_TOLERANCE_PX: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TileSpan {
    pub(super) y_start: u32,
    pub(super) y_end: u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct BoxBounds {
    pub(super) min_x: f32,
    pub(super) max_x: f32,
    pub(super) min_y: f32,
    pub(super) max_y: f32,
    pub(super) width: f32,
    pub(super) height: f32,
    pub(super) area: f32,
}

#[derive(Debug, Clone)]
pub(super) struct TileDetection {
    pub(super) item: OcrLineItem,
    pub(super) bounds: BoxBounds,
    pub(super) tile_idx: usize,
    pub(super) edge_clearance: f32,
}

pub(super) fn should_tile_vertically(orig_w: u32, orig_h: u32) -> bool {
    orig_h > VERTICAL_TILE_TRIGGER_HEIGHT
        || (orig_h > orig_w.saturating_mul(2) && orig_h > VERTICAL_TILE_ASPECT_TRIGGER_HEIGHT)
}

pub(super) fn compute_vertical_tile_spans(
    img_height: u32,
    tile_h: u32,
    overlap: u32,
) -> Vec<TileSpan> {
    if img_height == 0 {
        return Vec::new();
    }
    if img_height <= tile_h {
        return vec![TileSpan {
            y_start: 0,
            y_end: img_height,
        }];
    }
    let step = tile_h.saturating_sub(overlap).max(1);
    let mut spans = Vec::new();
    let mut y_start = 0u32;
    loop {
        let y_end = y_start.saturating_add(tile_h).min(img_height);
        spans.push(TileSpan { y_start, y_end });
        if y_end >= img_height {
            break;
        }
        y_start = y_start.saturating_add(step);
    }
    spans
}

pub(super) fn compute_tile_scale(
    orig_w: u32,
    tile_h: u32,
    max_det_side: u32,
    max_det_pixels: u64,
) -> f32 {
    if orig_w >= 736 {
        return 1.0;
    }
    let desired = (960.0 / orig_w.max(1) as f32).clamp(1.2, 1.8);
    let max_dim = orig_w.max(tile_h).max(1) as f32;
    let side_cap = max_det_side as f32 / max_dim;
    let tile_pixels = orig_w.max(1) as f64 * tile_h.max(1) as f64;
    let pixel_cap = (max_det_pixels as f64 / tile_pixels).sqrt() as f32;
    let capped = desired.min(side_cap).min(pixel_cap);
    if capped > 1.05 {
        capped
    } else {
        1.0
    }
}

pub(super) fn translate_tile_box(
    points: [[f32; 2]; 4],
    scale: f32,
    y_offset: u32,
) -> [[f32; 2]; 4] {
    let inv_scale = if scale > 1.0 { 1.0 / scale } else { 1.0 };
    let y_shift = y_offset as f32;
    [
        [points[0][0] * inv_scale, points[0][1] * inv_scale + y_shift],
        [points[1][0] * inv_scale, points[1][1] * inv_scale + y_shift],
        [points[2][0] * inv_scale, points[2][1] * inv_scale + y_shift],
        [points[3][0] * inv_scale, points[3][1] * inv_scale + y_shift],
    ]
}

pub(super) fn box_bounds(quad: &[[f32; 2]; 4]) -> BoxBounds {
    let min_x = quad.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
    let max_x = quad.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
    let min_y = quad.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
    let max_y = quad.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);
    let width = (max_x - min_x).max(0.0);
    let height = (max_y - min_y).max(0.0);
    BoxBounds {
        min_x,
        max_x,
        min_y,
        max_y,
        width,
        height,
        area: width * height,
    }
}

pub(super) fn interior_edge_clearance(bounds: &BoxBounds, span: TileSpan, img_height: u32) -> f32 {
    let top_clearance = if span.y_start > 0 {
        (bounds.min_y - span.y_start as f32).max(0.0)
    } else {
        f32::INFINITY
    };
    let bottom_clearance = if span.y_end < img_height {
        (span.y_end as f32 - bounds.max_y).max(0.0)
    } else {
        f32::INFINITY
    };
    top_clearance.min(bottom_clearance)
}

fn contains_with_tolerance(outer: &BoxBounds, inner: &BoxBounds) -> bool {
    outer.min_x <= inner.min_x + CONTAINMENT_TOLERANCE_PX
        && outer.max_x >= inner.max_x - CONTAINMENT_TOLERANCE_PX
        && outer.min_y <= inner.min_y + CONTAINMENT_TOLERANCE_PX
        && outer.max_y >= inner.max_y - CONTAINMENT_TOLERANCE_PX
}

fn boxes_overlap_for_dedup(a: &BoxBounds, b: &BoxBounds) -> bool {
    let overlap_x = (a.max_x.min(b.max_x) - a.min_x.max(b.min_x)).max(0.0);
    let overlap_y = (a.max_y.min(b.max_y) - a.min_y.max(b.min_y)).max(0.0);
    if overlap_x <= 0.0 || overlap_y <= 0.0 {
        return false;
    }

    if contains_with_tolerance(a, b) || contains_with_tolerance(b, a) {
        return true;
    }

    let min_w = a.width.min(b.width);
    let min_h = a.height.min(b.height);
    if min_w <= 0.0 || min_h <= 0.0 {
        return false;
    }

    let x_ratio = overlap_x / min_w;
    let y_ratio = overlap_y / min_h;
    y_ratio >= SEAM_MIN_VERTICAL_OVERLAP_RATIO && x_ratio >= SEAM_MIN_HORIZONTAL_OVERLAP_RATIO
}

fn compare_detection_priority(a: &TileDetection, b: &TileDetection) -> std::cmp::Ordering {
    let a_safe = a.edge_clearance.min(SEAM_SAFE_CLEARANCE_PX);
    let b_safe = b.edge_clearance.min(SEAM_SAFE_CLEARANCE_PX);
    b_safe
        .total_cmp(&a_safe)
        .then_with(|| b.item.score.total_cmp(&a.item.score))
        .then_with(|| b.bounds.area.total_cmp(&a.bounds.area))
        .then_with(|| b.edge_clearance.total_cmp(&a.edge_clearance))
}

pub(super) fn deduplicate_seam_detections(mut detections: Vec<TileDetection>) -> Vec<OcrLineItem> {
    detections.sort_by(compare_detection_priority);

    let mut kept: Vec<TileDetection> = Vec::with_capacity(detections.len());
    for candidate in detections {
        let is_duplicate = kept.iter().any(|existing| {
            existing.tile_idx != candidate.tile_idx
                && boxes_overlap_for_dedup(&existing.bounds, &candidate.bounds)
        });
        if !is_duplicate {
            kept.push(candidate);
        }
    }

    kept.sort_by(|a, b| {
        a.bounds
            .min_y
            .total_cmp(&b.bounds.min_y)
            .then_with(|| a.bounds.min_x.total_cmp(&b.bounds.min_x))
    });

    kept.into_iter().map(|d| d.item).collect()
}
