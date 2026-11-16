use super::*;

#[test]
fn test_horizontal_line_clustering() {
    let lines = vec![
        OcrLineItem {
            text: "HELLO".to_string(),
            score: 0.95,
            r#box: [[10.0, 10.0], [50.0, 10.0], [50.0, 25.0], [10.0, 25.0]],
        },
        OcrLineItem {
            text: "WORLD".to_string(),
            score: 0.95,
            r#box: [[10.0, 30.0], [50.0, 30.0], [50.0, 45.0], [10.0, 45.0]],
        },
    ];

    let bubbles = cluster_lines(lines, 200, 200, "en");
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "Hello world");
}

#[test]
fn test_vertical_japanese_manga_clustering() {
    let col_right = OcrLineItem {
        text: "何でそんなこと".to_string(),
        score: 0.92,
        r#box: [[150.0, 20.0], [170.0, 20.0], [170.0, 140.0], [150.0, 140.0]],
    };
    let col_left = OcrLineItem {
        text: "言うんだよ".to_string(),
        score: 0.90,
        r#box: [[120.0, 20.0], [140.0, 20.0], [140.0, 130.0], [120.0, 130.0]],
    };

    let bubbles = cluster_lines(vec![col_right, col_left], 300, 300, "ja");
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "何でそんなこと言うんだよ");
}

#[test]
fn test_tilted_chat_bubbles_separate_headers_and_merge_wrapped_lines() {
    let lines = vec![
        OcrLineItem {
            text: "Yewon".to_string(),
            score: 0.99,
            r#box: [
                [154.5, 229.0],
                [190.0, 220.5],
                [193.0, 232.5],
                [157.5, 241.5],
            ],
        },
        OcrLineItem {
            text: "Dain, I'm so screwed...".to_string(),
            score: 0.98,
            r#box: [
                [169.5, 249.5],
                [285.0, 216.0],
                [289.0, 230.5],
                [173.5, 264.0],
            ],
        },
        OcrLineItem {
            text: "Things have gotten way out of".to_string(),
            score: 0.99,
            r#box: [
                [175.5, 284.0],
                [335.0, 237.0],
                [340.0, 254.0],
                [180.5, 301.0],
            ],
        },
        OcrLineItem {
            text: "hand.".to_string(),
            score: 0.99,
            r#box: [
                [180.0, 299.0],
                [213.5, 290.0],
                [217.5, 304.0],
                [184.0, 313.0],
            ],
        },
    ];

    let bubbles = cluster_lines(lines, 599, 787, "en");
    assert_eq!(bubbles.len(), 3);
    assert_eq!(bubbles[0].source_text, "Yewon");
    assert_eq!(bubbles[1].source_text, "Dain, I'm so screwed...");
    assert_eq!(
        bubbles[2].source_text,
        "Things have gotten way out of hand."
    );
    assert!(bubbles[2].is_left_aligned);
    assert!(bubbles[2].angle_rad < -0.2);
}

#[test]
fn test_wide_manhwa_strip_bubble_merges_and_same_row_splits() {
    let narration = vec![
        OcrLineItem {
            text: "THAT'S HOW LONG I'VE WASTED".to_string(),
            score: 0.98,
            r#box: [
                [116.0, 598.0],
                [604.0, 598.0],
                [604.0, 627.0],
                [116.0, 627.0],
            ],
        },
        OcrLineItem {
            text: "SEARCHING FOR A MUSIC BOX THAT".to_string(),
            score: 0.99,
            r#box: [[93.0, 636.0], [628.0, 636.0], [628.0, 665.0], [93.0, 665.0]],
        },
        OcrLineItem {
            text: "COULD BE ANYWHERE.".to_string(),
            score: 1.0,
            r#box: [
                [186.0, 673.0],
                [533.0, 673.0],
                [533.0, 701.0],
                [186.0, 701.0],
            ],
        },
    ];
    let bubbles = cluster_lines(narration, 720, 1202, "en");
    assert_eq!(bubbles.len(), 1);
    assert_eq!(
        bubbles[0].source_text,
        "That's how long I've wasted searching for a music box that could be anywhere."
    );

    let split_row = vec![
        OcrLineItem {
            text: "IT COST A-WHOPPING".to_string(),
            score: 0.98,
            r#box: [[337.0, 27.0], [614.0, 6.0], [617.0, 32.0], [339.0, 53.0]],
        },
        OcrLineItem {
            text: "FRANCS,".to_string(),
            score: 1.0,
            r#box: [[473.0, 45.0], [599.0, 40.0], [600.0, 70.0], [474.0, 74.0]],
        },
        OcrLineItem {
            text: "2 MILLION".to_string(),
            score: 0.99,
            r#box: [[352.0, 59.0], [487.0, 47.0], [490.0, 74.0], [355.0, 86.0]],
        },
        OcrLineItem {
            text: "THEY SAY.".to_string(),
            score: 0.93,
            r#box: [[412.0, 85.0], [545.0, 74.0], [548.0, 103.0], [414.0, 114.0]],
        },
    ];
    let split_bubbles = cluster_lines(split_row, 720, 1202, "en");
    assert_eq!(split_bubbles.len(), 1);
    assert_eq!(
        split_bubbles[0].source_text,
        "It cost a-whopping 2 million francs, they say."
    );
}

#[test]
fn test_ultra_tall_strip_clamps_cluster_height_and_preserves_page1_limits() {
    let (_, page1_h) = compute_cluster_limits(720, 1202);
    assert!((page1_h - 1202.0 * 0.45).abs() < 1e-3);

    let (_, ultra_tall_h) = compute_cluster_limits(720, 13499);
    assert!((ultra_tall_h - 900.0).abs() < 1e-3);

    let mut dense_lines = Vec::new();
    for i in 0..25 {
        let y1 = 100.0 + i as f32 * 42.0;
        let y2 = y1 + 28.0;
        dense_lines.push(OcrLineItem {
            text: format!("LINE {}", i),
            score: 0.97,
            r#box: [[140.0, y1], [540.0, y1], [540.0, y2], [140.0, y2]],
        });
    }

    let bubbles = cluster_lines(dense_lines, 720, 13499, "en");
    assert!(bubbles.len() >= 2);
    for bubble in bubbles {
        assert!(bubble.h <= 900.0);
    }
}

#[test]
fn test_cjk_hallucinations_suppressed_on_predominantly_latin_pages() {
    let items = vec![
        OcrLineItem {
            text: "Just to be offered".to_string(),
            score: 0.98,
            r#box: [[100.0, 100.0], [320.0, 100.0], [320.0, 130.0], [100.0, 130.0]],
        },
        OcrLineItem {
            text: "as a bribe.".to_string(),
            score: 0.97,
            r#box: [[120.0, 136.0], [300.0, 136.0], [300.0, 166.0], [120.0, 166.0]],
        },
        OcrLineItem {
            text: "础科".to_string(),
            score: 0.88,
            r#box: [[20.0, 400.0], [220.0, 400.0], [220.0, 560.0], [20.0, 560.0]],
        },
        OcrLineItem {
            text: "肯rot..".to_string(),
            score: 0.85,
            r#box: [[450.0, 700.0], [550.0, 700.0], [550.0, 760.0], [450.0, 760.0]],
        },
    ];

    let en_bubbles = cluster_lines(items.clone(), 720, 1200, "en");
    assert_eq!(en_bubbles.len(), 1);
    assert_eq!(en_bubbles[0].source_text, "Just to be offered as a bribe.");

    let auto_bubbles = cluster_lines(items.clone(), 720, 1200, "auto");
    assert_eq!(auto_bubbles.len(), 1);

    let ja_bubbles = cluster_lines(items, 720, 1200, "ja");
    assert_eq!(ja_bubbles.len(), 3);
}

#[test]
fn test_cjk_hallucinations_suppressed_with_short_latin_exclamations() {
    let items = vec![
        OcrLineItem {
            text: "年05".to_string(),
            score: 0.90,
            r#box: [[276.0, 0.0], [672.0, 0.0], [672.0, 292.0], [276.0, 292.0]],
        },
        OcrLineItem {
            text: "Ah!".to_string(),
            score: 0.95,
            r#box: [[180.0, 780.0], [277.0, 780.0], [277.0, 839.0], [180.0, 839.0]],
        },
        OcrLineItem {
            text: "Hah!".to_string(),
            score: 0.95,
            r#box: [[449.0, 1442.0], [546.0, 1442.0], [546.0, 1484.0], [449.0, 1484.0]],
        },
        OcrLineItem {
            text: "Aah!".to_string(),
            score: 0.95,
            r#box: [[234.0, 4038.0], [334.0, 4038.0], [334.0, 4080.0], [234.0, 4080.0]],
        },
        OcrLineItem {
            text: "Ngh...".to_string(),
            score: 0.95,
            r#box: [[194.0, 4489.0], [278.0, 4489.0], [278.0, 4521.0], [194.0, 4521.0]],
        },
    ];

    let bubbles = cluster_lines(items, 720, 11826, "en");
    assert_eq!(bubbles.len(), 4);
    assert!(bubbles.iter().all(|b| !b.source_text.contains('年')));
}

#[test]
fn test_detect_dominant_source_lang_for_ja_zh_en() {
    let make_line = |text: &str, score: f32| OcrLineItem {
        text: text.to_string(),
        score,
        r#box: [[10.0, 10.0], [100.0, 10.0], [100.0, 40.0], [10.0, 40.0]],
    };

    let ja_lines = vec![
        make_line("まさか…一撃で結界を壊すなんて！", 0.96),
        make_line("SFX", 0.90),
    ];
    assert_eq!(detect_dominant_source_lang(&ja_lines, "auto"), "ja");
    assert_eq!(detect_dominant_source_lang(&ja_lines, "en"), "ja");

    let zh_lines = vec![
        make_line("快退后！这股灵压已经超出了极限！", 0.95),
        make_line("GO", 0.85),
    ];
    assert_eq!(detect_dominant_source_lang(&zh_lines, "auto"), "zh");
    assert_eq!(detect_dominant_source_lang(&zh_lines, "en"), "zh");

    let en_lines = vec![
        make_line("I ONLY CAME TO HELP YOU.", 0.98),
        make_line("は", 0.30),
    ];
    assert_eq!(detect_dominant_source_lang(&en_lines, "auto"), "en");
    assert_eq!(detect_dominant_source_lang(&en_lines, ""), "en");
    assert_eq!(detect_dominant_source_lang(&en_lines, "en"), "en");
    assert_eq!(detect_dominant_source_lang(&en_lines, "fr"), "fr");

    let mixed_cover_lines = vec![
        make_line("妹じゃなくて", 0.92),
        make_line("お前が許嫁", 0.90),
        make_line("IN OTHER WORDS, YOU AREN'T ACTUALLY RELATED BY BLOOD!", 0.99),
        make_line("YOU'RE UNHAPPY THAT HE KEEPS TREATING YOU LIKE A SISTER.", 0.99),
    ];
    assert_eq!(detect_dominant_source_lang(&mixed_cover_lines, "en"), "en");
}

#[test]
fn test_vertical_cjk_absorbs_furigana_and_merges_multi_column_chunks() {
    let line = |text: &str, x1: f32, y1: f32, x2: f32, y2: f32| OcrLineItem {
        text: text.to_string(),
        score: 0.95,
        r#box: [[x1, y1], [x2, y1], [x2, y2], [x1, y2]],
    };

    let items = vec![
        line("地頭って", 300.0, 100.0, 342.0, 240.0),
        line("じあたま", 344.0, 105.0, 362.0, 185.0),
        line("基本", 300.0, 260.0, 342.0, 340.0),
        line("きほん", 344.0, 262.0, 362.0, 330.0),
        line("バカ", 248.0, 110.0, 290.0, 190.0),
        line("を励", 248.0, 195.0, 290.0, 275.0),
        line("はげ", 292.0, 210.0, 308.0, 265.0),
        line("ま", 248.0, 282.0, 288.0, 316.0),
        line("す", 248.0, 322.0, 288.0, 355.0),
        line("言葉だからな", 196.0, 105.0, 238.0, 320.0),
        line("ことば", 240.0, 110.0, 256.0, 180.0),
    ];

    let bubbles = cluster_lines(items, 800, 800, "auto");
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "地頭って基本バカを励ます言葉だからな");
    assert!(
        bubbles[0].lines.iter().any(|l| l.text == "じあたま"),
        "furigana must remain in bubble.lines for inpainting"
    );
    assert!(
        bubbles[0].lines.iter().any(|l| l.text.is_empty()),
        "same-column gap bridge box expected for inpainting"
    );
}

#[test]
fn test_short_adjacent_vertical_cjk_merges_and_bridges_top_gap() {
    let items = vec![
        OcrLineItem {
            text: "坂本".to_string(),
            score: 0.95,
            r#box: [
                [1172.0, 221.0],
                [1223.0, 221.0],
                [1223.0, 298.0],
                [1172.0, 298.0],
            ],
        },
        OcrLineItem {
            text: "ん？".to_string(),
            score: 0.93,
            r#box: [
                [1140.0, 252.0],
                [1192.0, 252.0],
                [1192.0, 337.0],
                [1140.0, 337.0],
            ],
        },
    ];

    let bubbles = cluster_lines(items, 1400, 1400, "auto");
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "坂本ん？");
    assert!(
        bubbles[0]
            .lines
            .iter()
            .any(|l| l.text.is_empty() && l.y1 <= 222.0 && l.y2 >= 252.0),
        "expected synthetic top-gap bridge above ん？"
    );
}

#[test]
fn test_tall_multi_column_manga_keeps_main_kana_and_merges_wide_single_char() {
    let line = |text: &str, x1: f32, y1: f32, x2: f32, y2: f32| OcrLineItem {
        text: text.to_string(),
        score: 0.95,
        r#box: [[x1, y1], [x2, y1], [x2, y2], [x1, y2]],
    };

    // Mirrors ja_manga_real_01 (791x1236) where "言葉だからな" exceeds 0.45*H,
    // "ま" (w=76) sits in column 2 under "を励", and "す" has a wide 196x103 box.
    let items = vec![
        line("地頭って", 455.0, 361.0, 545.0, 660.0),
        line("じあたま", 547.0, 366.0, 572.0, 540.0),
        line("基本", 455.0, 690.0, 545.0, 913.0),
        line("バカ", 345.0, 365.0, 425.0, 520.0),
        line("を励", 337.0, 525.0, 432.0, 720.0),
        line("ま", 347.0, 726.0, 423.0, 824.0),
        line("す", 307.0, 814.0, 503.0, 917.0),
        line("言葉だからな", 184.0, 349.0, 310.0, 927.0),
    ];

    let bubbles = cluster_lines(items, 791, 1236, "ja");
    assert_eq!(bubbles.len(), 1);
    assert_eq!(bubbles[0].source_text, "地頭って基本バカを励ます言葉だからな");
    assert!(bubbles[0].x2 >= 571.0, "bubble.x2 should include absorbed furigana");

    // Overlapping adjacent columns ("坂", "本" and "ん？") must remain separate columns RTL.
    let sakamoto = vec![
        line("坂", 1181.0, 221.0, 1223.0, 258.0),
        line("本", 1170.0, 260.0, 1215.0, 298.0),
        line("ん？", 1140.0, 252.0, 1192.0, 337.0),
    ];
    let s_bubbles = cluster_lines(sakamoto, 1487, 2048, "ja");
    assert_eq!(s_bubbles.len(), 1);
    assert_eq!(s_bubbles[0].source_text, "坂本ん？");

    let cleanup_line = s_bubbles[0]
        .lines
        .iter()
        .find(|l| is_interior_cleanup_line(l))
        .expect("CJK bubble must include synthetic interior cleanup line");
    assert!(cleanup_line.x1 <= s_bubbles[0].x1 - 1.9);
    assert!(cleanup_line.x2 >= s_bubbles[0].x2 + 6.0);
}
