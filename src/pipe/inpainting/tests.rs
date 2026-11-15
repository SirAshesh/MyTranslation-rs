use super::clean_image_text;
use crate::model::{Bubble, LineInfo};
use image::{Rgb, RgbImage};

fn make_bubble(x1: f32, y1: f32, x2: f32, y2: f32) -> Bubble {
    let quad = [[x1, y1], [x2, y1], [x2, y2], [x1, y2]];
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
        lines: vec![LineInfo {
            quad,
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
            text: "TEST".to_string(),
            score: 0.99,
        }],
    }
}

#[test]
fn cleans_colored_text_and_halo_on_tinted_gradient_bubble() {
    let mut img = RgbImage::from_fn(120, 100, |_, y| {
        let shade = (252 - (y / 4) as u8).max(220);
        Rgb([shade, shade.saturating_sub(6), shade])
    });
    for y in 38..62 {
        for x in 36..84 {
            if (x + y) % 5 == 0 {
                img.put_pixel(x, y, Rgb([210, 95, 110]));
            }
        }
    }

    let bubble = make_bubble(34.0, 36.0, 86.0, 64.0);
    clean_image_text(&mut img, &[bubble]);

    for y in 36..64 {
        let expected = (252 - (y / 4) as u8).max(220);
        for x in 34..86 {
            let p = img.get_pixel(x, y);
            assert!(
                (p[0] as i32 - expected as i32).abs() <= 6
                    && (p[1] as i32 - (expected.saturating_sub(6)) as i32).abs() <= 6,
                "Residual ink at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
}

#[test]
fn cleans_white_text_with_dark_outline_on_artwork() {
    let mut img = RgbImage::from_fn(140, 100, |x, y| {
        let r = (180 + (x % 20) as u8).min(230);
        let g = (110 + (y % 20) as u8).min(170);
        let b = 150;
        Rgb([r, g, b])
    });
    for y in 38..62 {
        for x in 38..102 {
            let is_core = (44..=56).contains(&y) && (44..=96).contains(&x);
            let is_outline = (41..=59).contains(&y) && (41..=99).contains(&x) && !is_core;
            if is_core {
                img.put_pixel(x, y, Rgb([252, 252, 252]));
            } else if is_outline {
                img.put_pixel(x, y, Rgb([60, 40, 80]));
            }
        }
    }

    let bubble = make_bubble(38.0, 38.0, 102.0, 62.0);
    clean_image_text(&mut img, &[bubble]);

    for y in 40..60 {
        for x in 40..100 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] < 235 && p[0] > 150 && p[1] > 80,
                "Leftover core or outline at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
}

#[test]
fn cleans_trailing_punctuation_tail_without_touching_border() {
    let mut img = RgbImage::from_pixel(220, 120, Rgb([250, 250, 250]));
    // Black bubble border near the right edge must remain intact.
    for y in 5..115 {
        for x in 198..203 {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    // Maroon interior text inside [30..90, 40..80].
    for y in 45..75 {
        for x in 36..84 {
            if (x - 36) % 12 < 5 {
                img.put_pixel(x, y, Rgb([78, 32, 52]));
            }
        }
    }
    // Trailing punctuation ("??") outside the OCR box right edge [95..145].
    for y in 44..72 {
        for x in 96..112 {
            img.put_pixel(x, y, Rgb([78, 32, 52]));
        }
        for x in 120..136 {
            img.put_pixel(x, y, Rgb([78, 32, 52]));
        }
    }

    let mut bubble = make_bubble(30.0, 40.0, 90.0, 80.0);
    bubble.source_text = "Huh?".to_string();
    bubble.lines[0].text = "Huh?".to_string();
    clean_image_text(&mut img, &[bubble]);

    for y in 44..72 {
        for x in 96..136 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] > 230 && p[1] > 230 && p[2] > 230,
                "Leftover trailing punctuation at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
    for y in 20..100 {
        let border = img.get_pixel(200, y);
        assert!(
            border[0] < 30 && border[1] < 30 && border[2] < 30,
            "Bubble border damaged at (200, {}): {:?}",
            y,
            border
        );
    }
}

#[test]
fn cleans_gradient_trailing_punctuation_tail_dot_and_top() {
    let mut img = RgbImage::from_pixel(220, 140, Rgb([248, 240, 245]));
    let gradient_rgb = |y: u32| -> Rgb<u8> {
        let t = ((y.saturating_sub(25)) as f32 / 85.0).clamp(0.0, 1.0);
        Rgb([
            (28.0 + t * 132.0) as u8,
            (16.0 + t * 56.0) as u8,
            (24.0 + t * 84.0) as u8,
        ])
    };

    for y in 25..110 {
        let c = gradient_rgb(y);
        for x in 34..86 {
            if (x - 34) % 14 < 5 {
                img.put_pixel(x, y, c);
            }
        }
    }
    for y in 25..82 {
        let c = gradient_rgb(y);
        for x in 98..114 {
            img.put_pixel(x, y, c);
        }
    }
    for y in 98..108 {
        let c = gradient_rgb(y);
        for x in 102..112 {
            img.put_pixel(x, y, c);
        }
    }

    let mut bubble = make_bubble(30.0, 20.0, 90.0, 115.0);
    bubble.source_text = "Huh?".to_string();
    bubble.lines[0].text = "Huh?".to_string();
    clean_image_text(&mut img, &[bubble]);

    for y in 98..108 {
        for x in 102..112 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] > 230 && p[1] > 220 && p[2] > 225,
                "Leftover gradient dot at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
}

#[test]
fn cleans_webp_ringing_ghost_silhouette_while_preserving_bubble_border() {
    let mut img = RgbImage::from_pixel(180, 120, Rgb([250, 250, 250]));
    for y in 10..110 {
        for x in 148..153 {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for y in 42..78 {
        for x in 36..126 {
            let is_core = (48..=72).contains(&y) && (42..=120).contains(&x) && (x - 42) % 10 < 4;
            let is_ringing = (44..=76).contains(&y) && (38..=124).contains(&x) && !is_core;
            if is_core {
                img.put_pixel(x, y, Rgb([25, 25, 25]));
            } else if is_ringing {
                img.put_pixel(x, y, Rgb([236, 238, 235]));
            }
        }
    }

    let bubble = make_bubble(40.0, 46.0, 122.0, 74.0);
    clean_image_text(&mut img, &[bubble]);

    for y in 42..78 {
        for x in 36..126 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] >= 248 && p[1] >= 248 && p[2] >= 248,
                "Ghost ringing remained at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
    for y in 20..100 {
        let border = img.get_pixel(150, y);
        assert!(
            border[0] < 30 && border[1] < 30 && border[2] < 30,
            "Bubble border damaged at (150, {}): {:?}",
            y,
            border
        );
    }
}

#[test]
fn cleans_soft_glow_around_artwork_text() {
    let mut img = RgbImage::from_fn(160, 100, |x, y| {
        let r = (120 + (x % 30) as u8).min(180);
        let g = (95 + (y % 25) as u8).min(160);
        let b = 135;
        Rgb([r, g, b])
    });
    for y in 36..64 {
        for x in 40..120 {
            let is_core = (44..=56).contains(&y) && (46..=114).contains(&x);
            let is_soft_glow = (40..=60).contains(&y) && (42..=118).contains(&x) && !is_core;
            if is_core {
                img.put_pixel(x, y, Rgb([250, 250, 250]));
            } else if is_soft_glow {
                let base = img.get_pixel(x, y);
                img.put_pixel(
                    x,
                    y,
                    Rgb([
                        base[0].saturating_sub(14),
                        base[1].saturating_sub(14),
                        base[2].saturating_sub(14),
                    ]),
                );
            }
        }
    }

    let bubble = make_bubble(42.0, 40.0, 118.0, 60.0);
    clean_image_text(&mut img, &[bubble]);

    for y in 41..59 {
        for x in 43..117 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] < 200 && p[0] > 105 && p[1] > 80,
                "Soft glow or text core leftover at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
}

#[test]
fn preserves_box_border_crossing_allow_pad_next_to_dark_text() {
    let mut img = RgbImage::from_pixel(180, 120, Rgb([255, 255, 255]));
    // Black rectangular box border at y = 27..=29, crossing right through ALLOW_PAD (30 - 5 = 25).
    for y in 27..=29 {
        for x in 15..=165 {
            img.put_pixel(x, y, Rgb([12, 12, 12]));
        }
    }
    for y in 27..=92 {
        for x in 15..=17 {
            img.put_pixel(x, y, Rgb([12, 12, 12]));
        }
        for x in 163..=165 {
            img.put_pixel(x, y, Rgb([12, 12, 12]));
        }
    }
    for y in 90..=92 {
        for x in 15..=165 {
            img.put_pixel(x, y, Rgb([12, 12, 12]));
        }
    }

    // Dark text inside [30..150, 31..55], only 2px below the top border!
    for y in 33..53 {
        for x in 34..146 {
            if (x - 34) % 10 < 4 {
                img.put_pixel(x, y, Rgb([20, 20, 20]));
            }
        }
    }

    let bubble = make_bubble(30.0, 31.0, 150.0, 55.0);
    clean_image_text(&mut img, &[bubble]);

    // Text interior must be pure white (no grey smear from border).
    for y in 33..53 {
        for x in 34..146 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] >= 250 && p[1] >= 250 && p[2] >= 250,
                "Text or grey smudge left at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }

    // Top black border inside ALLOW_PAD must remain completely intact.
    for x in 20..=160 {
        let border = img.get_pixel(x, 28);
        assert!(
            border[0] < 30 && border[1] < 30 && border[2] < 30,
            "Box border damaged at ({}, 28): {:?}",
            x,
            border
        );
    }
}

#[test]
fn preserves_radial_rays_and_hair_outline_while_cleaning_solid_interior() {
    let mut img = RgbImage::from_pixel(200, 140, Rgb([255, 255, 255]));
    // Colored radial rays around the outer ring and entering ALLOW_PAD (pad 2..11px).
    for step in 0..14 {
        let x_top = 35 + step * 10;
        for y in 15..42 {
            img.put_pixel(x_top, y, Rgb([220, 90, 120]));
            img.put_pixel(x_top + 1, y, Rgb([220, 90, 120]));
        }
    }
    // Continuous dark hair outline near trailing punctuation on the right.
    for y in 20..120 {
        img.put_pixel(156, y, Rgb([25, 22, 28]));
        img.put_pixel(157, y, Rgb([25, 22, 28]));
    }
    // Dark text inside [40..108, 45..75] + trailing "??" at [115..135, 48..72].
    for y in 49..71 {
        for x in 44..104 {
            if (x - 44) % 9 < 4 {
                img.put_pixel(x, y, Rgb([18, 18, 18]));
            }
        }
        for x in 116..132 {
            img.put_pixel(x, y, Rgb([18, 18, 18]));
        }
    }

    let mut bubble = make_bubble(40.0, 45.0, 108.0, 75.0);
    bubble.source_text = "Huh?".to_string();
    bubble.lines[0].text = "Huh?".to_string();
    clean_image_text(&mut img, &[bubble]);

    for y in 49..71 {
        for x in 44..104 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] >= 250 && p[1] >= 250 && p[2] >= 250,
                "Text or pink tint left at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
        for x in 116..132 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] >= 250 && p[1] >= 250 && p[2] >= 250,
                "Trailing tail not cleaned at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }

    // Radial ray tips entering ALLOW_PAD (y = 41, quad.y1 = 45 -> pad = 4.0) stay intact.
    for step in 0..14 {
        let x_top = 35 + step * 10;
        let ray = img.get_pixel(x_top, 41);
        assert!(
            ray[0] > 180 && ray[1] < 130,
            "Radial ray damaged at ({}, 41): {:?}",
            x_top,
            ray
        );
    }

    // Hair outline stays intact.
    for y in 30..100 {
        let hair = img.get_pixel(156, y);
        assert!(
            hair[0] < 40 && hair[1] < 40 && hair[2] < 40,
            "Hair outline damaged at (156, {}): {:?}",
            y,
            hair
        );
    }
}

#[test]
fn skin_and_hair_artwork_line_avoids_white_fill_and_cleans_trailing_question_marks() {
    let mut img = RgbImage::from_pixel(220, 140, Rgb([242, 206, 180]));
    for y in 0..140 {
        for x in 0..55 {
            img.put_pixel(x, y, Rgb([92, 54, 122]));
        }
    }
    // White-outlined text inside quad [50..120, 48..88], plus trailing '?' tail at x=122..136.
    for y in 58..78 {
        for x in 56..115 {
            if (x / 10) % 2 == 0 {
                img.put_pixel(x, y, Rgb([250, 248, 246]));
            }
        }
        for x in 122..135 {
            img.put_pixel(x, y, Rgb([250, 248, 246]));
        }
        img.put_pixel(121, y, Rgb([25, 20, 30]));
        img.put_pixel(135, y, Rgb([25, 20, 30]));
    }
    for x in 121..=135 {
        img.put_pixel(x, 57, Rgb([25, 20, 30]));
        img.put_pixel(x, 78, Rgb([25, 20, 30]));
    }

    let mut bubble = make_bubble(50.0, 48.0, 120.0, 88.0);
    bubble.source_text = "Huh???".to_string();
    bubble.lines[0].text = "Huh???".to_string();
    clean_image_text(&mut img, &[bubble]);

    // Interior should diffuse smoothly without solid white fill or leftover outline ink.
    for y in 58..78 {
        for x in 65..112 {
            let p = img.get_pixel(x, y);
            let is_white_glyph = p[0] > 243 && p[1] > 243 && p[2] > 243;
            assert!(
                !is_white_glyph && p[0] > p[2],
                "Expected skin/hair diffusion (not white fill) at ({}, {}), got {:?}",
                x,
                y,
                p
            );
            if x >= 85 {
                assert!(
                    p[0] > 215 && p[2] < 200,
                    "Expected peach skin diffusion at ({}, {}), got {:?}",
                    x,
                    y,
                    p
                );
            }
        }
        for x in 122..135 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] > 210 && p[2] < 210,
                "Trailing '?' not cleaned to peach skin at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
    assert!(img.get_pixel(30, 70)[0] < 120 && img.get_pixel(30, 70)[2] > 100);
}

#[test]
fn vertical_bubble_cleans_trailing_exclamation_below_y2_preserving_border() {
    let mut img = RgbImage::from_pixel(140, 220, Rgb([255, 255, 255]));
    for x in 25..115 {
        for y in [18_u32, 19, 185, 186] {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for y in 18..=186 {
        for x in [25_u32, 26, 114, 115] {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    // Vertical text inside [52..88, 35..140], plus exclamation mark below y2 at y=144..166.
    for y in 42..135 {
        for x in 62..78 {
            img.put_pixel(x, y, Rgb([20, 20, 20]));
        }
    }
    for y in 144..160 {
        for x in 68..73 {
            img.put_pixel(x, y, Rgb([20, 20, 20]));
        }
    }
    for y in 163..167 {
        for x in 68..73 {
            img.put_pixel(x, y, Rgb([20, 20, 20]));
        }
    }

    let mut bubble = make_bubble(52.0, 35.0, 88.0, 140.0);
    bubble.source_text = "行くぞ！".to_string();
    bubble.lines[0].text = "行くぞ！".to_string();
    clean_image_text(&mut img, &[bubble]);

    for y in 144..168 {
        for x in 67..74 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] > 235 && p[1] > 235 && p[2] > 235,
                "Trailing '!' left at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
    assert!(img.get_pixel(70, 185)[0] < 40, "Bottom border damaged");
}

#[test]
fn roi_expands_to_cover_lines_outside_bubble_bbox() {
    let mut img = RgbImage::from_pixel(200, 140, Rgb([255, 255, 255]));
    for y in 40..65 {
        for x in 140..170 {
            img.put_pixel(x, y, Rgb([20, 20, 20]));
        }
    }

    let mut bubble = make_bubble(20.0, 40.0, 80.0, 65.0);
    let mut extra = make_bubble(140.0, 40.0, 170.0, 65.0).lines.remove(0);
    extra.text = "ふり".to_string();
    bubble.lines.push(extra);

    clean_image_text(&mut img, &[bubble]);

    for y in 42..63 {
        for x in 142..168 {
            let p = img.get_pixel(x, y);
            assert!(
                p[0] > 235 && p[1] > 235 && p[2] > 235,
                "Line outside bubble.x2 not cleaned at ({}, {}): {:?}",
                x,
                y,
                p
            );
        }
    }
}
