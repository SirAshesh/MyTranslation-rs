use crate::model::{Bubble, LineInfo, OcrLineItem};
use crate::pipe::text_cleaner::{
    contains_cjk, is_junk_or_sfx, join_manga_lines, normalize_source_text,
};

struct Cluster {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    lines: Vec<LineInfo>,
}

const MIN_TILT_RAD: f32 = 0.026;

fn compute_oriented_metrics(quad: &[[f32; 2]; 4]) -> (f32, f32, f32) {
    let top_dx = quad[1][0] - quad[0][0];
    let top_dy = quad[1][1] - quad[0][1];
    let bot_dx = quad[2][0] - quad[3][0];
    let bot_dy = quad[2][1] - quad[3][1];

    let top_len = top_dx.hypot(top_dy);
    let bot_len = bot_dx.hypot(bot_dy);
    let left_len = (quad[3][0] - quad[0][0]).hypot(quad[3][1] - quad[0][1]);
    let right_len = (quad[2][0] - quad[1][0]).hypot(quad[2][1] - quad[1][1]);

    let true_w = ((top_len + bot_len) * 0.5).max(1.0);
    let true_h = ((left_len + right_len) * 0.5).max(1.0);

    let avg_dx = (top_dx + bot_dx) * 0.5;
    let avg_dy = (top_dy + bot_dy) * 0.5;
    let raw_angle = avg_dy.atan2(avg_dx);
    let angle_rad = if raw_angle.abs() < MIN_TILT_RAD {
        0.0
    } else {
        raw_angle
    };

    (true_w, true_h, angle_rad)
}

fn project_quad_on_angle(quad: &[[f32; 2]; 4], angle_rad: f32) -> (f32, f32, f32, f32) {
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();

    let mut u_min = f32::INFINITY;
    let mut u_max = f32::NEG_INFINITY;
    let mut v_min = f32::INFINITY;
    let mut v_max = f32::NEG_INFINITY;

    for p in quad {
        let u = p[0] * cos_a + p[1] * sin_a;
        let v = -p[0] * sin_a + p[1] * cos_a;
        u_min = u_min.min(u);
        u_max = u_max.max(u);
        v_min = v_min.min(v);
        v_max = v_max.max(v);
    }

    (u_min, u_max, v_min, v_max)
}

fn normalize_quad(raw_box: [[f32; 2]; 4], img_width: u32, img_height: u32) -> [[f32; 2]; 4] {
    let max_w = img_width as f32;
    let max_h = img_height as f32;
    let mut out = raw_box;
    for p in &mut out {
        p[0] = p[0].clamp(0.0, max_w);
        p[1] = p[1].clamp(0.0, max_h);
    }
    out
}

pub fn cluster_lines(
    ocr_lines: Vec<OcrLineItem>,
    img_width: u32,
    img_height: u32,
    source_lang: &str,
) -> Vec<Bubble> {
    if ocr_lines.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for item in ocr_lines {
        let text = item.text.trim().to_string();
        if text.is_empty() {
            continue;
        }

        let quad = normalize_quad(item.r#box, img_width, img_height);
        let min_x = quad.iter().map(|p| p[0]).fold(f32::INFINITY, f32::min);
        let max_x = quad.iter().map(|p| p[0]).fold(f32::NEG_INFINITY, f32::max);
        let min_y = quad.iter().map(|p| p[1]).fold(f32::INFINITY, f32::min);
        let max_y = quad.iter().map(|p| p[1]).fold(f32::NEG_INFINITY, f32::max);

        let w = (max_x - min_x).max(1.0);
        let h = (max_y - min_y).max(1.0);
        let (true_w, true_h, angle_rad) = compute_oriented_metrics(&quad);

        if is_junk_or_sfx(&text, item.score, true_w, true_h) {
            continue;
        }

        lines.push(LineInfo {
            quad,
            x1: min_x,
            y1: min_y,
            x2: max_x,
            y2: max_y,
            cx: (min_x + max_x) / 2.0,
            cy: (min_y + max_y) / 2.0,
            w,
            h,
            true_w,
            true_h,
            angle_rad,
            text,
            score: item.score,
        });
    }

    if lines.is_empty() {
        return Vec::new();
    }

    let is_lang_cjk = source_lang.starts_with("ja") || source_lang.starts_with("zh");
    let vertical_line_count = lines.iter().filter(|l| l.true_h > l.true_w * 1.15).count();
    let mostly_vertical = (vertical_line_count as f32 / lines.len() as f32) > 0.40;

    if mostly_vertical || is_lang_cjk {
        lines.sort_by(|a, b| {
            b.cx.partial_cmp(&a.cx)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cy.partial_cmp(&b.cy).unwrap_or(std::cmp::Ordering::Equal))
        });
    } else {
        lines.sort_by(|a, b| {
            a.cy.partial_cmp(&b.cy)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cx.partial_cmp(&b.cx).unwrap_or(std::cmp::Ordering::Equal))
        });
    }

    let is_tall_strip = img_height > img_width;
    let max_cluster_w = if is_tall_strip {
        (img_width as f32 * 0.85).max(260.0)
    } else {
        (img_width as f32 * 0.55).max(240.0)
    };
    let max_cluster_h = if is_tall_strip {
        (img_height as f32 * 0.45).max(240.0)
    } else {
        (img_height as f32 * 0.35).max(220.0)
    };

    let mut clusters: Vec<Cluster> = Vec::new();

    for l in lines {
        let is_cjk = contains_cjk(&l.text);
        let line_is_vertical = is_cjk && (l.true_h >= l.true_w * 1.25);

        let mut best_idx = None;
        let mut best_score = f32::MAX;

        for (idx, c) in clusters.iter().enumerate() {
            let last_line = match c.lines.last() {
                Some(line) => line,
                None => continue,
            };

            let cluster_is_vert = c
                .lines
                .iter()
                .filter(|cl| cl.true_h >= cl.true_w * 1.25)
                .count()
                * 2
                >= c.lines.len();
            if line_is_vertical != cluster_is_vert {
                continue;
            }

            let cand_x1 = c.x1.min(l.x1);
            let cand_y1 = c.y1.min(l.y1);
            let cand_x2 = c.x2.max(l.x2);
            let cand_y2 = c.y2.max(l.y2);

            if (cand_x2 - cand_x1) > max_cluster_w || (cand_y2 - cand_y1) > max_cluster_h {
                continue;
            }

            if line_is_vertical {
                let dx = (last_line.x1 - l.x2).max(l.x1 - last_line.x2).max(0.0);
                let dcy = (l.cy - last_line.cy).abs();
                let overlap_y = last_line.y2.min(l.y2) - last_line.y1.max(l.y1);
                let min_h = l.h.min(last_line.h);
                let min_w = l.w.min(last_line.w);

                let is_adjacent_col = dx <= min_w * 0.75 + 10.0;
                let is_vertically_aligned = overlap_y >= min_h * 0.45 || dcy <= min_h * 0.35 + 12.0;

                if is_adjacent_col && is_vertically_aligned {
                    let score = dx + dcy * 0.5;
                    if score < best_score {
                        best_score = score;
                        best_idx = Some(idx);
                    }
                }
            } else {
                let angle_diff = (l.angle_rad - last_line.angle_rad).abs();
                if angle_diff > 0.25 {
                    continue;
                }

                let ref_angle = if l.true_w > last_line.true_w {
                    l.angle_rad
                } else {
                    last_line.angle_rad
                };
                let (lu_min, lu_max, lv_min, lv_max) =
                    project_quad_on_angle(&last_line.quad, ref_angle);
                let (cu_min, cu_max, cv_min, cv_max) =
                    project_quad_on_angle(&l.quad, ref_angle);

                let last_v_mid = (lv_min + lv_max) * 0.5;
                let mut row_u_min = lu_min;
                let mut row_u_max = lu_max;
                let mut row_v_min = lv_min;
                let mut row_v_max = lv_max;
                for cl in &c.lines {
                    let (u1, u2, v1, v2) = project_quad_on_angle(&cl.quad, ref_angle);
                    let v_mid = (v1 + v2) * 0.5;
                    if (v_mid - last_v_mid).abs() <= last_line.true_h * 0.45 {
                        row_u_min = row_u_min.min(u1);
                        row_u_max = row_u_max.max(u2);
                        row_v_min = row_v_min.min(v1);
                        row_v_max = row_v_max.max(v2);
                    }
                }

                let row_w = (row_u_max - row_u_min).max(1.0);
                let dv = (cv_min - row_v_max).max(row_v_min - cv_max).max(0.0);
                let overlap_v = row_v_max.min(cv_max) - row_v_min.max(cv_min);
                let du_gap = (cu_min - row_u_max).max(row_u_min - cu_max).max(0.0);
                let du_center = ((cu_min + cu_max) * 0.5 - (row_u_min + row_u_max) * 0.5).abs();
                let du_left = (cu_min - row_u_min).abs();
                let overlap_u = row_u_max.min(cu_max) - row_u_min.max(cu_min);
                let min_true_w = l.true_w.min(row_w);

                let last_ends_sentence =
                    last_line.text.ends_with("...") || last_line.text.ends_with(". ");
                let max_gap = if last_ends_sentence {
                    last_line.true_h * 0.35
                } else {
                    last_line.true_h * 0.65
                };

                let is_adjacent_line = dv <= max_gap;
                let height_ratio =
                    l.true_h.max(last_line.true_h) / l.true_h.min(last_line.true_h).max(1.0);
                let is_same_row_fragment = overlap_v >= l.true_h.min(last_line.true_h) * 0.45
                    && du_gap <= l.true_h.min(last_line.true_h) * 0.65
                    && height_ratio <= 1.35;
                let is_centered_continuation =
                    overlap_u >= min_true_w * 0.45 && du_center <= min_true_w * 0.35 + 15.0;
                let is_left_continuation = overlap_u >= min_true_w * 0.45
                    && dv <= last_line.true_h * 0.55
                    && height_ratio <= 1.35
                    && (du_left <= 6.0
                        || (du_left <= last_line.true_h * 0.55 + 4.0
                            && l.true_w <= row_w * 1.25));

                if is_same_row_fragment || (is_adjacent_line && (is_centered_continuation || is_left_continuation)) {
                    let align_penalty = if is_same_row_fragment {
                        du_gap * 0.5
                    } else if is_centered_continuation {
                        du_center * 0.5
                    } else {
                        du_left * 0.5
                    };
                    let score = dv + align_penalty;
                    if score < best_score {
                        best_score = score;
                        best_idx = Some(idx);
                    }
                }
            }
        }

        if let Some(idx) = best_idx {
            let c = &mut clusters[idx];
            c.x1 = c.x1.min(l.x1);
            c.y1 = c.y1.min(l.y1);
            c.x2 = c.x2.max(l.x2);
            c.y2 = c.y2.max(l.y2);
            c.lines.push(l);
        } else {
            clusters.push(Cluster {
                x1: l.x1,
                y1: l.y1,
                x2: l.x2,
                y2: l.y2,
                lines: vec![l],
            });
        }
    }

    let mut bubbles = Vec::new();
    let mut bubble_id = 1;

    for mut c in clusters {
        let is_vertical_bubble = c
            .lines
            .iter()
            .filter(|l| l.true_h > l.true_w * 1.15)
            .count()
            * 2
            >= c.lines.len()
            || (c.lines.iter().any(|l| contains_cjk(&l.text))
                && (c.y2 - c.y1) > (c.x2 - c.x1) * 1.2);

        let bubble_angle = if is_vertical_bubble {
            0.0
        } else {
            let mut cos_sum = 0.0f32;
            let mut sin_sum = 0.0f32;
            for line in &c.lines {
                let weight = (line.true_w * line.true_h).max(1.0);
                cos_sum += weight * line.angle_rad.cos();
                sin_sum += weight * line.angle_rad.sin();
            }
            let mean_angle = sin_sum.atan2(cos_sum);
            if mean_angle.abs() < MIN_TILT_RAD {
                0.0
            } else {
                mean_angle
            }
        };

        if is_vertical_bubble {
            c.lines.sort_by(|a, b| {
                let dcx = (b.cx - a.cx).abs();
                let col_w = a.w.max(b.w);
                if dcx > col_w * 0.45 {
                    b.cx.partial_cmp(&a.cx).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    a.y1.partial_cmp(&b.y1).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        } else {
            c.lines.sort_by(|a, b| {
                let (_, _, av_min, av_max) = project_quad_on_angle(&a.quad, bubble_angle);
                let (_, _, bv_min, bv_max) = project_quad_on_angle(&b.quad, bubble_angle);
                let (au_min, au_max, _, _) = project_quad_on_angle(&a.quad, bubble_angle);
                let (bu_min, bu_max, _, _) = project_quad_on_angle(&b.quad, bubble_angle);
                let av_mid = (av_min + av_max) * 0.5;
                let bv_mid = (bv_min + bv_max) * 0.5;
                let dcv = (bv_mid - av_mid).abs();
                let row_h = a.true_h.max(b.true_h);
                if dcv > row_h * 0.45 {
                    av_mid.partial_cmp(&bv_mid).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    let au_mid = (au_min + au_max) * 0.5;
                    let bu_mid = (bu_min + bu_max) * 0.5;
                    au_mid.partial_cmp(&bu_mid).unwrap_or(std::cmp::Ordering::Equal)
                }
            });
        }

        let raw_texts: Vec<&str> = c.lines.iter().map(|l| l.text.as_str()).collect();
        let combined = join_manga_lines(&raw_texts);
        let normalized = normalize_source_text(&combined);

        let bubble_w = (c.x2 - c.x1).max(1.0);
        let bubble_h = (c.y2 - c.y1).max(1.0);
        let avg_score = c.lines.iter().map(|l| l.score).sum::<f32>() / c.lines.len() as f32;

        if is_junk_or_sfx(&normalized, avg_score, bubble_w, bubble_h) {
            continue;
        }

        let (cx, cy, oriented_w, oriented_h, is_left_aligned) =
            compute_bubble_oriented_box(&c.lines, bubble_angle, is_vertical_bubble);

        bubbles.push(Bubble {
            id: bubble_id,
            x1: c.x1,
            y1: c.y1,
            x2: c.x2,
            y2: c.y2,
            cx,
            cy,
            w: bubble_w,
            h: bubble_h,
            oriented_w,
            oriented_h,
            angle_rad: bubble_angle,
            is_left_aligned,
            source_text: normalized,
            lines: c.lines,
        });
        bubble_id += 1;
    }

    bubbles
}

fn compute_bubble_oriented_box(
    lines: &[LineInfo],
    angle_rad: f32,
    is_vertical: bool,
) -> (f32, f32, f32, f32, bool) {
    let mut u_min = f32::INFINITY;
    let mut u_max = f32::NEG_INFINITY;
    let mut v_min = f32::INFINITY;
    let mut v_max = f32::NEG_INFINITY;

    let mut left_min = f32::INFINITY;
    let mut left_max = f32::NEG_INFINITY;
    let mut center_min = f32::INFINITY;
    let mut center_max = f32::NEG_INFINITY;
    let mut h_sum = 0.0f32;

    for line in lines {
        let (lu_min, lu_max, lv_min, lv_max) = project_quad_on_angle(&line.quad, angle_rad);
        u_min = u_min.min(lu_min);
        u_max = u_max.max(lu_max);
        v_min = v_min.min(lv_min);
        v_max = v_max.max(lv_max);

        left_min = left_min.min(lu_min);
        left_max = left_max.max(lu_min);
        let lu_mid = (lu_min + lu_max) * 0.5;
        center_min = center_min.min(lu_mid);
        center_max = center_max.max(lu_mid);
        h_sum += line.true_h;
    }

    let oriented_w = (u_max - u_min).max(1.0);
    let oriented_h = (v_max - v_min).max(1.0);
    let u_mid = (u_min + u_max) * 0.5;
    let v_mid = (v_min + v_max) * 0.5;

    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let cx = u_mid * cos_a - v_mid * sin_a;
    let cy = u_mid * sin_a + v_mid * cos_a;

    let left_spread = (left_max - left_min).max(0.0);
    let center_spread = (center_max - center_min).max(0.0);
    let avg_true_h = if lines.is_empty() {
        12.0
    } else {
        h_sum / lines.len() as f32
    };
    let is_left_aligned = !is_vertical
        && lines.len() >= 2
        && left_spread < center_spread * 0.6
        && left_spread <= avg_true_h * 0.8 + 8.0;

    (cx, cy, oriented_w, oriented_h, is_left_aligned)
}

#[cfg(test)]
mod tests {
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
                r#box: [[154.5, 229.0], [190.0, 220.5], [193.0, 232.5], [157.5, 241.5]],
            },
            OcrLineItem {
                text: "Dain, I'm so screwed...".to_string(),
                score: 0.98,
                r#box: [[169.5, 249.5], [285.0, 216.0], [289.0, 230.5], [173.5, 264.0]],
            },
            OcrLineItem {
                text: "Things have gotten way out of".to_string(),
                score: 0.99,
                r#box: [[175.5, 284.0], [335.0, 237.0], [340.0, 254.0], [180.5, 301.0]],
            },
            OcrLineItem {
                text: "hand.".to_string(),
                score: 0.99,
                r#box: [[180.0, 299.0], [213.5, 290.0], [217.5, 304.0], [184.0, 313.0]],
            },
        ];

        let bubbles = cluster_lines(lines, 599, 787, "en");
        assert_eq!(bubbles.len(), 3);
        assert_eq!(bubbles[0].source_text, "Yewon");
        assert_eq!(bubbles[1].source_text, "Dain, I'm so screwed...");
        assert_eq!(bubbles[2].source_text, "Things have gotten way out of hand.");
        assert!(bubbles[2].is_left_aligned);
        assert!(bubbles[2].angle_rad < -0.2);
    }

    #[test]
    fn test_wide_manhwa_strip_bubble_merges_and_same_row_splits() {
        let narration = vec![
            OcrLineItem {
                text: "THAT'S HOW LONG I'VE WASTED".to_string(),
                score: 0.98,
                r#box: [[116.0, 598.0], [604.0, 598.0], [604.0, 627.0], [116.0, 627.0]],
            },
            OcrLineItem {
                text: "SEARCHING FOR A MUSIC BOX THAT".to_string(),
                score: 0.99,
                r#box: [[93.0, 636.0], [628.0, 636.0], [628.0, 665.0], [93.0, 665.0]],
            },
            OcrLineItem {
                text: "COULD BE ANYWHERE.".to_string(),
                score: 1.0,
                r#box: [[186.0, 673.0], [533.0, 673.0], [533.0, 701.0], [186.0, 701.0]],
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
}
