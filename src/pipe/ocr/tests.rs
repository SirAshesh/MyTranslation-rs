use super::tiling::{
    box_bounds, compute_tile_scale, compute_vertical_tile_spans, deduplicate_seam_detections,
    interior_edge_clearance, should_tile_vertically, translate_tile_box, TileDetection, TileSpan,
};
use super::*;

fn make_detection(
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    text: &str,
    score: f32,
    tile_idx: usize,
    span: TileSpan,
    img_height: u32,
) -> TileDetection {
    let quad = [[x1, y1], [x2, y1], [x2, y2], [x1, y2]];
    let bounds = box_bounds(&quad);
    let edge_clearance = interior_edge_clearance(&bounds, span, img_height);
    TileDetection {
        item: OcrLineItem {
            r#box: quad,
            text: text.to_string(),
            score,
        },
        bounds,
        tile_idx,
        edge_clearance,
    }
}

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

#[test]
fn triggers_tiling_for_tall_strips_and_preserves_normal_pages() {
    assert!(!should_tile_vertically(800, 1200));
    assert!(!should_tile_vertically(720, 1202));
    assert!(should_tile_vertically(720, 1900));
    assert!(should_tile_vertically(720, 13499));
    assert!(should_tile_vertically(1400, 2100));
}

#[test]
fn computes_vertical_tile_spans_with_overlap_and_clamped_bottom() {
    let spans = compute_vertical_tile_spans(5000, 1920, 256);
    assert_eq!(
        spans,
        vec![
            TileSpan {
                y_start: 0,
                y_end: 1920
            },
            TileSpan {
                y_start: 1664,
                y_end: 3584
            },
            TileSpan {
                y_start: 3328,
                y_end: 5000
            },
        ]
    );

    let short_spans = compute_vertical_tile_spans(1202, 1920, 256);
    assert_eq!(
        short_spans,
        vec![TileSpan {
            y_start: 0,
            y_end: 1202
        }]
    );
}

#[test]
fn computes_tile_scale_within_det_limits() {
    let dml_scale = compute_tile_scale(720, 1920, 3200, 2048 * 2048);
    assert!((dml_scale - 960.0 / 720.0).abs() < 1e-4);

    let cpu_scale = compute_tile_scale(720, 1920, 2560, 1440 * 1440);
    let scaled_pixels = (720.0 * cpu_scale).round() * (1920.0 * cpu_scale).round();
    assert!(cpu_scale > 1.2);
    assert!(scaled_pixels <= (1440 * 1440) as f32 + 5000.0);

    assert_eq!(compute_tile_scale(800, 1920, 3200, 2048 * 2048), 1.0);
}

#[test]
fn translates_tile_boxes_with_scale_and_y_offset() {
    let points = [[120.0, 40.0], [360.0, 40.0], [360.0, 80.0], [120.0, 80.0]];
    let translated = translate_tile_box(points, 1.5, 1664);
    assert_eq!(
        translated,
        [
            [80.0, 1664.0 + 40.0 / 1.5],
            [240.0, 1664.0 + 40.0 / 1.5],
            [240.0, 1664.0 + 80.0 / 1.5],
            [80.0, 1664.0 + 80.0 / 1.5],
        ]
    );

    let unscaled = translate_tile_box(points, 1.0, 3328);
    assert_eq!(
        unscaled,
        [
            [120.0, 3368.0],
            [360.0, 3368.0],
            [360.0, 3408.0],
            [120.0, 3408.0],
        ]
    );
}

#[test]
fn deduplicates_seam_fragment_preferring_interior_line() {
    let span0 = TileSpan {
        y_start: 0,
        y_end: 1920,
    };
    let span1 = TileSpan {
        y_start: 1664,
        y_end: 3584,
    };
    let img_h = 3584;

    let cut_fragment = make_detection(
        120.0, 1904.0, 320.0, 1920.0, "HEL", 0.99, 0, span0, img_h,
    );
    let interior_full = make_detection(
        120.0, 1904.0, 400.0, 1934.0, "HELLO WORLD", 0.94, 1, span1, img_h,
    );

    let result = deduplicate_seam_detections(vec![cut_fragment, interior_full]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].text, "HELLO WORLD");
}

#[test]
fn keeps_distinct_stacked_and_side_by_side_lines_in_overlap_band() {
    let span0 = TileSpan {
        y_start: 0,
        y_end: 1920,
    };
    let span1 = TileSpan {
        y_start: 1664,
        y_end: 3584,
    };
    let img_h = 3584;

    let top_left_0 = make_detection(
        80.0, 1740.0, 260.0, 1768.0, "LINE ONE", 0.91, 0, span0, img_h,
    );
    let top_left_1 = make_detection(
        80.0, 1740.0, 260.0, 1768.0, "LINE ONE", 0.97, 1, span1, img_h,
    );
    let stacked_below_1 = make_detection(
        80.0, 1774.0, 260.0, 1802.0, "LINE TWO", 0.95, 1, span1, img_h,
    );
    let side_by_side_0 = make_detection(
        400.0, 1740.0, 580.0, 1768.0, "RIGHT BUBBLE", 0.96, 0, span0, img_h,
    );

    let result = deduplicate_seam_detections(vec![
        top_left_0,
        top_left_1,
        stacked_below_1,
        side_by_side_0,
    ]);
    assert_eq!(result.len(), 3);
    let texts: Vec<&str> = result.iter().map(|r| r.text.as_str()).collect();
    assert_eq!(texts, vec!["LINE ONE", "RIGHT BUBBLE", "LINE TWO"]);
    assert!((result[0].score - 0.97).abs() < 1e-5);
}
