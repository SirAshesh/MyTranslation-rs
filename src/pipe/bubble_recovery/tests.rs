use image::{Rgb, RgbImage};

use crate::model::OcrLineItem;
use crate::pipe::clustering::cluster_lines;

use super::corridor::trace_bubble_corridor;
use super::ink_scan::{find_uncovered_ink_bands, is_corridor_unseparated_between};
use super::recover_bubbles_with_ocr_fn;

const WHITE: Rgb<u8> = Rgb([255, 255, 255]);
const BLACK: Rgb<u8> = Rgb([15, 15, 15]);

fn paint_rect(img: &mut RgbImage, x1: u32, y1: u32, x2: u32, y2: u32, color: Rgb<u8>) {
    for y in y1..=y2.min(img.height().saturating_sub(1)) {
        for x in x1..=x2.min(img.width().saturating_sub(1)) {
            img.put_pixel(x, y, color);
        }
    }
}

fn ocr_item(x1: f32, y1: f32, x2: f32, y2: f32, text: &str, score: f32) -> OcrLineItem {
    OcrLineItem {
        r#box: [[x1, y1], [x2, y1], [x2, y2], [x1, y2]],
        text: text.to_string(),
        score,
    }
}

fn draw_open_top_bubble_fixture() -> RgbImage {
    let mut img = RgbImage::from_pixel(240, 260, Rgb([200, 200, 200]));
    paint_rect(&mut img, 40, 0, 200, 220, WHITE);
    for y in 0..220u32 {
        let flare = if y < 60 { (60 - y) / 4 } else { 0 };
        let lx = 38u32.saturating_sub(flare);
        let rx = (202u32 + flare).min(238);
        paint_rect(&mut img, lx, y, lx + 2, y, BLACK);
        paint_rect(&mut img, rx, y, rx + 2, y, BLACK);
    }
    paint_rect(&mut img, 40, 220, 200, 222, BLACK);
    // Uncovered dark ink representing "THA-" above existing detected lines
    paint_rect(&mut img, 90, 56, 150, 72, BLACK);
    // Existing detected lines "THAT'S" and "NOT WHAT I SAID!!"
    paint_rect(&mut img, 85, 92, 155, 108, BLACK);
    paint_rect(&mut img, 68, 116, 172, 132, BLACK);
    img
}

#[test]
fn test_open_top_bubble_detects_uncovered_ink_and_recovers_via_crop_ocr() {
    let img = draw_open_top_bubble_fixture();
    let initial_ocr = vec![
        ocr_item(85.0, 92.0, 155.0, 108.0, "THAT'S", 0.98),
        ocr_item(68.0, 116.0, 172.0, 132.0, "NOT WHAT I SAID!!", 0.98),
    ];
    let mut bubbles = cluster_lines(initial_ocr.clone(), img.width(), img.height(), "en");
    assert_eq!(bubbles.len(), 1);

    let corridor = trace_bubble_corridor(&img, &bubbles[0]).expect("corridor should trace");
    assert!(corridor.top_y <= 56);
    let bands = find_uncovered_ink_bands(&img, &corridor, &bubbles, 0);
    assert_eq!(bands.len(), 1);
    assert!(bands[0].y1 <= 58 && bands[0].y2 >= 70);

    let mut region_ocr_calls = 0usize;
    let recovered = recover_bubbles_with_ocr_fn(
        &img,
        &initial_ocr,
        &mut bubbles,
        "en",
        |_x1, _y1, _x2, _y2| {
            region_ocr_calls += 1;
            Ok(vec![ocr_item(90.0, 56.0, 150.0, 72.0, "THA-", 0.91)])
        },
    );

    assert_eq!(region_ocr_calls, 1);
    assert_eq!(recovered, 1);
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "Tha- that's not what I said!!");
    assert!(bubbles[0].y1 <= 56.5);
    assert!(bubbles[0].lines.iter().any(|l| l.text == "THA-"));
}

#[test]
fn test_closed_clean_bubble_skips_region_ocr_completely() {
    let mut img = RgbImage::from_pixel(220, 180, Rgb([190, 190, 190]));
    paint_rect(&mut img, 30, 30, 190, 150, WHITE);
    paint_rect(&mut img, 28, 28, 192, 30, BLACK);
    paint_rect(&mut img, 28, 150, 192, 152, BLACK);
    paint_rect(&mut img, 28, 28, 30, 152, BLACK);
    paint_rect(&mut img, 190, 28, 192, 152, BLACK);
    paint_rect(&mut img, 65, 65, 155, 82, BLACK);
    paint_rect(&mut img, 70, 90, 150, 107, BLACK);

    let initial_ocr = vec![
        ocr_item(65.0, 65.0, 155.0, 82.0, "I ALREADY", 0.99),
        ocr_item(70.0, 90.0, 150.0, 107.0, "SAID NO.", 0.99),
    ];
    let mut bubbles = cluster_lines(initial_ocr.clone(), img.width(), img.height(), "en");
    assert_eq!(bubbles.len(), 1);

    let mut region_ocr_calls = 0usize;
    let recovered = recover_bubbles_with_ocr_fn(
        &img,
        &initial_ocr,
        &mut bubbles,
        "en",
        |_x1, _y1, _x2, _y2| {
            region_ocr_calls += 1;
            Ok(Vec::new())
        },
    );

    assert_eq!(region_ocr_calls, 0);
    assert_eq!(recovered, 0);
}

#[test]
fn test_dropped_raw_ocr_fragment_inside_bubble_recovered_without_crop_ocr() {
    let img = draw_open_top_bubble_fixture();
    let raw_ocr = vec![
        ocr_item(90.0, 56.0, 150.0, 72.0, "THA-", 0.52),
        ocr_item(85.0, 92.0, 155.0, 108.0, "THAT'S", 0.98),
        ocr_item(68.0, 116.0, 172.0, 132.0, "NOT WHAT I SAID!!", 0.98),
    ];
    let mut bubbles = cluster_lines(raw_ocr.clone(), img.width(), img.height(), "en");
    assert_eq!(bubbles.len(), 1);

    let mut region_ocr_calls = 0usize;
    let recovered = recover_bubbles_with_ocr_fn(
        &img,
        &raw_ocr,
        &mut bubbles,
        "en",
        |_x1, _y1, _x2, _y2| {
            region_ocr_calls += 1;
            Ok(Vec::new())
        },
    );

    assert_eq!(region_ocr_calls, 0);
    assert_eq!(recovered, 1);
    assert_eq!(bubbles[0].source_text, "Tha- that's not what I said!!");
}

#[test]
fn test_separated_figure_eight_bubbles_never_merge() {
    let mut img = RgbImage::from_pixel(220, 260, Rgb([190, 190, 190]));
    paint_rect(&mut img, 30, 20, 190, 240, WHITE);
    paint_rect(&mut img, 28, 20, 30, 240, BLACK);
    paint_rect(&mut img, 190, 20, 192, 240, BLACK);
    paint_rect(&mut img, 28, 20, 192, 22, BLACK);
    paint_rect(&mut img, 28, 126, 192, 129, BLACK);
    paint_rect(&mut img, 28, 238, 192, 240, BLACK);
    paint_rect(&mut img, 65, 65, 155, 82, BLACK);
    paint_rect(&mut img, 65, 160, 155, 177, BLACK);

    let raw_ocr = vec![
        ocr_item(65.0, 65.0, 155.0, 82.0, "WHAT?", 0.99),
        ocr_item(65.0, 160.0, 155.0, 177.0, "NOTHING.", 0.99),
    ];
    let mut bubbles = cluster_lines(raw_ocr.clone(), img.width(), img.height(), "en");
    assert_eq!(bubbles.len(), 2);

    let recovered = recover_bubbles_with_ocr_fn(
        &img,
        &raw_ocr,
        &mut bubbles,
        "en",
        |_x1, _y1, _x2, _y2| Ok(Vec::new()),
    );
    assert_eq!(recovered, 0);
    assert_eq!(bubbles.len(), 2);

    let corridor = trace_bubble_corridor(&img, &bubbles[0]);
    if let Some(c) = corridor {
        assert!(!is_corridor_unseparated_between(&img, &c, 82, 160));
    }
}

#[test]
fn test_same_corridor_wide_leading_lines_merge_into_one_bubble() {
    let img = draw_open_top_bubble_fixture();
    let raw_ocr = vec![
        ocr_item(90.0, 56.0, 150.0, 72.0, "THA-", 0.92),
        ocr_item(85.0, 92.0, 155.0, 108.0, "THAT'S", 0.98),
        ocr_item(68.0, 116.0, 172.0, 132.0, "NOT WHAT I SAID!!", 0.98),
    ];
    let mut bubbles = cluster_lines(raw_ocr.clone(), img.width(), img.height(), "en");
    assert_eq!(bubbles.len(), 2);

    let recovered = recover_bubbles_with_ocr_fn(
        &img,
        &raw_ocr,
        &mut bubbles,
        "en",
        |_x1, _y1, _x2, _y2| Ok(Vec::new()),
    );
    assert!(recovered >= 1);
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "Tha- that's not what I said!!");
}

#[test]
fn test_cjk_interior_cleanup_marker_never_leaks_on_recovery() {
    let img = draw_open_top_bubble_fixture();
    let raw_ocr = vec![
        ocr_item(90.0, 56.0, 150.0, 72.0, "THA-", 0.92),
        ocr_item(85.0, 92.0, 155.0, 108.0, "本当？", 0.98),
        ocr_item(68.0, 116.0, 172.0, 132.0, "THAT'S NOT IT", 0.98),
    ];
    let mut bubbles = cluster_lines(raw_ocr.clone(), img.width(), img.height(), "ja");
    assert!(!bubbles.is_empty());

    recover_bubbles_with_ocr_fn(
        &img,
        &raw_ocr,
        &mut bubbles,
        "ja",
        |_x1, _y1, _x2, _y2| Ok(Vec::new()),
    );
    for b in &bubbles {
        assert!(
            !b.source_text.to_ascii_lowercase().contains("cjk_interior_cleanup"),
            "Synthetic cleanup marker leaked into bubble source_text: {}",
            b.source_text
        );
    }
}
