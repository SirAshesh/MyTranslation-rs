use super::*;

fn make_render_bubble(id: usize, x1: f32, y1: f32, x2: f32, y2: f32) -> Bubble {
    Bubble {
        id,
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
        source_text: "SRC".to_string(),
        lines: Vec::new(),
    }
}

#[test]
fn composite_rotated_overlay_draws_at_tilted_endpoints() {
    let mut img = RgbImage::from_pixel(200, 200, Rgb([100, 140, 180]));
    let mut overlay = RgbaImage::new(80, 20);
    for y in 6..14 {
        for x in 6..74 {
            overlay.put_pixel(x, y, Rgba([12, 12, 12, 255]));
        }
    }

    let angle = -0.28_f32;
    composite_rotated_overlay(&mut img, &overlay, 100.0, 100.0, angle);

    let right_up = img.get_pixel(130, 91);
    let horizontal_far = img.get_pixel(130, 100);
    assert!(
        right_up[0] < 40,
        "expected rotated ink at (130, 91), got {:?}",
        right_up
    );
    assert_eq!(*horizontal_far, Rgb([100, 140, 180]));
}

#[test]
fn renderable_text_rejects_empty_and_glyphless_translations() {
    let renderer = ComicRenderer::new("fonts/comic_bold.ttf").expect("load comic font");
    let bubble = make_render_bubble(7, 0.0, 0.0, 100.0, 40.0);

    let empty_map = HashMap::new();
    assert_eq!(renderer.renderable_text(&bubble, &empty_map), None);

    let mut blank_map = HashMap::new();
    blank_map.insert(7, "   \n ".to_string());
    assert_eq!(renderer.renderable_text(&bubble, &blank_map), None);

    let mut glyphless_map = HashMap::new();
    glyphless_map.insert(7, "テスト".to_string());
    assert_eq!(renderer.renderable_text(&bubble, &glyphless_map), None);

    let mut valid_map = HashMap::new();
    valid_map.insert(7, "  halo dunia!  ".to_string());
    assert_eq!(
        renderer.renderable_text(&bubble, &valid_map),
        Some("HALO DUNIA!".to_string())
    );
}

#[test]
fn renders_within_bubble_borders_and_clamps_to_canvas_bounds() {
    let renderer = ComicRenderer::new("fonts/comic_bold.ttf").expect("load comic font");
    let mut img = RgbImage::from_pixel(200, 200, Rgb([255, 255, 255]));
    for y in 40..=160 {
        for x in [65_u32, 66, 134, 135] {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    for x in 65..=135 {
        for y in [40_u32, 41, 159, 160] {
            img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }

    let bubbles = vec![
        make_render_bubble(1, 76.0, 55.0, 124.0, 145.0),
        make_render_bubble(2, 164.0, 1.0, 198.0, 42.0),
    ];
    let mut translations = HashMap::new();
    translations.insert(1, "WAIT STOP RIGHT THERE!".to_string());
    translations.insert(2, "WAIT!".to_string());

    renderer.render_bubbles(&mut img, &bubbles, &translations);

    for y in 40..=160 {
        assert!(img.get_pixel(65, y)[0] < 35, "left border overwritten at y={y}");
        assert!(img.get_pixel(135, y)[0] < 35, "right border overwritten at y={y}");
    }
    for y in 0..3 {
        for x in 150..200 {
            assert_eq!(*img.get_pixel(x, y), Rgb([255, 255, 255]), "clipped top margin");
        }
    }
    for y in 0..55 {
        for x in 197..200 {
            assert_eq!(*img.get_pixel(x, y), Rgb([255, 255, 255]), "clipped right margin");
        }
    }
}

#[test]
fn non_white_hair_and_skin_background_does_not_recenter_text() {
    let renderer = ComicRenderer::new("fonts/comic_bold.ttf").expect("load comic font");
    let mut img = RgbImage::from_pixel(300, 160, Rgb([242, 206, 180]));
    for y in 0..160 {
        for x in 0..60 {
            img.put_pixel(x, y, Rgb([70, 45, 95]));
        }
        img.put_pixel(290, y, Rgb([15, 15, 15]));
    }

    let bubble = make_render_bubble(1, 70.0, 40.0, 150.0, 110.0);
    let mut translations = HashMap::new();
    translations.insert(1, "EH?".to_string());

    renderer.render_bubbles(&mut img, &[bubble], &translations);

    let mut dark_x_sum = 0.0_f32;
    let mut dark_count = 0usize;
    for y in 40..110 {
        for x in 70..280 {
            let p = img.get_pixel(x, y);
            if p[0] < 60 && p[1] < 60 && p[2] < 60 {
                dark_x_sum += x as f32;
                dark_count += 1;
            }
        }
    }
    assert!(dark_count > 10, "expected rendered EH? glyphs");
    let mean_x = dark_x_sum / dark_count as f32;
    assert!(
        (mean_x - 110.0).abs() <= 10.0,
        "text shifted away from bubble.cx=110.0 to mean_x={mean_x}"
    );
}

#[test]
fn curved_oval_tightens_top_bottom_width_and_vertical_cjk_expands_into_oval() {
    let renderer = ComicRenderer::new("fonts/comic_bold.ttf").expect("load comic font");
    let mut oval_img = RgbImage::from_pixel(360, 180, Rgb([255, 255, 255]));
    for y in 20..=160 {
        let dy = (y as f32 - 90.0) / 70.0;
        let half_w = (140.0 * (1.0 - dy * dy).max(0.0).sqrt()).max(10.0);
        let lx = (180.0 - half_w).round() as u32;
        let rx = (180.0 + half_w).round() as u32;
        for dx in 0..3 {
            oval_img.put_pixel(lx.saturating_sub(dx), y, Rgb([15, 15, 15]));
            oval_img.put_pixel((rx + dx).min(359), y, Rgb([15, 15, 15]));
        }
    }

    let bounds = detect_horizontal_bubble_bounds(&oval_img, 180.0, 90.0, 260.0, 100.0)
        .expect("oval bounds detected");
    assert!(
        bounds.0 >= 49.0 && bounds.1 <= 311.0,
        "expected tight top/bottom oval bounds, got {:?}",
        bounds
    );

    let mut vert_img = RgbImage::from_pixel(260, 320, Rgb([255, 255, 255]));
    for y in 20..=300 {
        for x in [50_u32, 51, 209, 210] {
            vert_img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    let vert_bubble = make_render_bubble(2, 105.0, 50.0, 155.0, 270.0);
    let mut translations = HashMap::new();
    translations.insert(2, "WAH, INI BAGUS BANGET!".to_string());
    renderer.render_bubbles(&mut vert_img, &[vert_bubble], &translations);

    let mut min_ink_x = 260u32;
    let mut max_ink_x = 0u32;
    for y in 50..270 {
        for x in 55..205 {
            if vert_img.get_pixel(x, y)[0] < 40 {
                min_ink_x = min_ink_x.min(x);
                max_ink_x = max_ink_x.max(x);
            }
        }
    }
    assert!(
        max_ink_x - min_ink_x > 60,
        "expected vertical CJK oval text to expand wider than narrow column (50px), got {}..{}",
        min_ink_x,
        max_ink_x
    );

    // Waist-pinched double bubble where y = cy - 0.35 * base_h hits outside/waist at cx.
    let mut double_img = RgbImage::from_pixel(300, 320, Rgb([255, 255, 255]));
    for y in 0..110 {
        for x in 0..160 {
            double_img.put_pixel(x, y, Rgb([50, 50, 50]));
        }
    }
    for y in 40..=280 {
        let lx = if y < 110 { 160 } else { 70 };
        let rx = if y > 200 { 200 } else { 250 };
        for dx in 0..3 {
            double_img.put_pixel(lx + dx, y, Rgb([15, 15, 15]));
            double_img.put_pixel(rx + dx, y, Rgb([15, 15, 15]));
        }
    }
    for x in 70..=165 {
        for y in 106..=110 {
            double_img.put_pixel(x, y, Rgb([15, 15, 15]));
        }
    }
    let dbl_bounds = detect_horizontal_bubble_bounds(&double_img, 150.0, 160.0, 140.0, 200.0)
        .expect("waist-pinched double bubble bounds detected");
    assert!(
        dbl_bounds.0 >= 72.0 && dbl_bounds.1 <= 202.0,
        "expected safe bounds inside double bubble lobes, got {:?}",
        dbl_bounds
    );

    // Panel-edge-clipped tall dome oval: open on the right at cy, closed at top/bottom domes.
    let mut clipped_oval = RgbImage::from_pixel(200, 320, Rgb([255, 255, 255]));
    for y in 20..=300 {
        let dy = (y as f32 - 160.0) / 140.0;
        let half_w = (90.0 * (1.0 - dy * dy).max(0.0).sqrt()).max(12.0);
        let lx = (120.0 - half_w).round() as u32;
        let rx = (120.0 + half_w).round() as u32;
        for dx in 0..3 {
            clipped_oval.put_pixel(lx.saturating_sub(dx), y, Rgb([15, 15, 15]));
            if rx + dx < 200 {
                clipped_oval.put_pixel(rx + dx, y, Rgb([15, 15, 15]));
            }
        }
    }
    let clipped_bubble = make_render_bubble(3, 70.0, 50.0, 175.0, 270.0);
    let mut tr = HashMap::new();
    tr.insert(
        3,
        "DIA PAKAI BAHASA SOPAN KE STAF TOKO JUGA HEBAT BANGET YA".to_string(),
    );
    renderer.render_bubbles(&mut clipped_oval, &[clipped_bubble], &tr);
    for x in [56_u32, 184] {
        assert!(
            clipped_oval.get_pixel(x, 60)[0] < 35,
            "Clipped tall dome border overwritten at ({}, 60)",
            x
        );
    }
    for y in 40..=280 {
        let dy = (y as f32 - 160.0) / 140.0;
        let half_w = (90.0 * (1.0 - dy * dy).max(0.0).sqrt()).max(12.0);
        let lx = (120.0 - half_w).round() as u32;
        let rx = (120.0 + half_w).round() as u32;
        assert!(
            clipped_oval.get_pixel(lx.saturating_sub(5), y)[0] > 240,
            "Rendered ink overflowed left border at y={}",
            y
        );
        if rx + 5 < 200 {
            assert!(
                clipped_oval.get_pixel(rx + 5, y)[0] > 240,
                "Rendered ink overflowed right border at y={}",
                y
            );
        }
    }
}
