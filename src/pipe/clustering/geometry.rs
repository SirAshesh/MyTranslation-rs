use crate::model::LineInfo;

pub(super) const MIN_TILT_RAD: f32 = 0.026;

pub(super) fn compute_oriented_metrics(quad: &[[f32; 2]; 4]) -> (f32, f32, f32) {
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

pub(super) fn project_quad_on_angle(
    quad: &[[f32; 2]; 4],
    angle_rad: f32,
) -> (f32, f32, f32, f32) {
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

pub(super) fn normalize_quad(
    raw_box: [[f32; 2]; 4],
    img_width: u32,
    img_height: u32,
) -> [[f32; 2]; 4] {
    let max_w = img_width as f32;
    let max_h = img_height as f32;
    let mut out = raw_box;
    for p in &mut out {
        p[0] = p[0].clamp(0.0, max_w);
        p[1] = p[1].clamp(0.0, max_h);
    }
    out
}

pub(super) fn compute_bubble_oriented_box(
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
