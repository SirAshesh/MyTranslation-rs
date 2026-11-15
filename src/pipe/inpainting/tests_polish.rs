use super::clean_image_text;
use crate::model::{Bubble, LineInfo, OcrLineItem};
use crate::pipe::clustering::cluster_lines;
use image::{Rgb, RgbImage};

fn make_line(x1: f32, y1: f32, x2: f32, y2: f32, text: &str) -> LineInfo {
    LineInfo {
        quad: [[x1, y1], [x2, y1], [x2, y2], [x1, y2]],
        x1,
        y1,
        x2,
        y2,
        cx: (x1 + x2) * 0.5,
        cy: (y1 + y2) * 0.5,
        w: x2 - x1,
        h: y2 - y1,
        true_w: x2 - x1,
        true_h: y2 - y1,
        angle_rad: 0.0,
        text: text.to_string(),
        score: 0.99,
    }
}

fn make_bubble_with_lines(x1: f32, y1: f32, x2: f32, y2: f32, lines: Vec<LineInfo>) -> Bubble {
    Bubble {
        id: 1,
        x1,
        y1,
        x2,
        y2,
        cx: (x1 + x2) * 0.5,
        cy: (y1 + y2) * 0.5,
        w: x2 - x1,
        h: y2 - y1,
        oriented_w: x2 - x1,
        oriented_h: y2 - y1,
        angle_rad: 0.0,
        is_left_aligned: false,
        source_text: "TEST".to_string(),
        lines,
    }
}

#[test]
fn preserves_top_box_border_when_ocr_quad_overlaps_and_blends_vignette() {
    let mut img = RgbImage::from_fn(200, 140, |x, y| {
        let dx = ((x as f32 - 100.0) / 80.0).abs().clamp(0.0, 1.0);
        let dy = ((y as f32 - 70.0) / 50.0).abs().clamp(0.0, 1.0);
        let edge = dx.max(dy);
        let g = (255.0 - edge * 12.0).round() as u8;
        let b = (255.0 - edge * 10.0).round() as u8;
        Rgb([255, g, b])
    });

    for y in 26..=28 {
        for x in 15..=185 {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for y in 26..=114 {
        for x in 15..=17 {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
        for x in 183..=185 {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for y in 112..=114 {
        for x in 15..=185 {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }

    for y in 35..51 {
        for x in 46..154 {
            if (x - 46) % 10 < 4 {
                img.put_pixel(x, y, Rgb([40, 30, 35]));
            }
        }
    }
    // Second stacked line 7px below line 1 (testing that inset core does not protect neighbor line edges).
    for y in 61..83 {
        for x in 36..164 {
            if (x - 36) % 9 < 4 {
                img.put_pixel(x, y, Rgb([40, 30, 35]));
            }
        }
    }

    // Top of OCR quad (y1 = 26.0) overlaps the black border at y = 26..=28.
    let line1 = make_line(40.0, 26.0, 160.0, 54.0, "JUST TO");
    let line2 = make_line(32.0, 61.0, 168.0, 83.0, "BE OFFERED AS");
    let bubble = make_bubble_with_lines(32.0, 26.0, 168.0, 83.0, vec![line1, line2]);
    clean_image_text(&mut img, &[bubble]);

    for x in 20..=180 {
        let p = img.get_pixel(x, 27);
        assert!(
            p[0] < 35 && p[1] < 35 && p[2] < 35,
            "Top box border gap at ({}, 27): {:?}",
            x,
            p
        );
    }
    for y in (35..51).chain(61..83) {
        for x in 46..154 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] > 235 && p[1] > 230 && p[2] > 230,
                "Text ink leftover at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }

    // Surrounding vignette at y = 31 (just below top border) should stay tinted, not #FFFFFF.
    let vignette_px = img.get_pixel(50, 31);
    assert!(
        vignette_px[1] < 252,
        "Expected warm tinted vignette near border (not stark #FFFFFF), got {:?}",
        vignette_px
    );
}

#[test]
fn preserves_starburst_tips_and_blends_pink_vignette_without_white_rectangle() {
    let mut img = RgbImage::from_fn(220, 160, |x, y| {
        let dx = (x as f32 - 110.0) / 90.0;
        let dy = (y as f32 - 80.0) / 60.0;
        let r = (dx * dx + dy * dy).sqrt().clamp(0.0, 1.0);
        let g = (255.0 - r * 14.0).round() as u8;
        let b = (255.0 - r * 11.0).round() as u8;
        Rgb([255, g, b])
    });

    for step in 0..16 {
        let sx = 35 + step * 10;
        for y in 12..49 {
            img.put_pixel(sx, y, Rgb([95, 50, 105]));
            img.put_pixel(sx + 1, y, Rgb([95, 50, 105]));
        }
    }
    for y in 54..76 {
        for x in 55..165 {
            if (x - 55) % 9 < 4 {
                img.put_pixel(x, y, Rgb([28, 22, 30]));
            }
        }
    }

    // Quad top y1 = 44 overlaps spike tips at y = 44..48.
    let line = make_line(45.0, 44.0, 175.0, 80.0, "I ONLY CAME TO");
    let bubble = make_bubble_with_lines(45.0, 44.0, 175.0, 80.0, vec![line]);
    clean_image_text(&mut img, &[bubble]);

    for step in 0..16 {
        let sx = 35 + step * 10;
        let tip = img.get_pixel(sx, 47);
        assert!(
            tip[0] < 130 && tip[1] < 80,
            "Starburst spike tip damaged at ({}, 47): {:?}",
            sx,
            tip
        );
    }

    // Outer vignette pixel inside RING_OUTER_PAD (e.g. x=42, y=75) must keep pink tint, not #FFFFFF.
    let outer_tint = img.get_pixel(42, 75);
    assert!(
        outer_tint[1] <= 249,
        "Expected smooth pink vignette outside text without #FFFFFF snap, got {:?}",
        outer_tint
    );
}

#[test]
fn cjk_interior_cleanup_erases_unocrd_furigana_and_protects_border_and_artwork() {
    let mut white_img = RgbImage::from_pixel(200, 220, Rgb([255, 255, 255]));
    for y in 20..=200 {
        for x in [25_u32, 26, 174, 175] {
            white_img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for x in 25..=175 {
        for y in [20_u32, 21, 199, 200] {
            white_img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    // Main vertical column [60..92, 40..170] and un-OCR'd furigana "きのう" at [95..103, 55..115].
    for y in 45..165 {
        for x in 68..84 {
            white_img.put_pixel(x, y, Rgb([20, 20, 20]));
        }
    }
    for y in 55..115 {
        for x in 95..103 {
            white_img.put_pixel(x, y, Rgb([25, 25, 25]));
        }
    }

    let items = vec![OcrLineItem {
        text: "髪型変えたら".to_string(),
        score: 0.96,
        r#box: [[60.0, 40.0], [92.0, 40.0], [92.0, 170.0], [60.0, 170.0]],
    }];
    let bubbles = cluster_lines(items, 200, 220, "ja");
    clean_image_text(&mut white_img, &bubbles);

    for y in 55..115 {
        for x in 95..103 {
            let p = white_img.get_pixel(x, y);
            assert!(
                p[0] > 240 && p[1] > 240 && p[2] > 240,
                "Residual furigana left at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
    assert!(white_img.get_pixel(25, 100)[0] < 35, "Left border damaged");
    assert!(white_img.get_pixel(174, 100)[0] < 35, "Right border damaged");

    // Non-white artwork CJK bubble must NOT apply synthetic interior cleanup over artwork.
    let mut art_img = RgbImage::from_fn(200, 220, |x, y| {
        Rgb([
            (90 + (x % 40) as u8),
            (60 + (y % 40) as u8),
            120,
        ])
    });
    for y in 45..165 {
        for x in 68..84 {
            art_img.put_pixel(x, y, Rgb([250, 250, 250]));
        }
    }
    // Unrelated dark hair streak inside cleanup_line right band (x=96..101) must stay intact.
    for y in 50..160 {
        for x in 96..101 {
            art_img.put_pixel(x, y, Rgb([35, 25, 45]));
        }
    }
    let art_items = vec![OcrLineItem {
        text: "髪型変えたら".to_string(),
        score: 0.96,
        r#box: [[60.0, 40.0], [92.0, 40.0], [92.0, 170.0], [60.0, 170.0]],
    }];
    let art_bubbles = cluster_lines(art_items, 200, 220, "ja");
    clean_image_text(&mut art_img, &art_bubbles);

    let hair_px = art_img.get_pixel(98, 100);
    assert!(
        hair_px[0] < 50 && hair_px[1] < 40,
        "Artwork hair damaged by CJK cleanup: {:?}",
        hair_px
    );

    // Slightly clipped left-edge stroke extending 2px outside horizontal CJK OCR box must be erased.
    let mut clipped_zh = RgbImage::from_pixel(220, 140, Rgb([255, 255, 255]));
    for y in 20..=120 {
        for x in [20_u32, 21, 199, 200] {
            clipped_zh.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for x in 20..=200 {
        for y in [20_u32, 21, 119, 120] {
            clipped_zh.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for y in 42..72 {
        for x in 48..54 {
            clipped_zh.put_pixel(x, y, Rgb([20, 20, 20]));
        }
        for x in 58..165 {
            clipped_zh.put_pixel(x, y, Rgb([20, 20, 20]));
        }
    }
    let zh_items = vec![OcrLineItem {
        text: "你以为仅凭这种程度的攻击".to_string(),
        score: 0.96,
        r#box: [[50.0, 40.0], [170.0, 40.0], [170.0, 74.0], [50.0, 74.0]],
    }];
    let zh_bubbles = cluster_lines(zh_items, 220, 140, "zh");
    clean_image_text(&mut clipped_zh, &zh_bubbles);
    let clipped_remnant = clipped_zh.get_pixel(50, 56);
    assert!(
        clipped_remnant[0] > 240 && clipped_remnant[1] > 240 && clipped_remnant[2] > 240,
        "Left-edge clipped CJK stroke left behind: {:?}",
        clipped_remnant
    );
    assert!(clipped_zh.get_pixel(20, 70)[0] < 35, "Left outline damaged");
}

#[test]
fn shaded_glow_bubble_and_neon_dark_artwork_text_erase_completely() {
    // 1. Shaded [228,225,227] bubble with bright [255] outer glow reaching outer ring.
    let mut shaded = RgbImage::from_pixel(220, 140, Rgb([228, 225, 227]));
    for y in 20..120 {
        for x in 60..160 {
            shaded.put_pixel(x, y, Rgb([254, 254, 254]));
        }
    }
    for y in 42..68 {
        for x in 70..150 {
            shaded.put_pixel(x, y, Rgb([18, 18, 18]));
        }
    }
    let shaded_line = make_line(50.0, 40.0, 170.0, 70.0, "ONLY");
    let shaded_bubble = make_bubble_with_lines(50.0, 40.0, 170.0, 70.0, vec![shaded_line]);
    clean_image_text(&mut shaded, &[shaded_bubble]);
    for y in 42..68 {
        let mid_px = shaded.get_pixel(110, y);
        assert!(
            mid_px[0] > 215 && mid_px[1] > 215 && mid_px[2] > 215,
            "Shaded glow bubble letter cap left un-erased at (110, {}): {:?}",
            y,
            mid_px
        );
    }

    // 2. White text with bright neon glow over dark artwork reaching trav boundary.
    let mut neon = RgbImage::from_pixel(220, 140, Rgb([28, 16, 34]));
    for y in 20..120 {
        for x in 30..190 {
            neon.put_pixel(x, y, Rgb([125, 70, 145]));
        }
    }
    for y in 42..68 {
        for x in 60..160 {
            neon.put_pixel(x, y, Rgb([250, 240, 252]));
        }
    }
    let neon_line = make_line(55.0, 40.0, 165.0, 70.0, "RESEMBLE");
    let neon_bubble = make_bubble_with_lines(55.0, 40.0, 165.0, 70.0, vec![neon_line]);
    clean_image_text(&mut neon, &[neon_bubble]);
    for y in 42..68 {
        let cap_px = neon.get_pixel(110, y);
        assert!(
            cap_px[0] < 150 && cap_px[1] < 100,
            "White neon text cap left behind at (110, {}): {:?}",
            y,
            cap_px
        );
    }
}
