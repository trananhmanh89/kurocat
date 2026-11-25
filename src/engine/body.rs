//! Drawing of the built-in black cat and ragdoll: tail, body, head, ears, face, eyes and paws.
//!
//! Custom (MCP) cats are drawn by `custom_skin.rs`; the functions here hand over to it at the
//! same points where the original drawing code did.

use super::{Engine, PawMode, PawSide, PawState, Vec2, BODY_SLICES};
use crate::render::{Canvas, Gradient, LineCap, LineJoin};
use std::f32::consts::PI;

/// Fill colour of the black cat.
const CAT_BLACK: &str = "#141314";
/// Outline colour used on the ragdoll.
const INK_OUTLINE: &str = "#5d5148";

/// Traces the closed body silhouette through the measured left and right outline points.
///
/// @param ctx The canvas.
/// @param left_points Left outline points, from the head down.
/// @param right_points Right outline points, from the head down.
/// @param crown_point Top of the head.
/// @param base_point Bottom centre of the body.
/// @param head_tilt Head rotation in radians.
/// @param purr_shift Horizontal purr jitter.
fn trace_body_path(
    ctx: &mut Canvas,
    left_points: &[Vec2],
    right_points: &[Vec2],
    crown_point: Vec2,
    base_point: Vec2,
    head_tilt: f32,
    purr_shift: f32,
) {
    ctx.begin_path();
    let crown_cos = head_tilt.cos();
    let crown_sin = head_tilt.sin();
    ctx.move_to(
        left_points[0].x + 14.0 * crown_cos + 6.0 * crown_sin,
        left_points[0].y + 14.0 * crown_sin - 6.0 * crown_cos,
    );
    ctx.quadratic_curve_to(
        crown_point.x,
        crown_point.y - 2.0,
        right_points[0].x - 14.0 * crown_cos + 6.0 * crown_sin,
        right_points[0].y - 14.0 * crown_sin - 6.0 * crown_cos,
    );

    ctx.line_to(right_points[0].x, right_points[0].y);
    for index in 0..right_points.len() - 1 {
        let p0 = right_points[index];
        let p1 = right_points[index + 1];
        let mid_x = (p0.x + p1.x) * 0.5;
        let mid_y = (p0.y + p1.y) * 0.5;
        ctx.quadratic_curve_to(p0.x, p0.y, mid_x, mid_y);
    }
    let last_right = right_points[right_points.len() - 1];
    ctx.line_to(last_right.x, last_right.y);

    let last_left = left_points[left_points.len() - 1];
    ctx.quadratic_curve_to(base_point.x + purr_shift, base_point.y + 6.0, last_left.x, last_left.y);

    let mut index = left_points.len() - 1;
    while index > 0 {
        let p0 = left_points[index];
        let p1 = left_points[index - 1];
        let mid_x = (p0.x + p1.x) * 0.5;
        let mid_y = (p0.y + p1.y) * 0.5;
        ctx.quadratic_curve_to(p0.x, p0.y, mid_x, mid_y);
        index -= 1;
    }
    ctx.line_to(left_points[0].x, left_points[0].y);
    ctx.close_path();
}

/// Traces the outline of one ear in ear-local coordinates.
///
/// @param ctx The canvas.
/// @param side_sign -1 for the left ear, 1 for the right ear.
fn trace_ear_path(ctx: &mut Canvas, side_sign: f32) {
    ctx.begin_path();
    ctx.move_to(side_sign * 19.0, 20.0);
    ctx.bezier_curve_to(side_sign * 19.0, -8.0, side_sign * 12.0, -23.0, side_sign * 3.0, -21.0);
    ctx.bezier_curve_to(side_sign * -8.0, -18.0, side_sign * -16.0, -6.0, side_sign * -21.0, 8.0);
    ctx.close_path();
}

/// Traces a tapered arm from shoulder to wrist.
///
/// @param ctx The canvas.
/// @param shoulder Shoulder centre.
/// @param wrist Wrist centre.
/// @param normal Unit vector perpendicular to the arm.
/// @param shoulder_radius Half-width at the shoulder.
/// @param wrist_radius Half-width at the wrist.
fn trace_arm_path(
    ctx: &mut Canvas,
    shoulder: Vec2,
    wrist: Vec2,
    normal: Vec2,
    shoulder_radius: f32,
    wrist_radius: f32,
) {
    let (sx, sy) = (shoulder.x, shoulder.y);
    let (wx, wy) = (wrist.x, wrist.y);
    let (nx, ny) = (normal.x, normal.y);
    ctx.begin_path();
    ctx.move_to(sx + nx * shoulder_radius, sy + ny * shoulder_radius);
    ctx.quadratic_curve_to(
        (sx + wx) * 0.5 + nx * (shoulder_radius * 0.84),
        (sy + wy) * 0.5 + ny * (shoulder_radius * 0.84),
        wx + nx * wrist_radius,
        wy + ny * wrist_radius,
    );
    ctx.line_to(wx - nx * wrist_radius, wy - ny * wrist_radius);
    ctx.quadratic_curve_to(
        (sx + wx) * 0.5 - nx * (shoulder_radius * 0.84),
        (sy + wy) * 0.5 - ny * (shoulder_radius * 0.84),
        sx - nx * shoulder_radius,
        sy - ny * shoulder_radius,
    );
    ctx.close_path();
}

/// The tail as one cubic Bézier curve: start at the hip, two control points, tip.
#[derive(Clone, Copy, Debug)]
struct TailCurve {
    start_x: f32,
    start_y: f32,
    cp1_x: f32,
    cp1_y: f32,
    cp2_x: f32,
    cp2_y: f32,
    end_x: f32,
    end_y: f32,
}

/// Mixes two numbers.
///
/// @param from The value at amount 0.
/// @param to The value at amount 1.
/// @param amount How far to go from `from` to `to`.
/// @return The mixed value.
fn mix(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

impl TailCurve {
    /// Mixes this curve with another one, point by point.
    ///
    /// @param other The curve to move towards.
    /// @param amount 0 = this curve, 1 = the other curve.
    /// @return The mixed curve.
    fn blend_towards(&self, other: &TailCurve, amount: f32) -> TailCurve {
        TailCurve {
            start_x: mix(self.start_x, other.start_x, amount),
            start_y: mix(self.start_y, other.start_y, amount),
            cp1_x: mix(self.cp1_x, other.cp1_x, amount),
            cp1_y: mix(self.cp1_y, other.cp1_y, amount),
            cp2_x: mix(self.cp2_x, other.cp2_x, amount),
            cp2_y: mix(self.cp2_y, other.cp2_y, amount),
            end_x: mix(self.end_x, other.end_x, amount),
            end_y: mix(self.end_y, other.end_y, amount),
        }
    }
}

impl Engine {
    /// Draws the tail behind the cat: a soft S-hook leaving the right hip. While the cat dozes,
    /// the tail lowers and wraps around the front paws (see `draw_tail_front`).
    ///
    /// @param ctx The canvas.
    pub(crate) fn draw_tail(&self, ctx: &mut Canvas) {
        let curve = self.tail_curve();
        self.stroke_tail(ctx, &curve);
    }

    /// Draws the part of the wrapped tail that passes in front of the front paws while the cat
    /// dozes. Call it after the body and paws. Draws nothing while the cat is awake.
    ///
    /// @param ctx The canvas.
    pub(crate) fn draw_tail_front(&self, ctx: &mut Canvas) {
        if self.tail_wrap_amount() <= 0.01 {
            return;
        }
        let curve = self.tail_curve();
        let base = self.spine_point(1.0, 0.0);
        ctx.save();
        ctx.begin_path();
        ctx.rect(0.0, base.y - 3.0, self.logical_w, self.logical_h);
        ctx.clip();
        self.stroke_tail(ctx, &curve);
        ctx.restore();
    }

    /// Returns the tail curve: the seated S-hook, blended towards the pose wrapped around the
    /// front paws while the cat dozes.
    ///
    /// @return The curve in logical pixels.
    fn tail_curve(&self) -> TailCurve {
        let gentle_drift = (self.elapsed as f32 * 0.9).sin() * 1.4;
        // The tail tip flicks quickly while the cat is curious.
        let curious_twitch = (self.elapsed as f32 * 14.5).sin() * 4.2 * self.curious_level();
        let hip_right = self.spine_point(0.84, 0.0);
        let base = self.spine_point(1.0, 0.0);
        let wrap = self.tail_wrap_amount();

        let start_x = hip_right.x + 56.0;
        let start_y = hip_right.y - 2.0;

        let seated = TailCurve {
            start_x,
            start_y,
            cp1_x: start_x + 34.0,
            cp1_y: start_y + 4.0 + gentle_drift * 0.25,
            cp2_x: start_x + 38.0,
            cp2_y: start_y - 34.0 + gentle_drift * 0.45,
            end_x: start_x + 70.0 + gentle_drift * 0.35,
            end_y: start_y - 35.0 + gentle_drift + curious_twitch,
        };
        if wrap <= 0.0 {
            return seated;
        }

        // Down past the hip, under the paws, with the tip resting in front of the left paw.
        let wrapped = TailCurve {
            start_x,
            start_y,
            cp1_x: start_x + 28.0,
            cp1_y: base.y + 2.0,
            cp2_x: base.x + 62.0,
            cp2_y: base.y + 18.0,
            end_x: base.x - 26.0 + gentle_drift * 1.2,
            end_y: base.y + 10.0,
        };
        seated.blend_towards(&wrapped, wrap)
    }

    /// Strokes the tail along a curve in the current skin.
    ///
    /// @param ctx The canvas.
    /// @param curve The tail curve.
    fn stroke_tail(&self, ctx: &mut Canvas, curve: &TailCurve) {
        let start_x = curve.start_x;
        let start_y = curve.start_y;
        let cp1_x = curve.cp1_x;
        let cp1_y = curve.cp1_y;
        let cp2_x = curve.cp2_x;
        let cp2_y = curve.cp2_y;
        let end_x = curve.end_x;
        let end_y = curve.end_y;

        ctx.save();
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_join(LineJoin::Round);

        if let Some(skin) = &self.custom_skin {
            let tail_width = if skin.tail_fluffy { 20.0 } else { 16.0 };
            if let Some(outline) = &skin.outline_color {
                ctx.set_shadow_color("rgba(30, 32, 40, 0.22)");
                ctx.set_shadow_blur(4.0);
                ctx.set_shadow_offset_y(1.5);
                ctx.set_stroke_color(outline);
                ctx.set_line_width(tail_width + 3.4);
                ctx.begin_path();
                ctx.move_to(start_x, start_y);
                ctx.bezier_curve_to(cp1_x, cp1_y, cp2_x, cp2_y, end_x, end_y);
                ctx.stroke();
                ctx.set_shadow_blur(0.0);
                ctx.set_shadow_offset_y(0.0);
            } else {
                ctx.set_shadow_color("rgba(18, 17, 18, 0.42)");
                ctx.set_shadow_blur(1.6);
            }

            let mut tail_gradient = Gradient::linear(start_x, start_y, end_x, end_y);
            tail_gradient.add_color_stop(0.0, &skin.tail_color);
            tail_gradient.add_color_stop(1.0, &skin.tail_tip_color);
            ctx.set_stroke_gradient(tail_gradient);
            ctx.set_line_width(tail_width);
            ctx.begin_path();
            ctx.move_to(start_x, start_y);
            ctx.bezier_curve_to(cp1_x, cp1_y, cp2_x, cp2_y, end_x, end_y);
            ctx.stroke();

            if skin.tail_fluffy {
                ctx.set_fill_color(&skin.tail_tip_color);
                ctx.begin_path();
                ctx.ellipse(end_x + 1.0, end_y, 11.8, 10.2, -0.15, 0.0, PI * 2.0, false);
                ctx.fill();
            }

            // Tabby rings across the tail.
            if skin.tail_stripes {
                let ring_color = match &skin.stripe_color {
                    Some(stripe) if !stripe.is_empty() => stripe.clone(),
                    _ => skin.tail_tip_color.clone(),
                };
                if !ring_color.is_empty() {
                    ctx.set_stroke_color(&ring_color);
                    ctx.set_line_width(3.2);
                    ctx.set_shadow_blur(0.0);
                    for t in [0.24_f32, 0.44, 0.64, 0.82] {
                        let mt = 1.0 - t;
                        let bx = mt * mt * mt * start_x
                            + 3.0 * mt * mt * t * cp1_x
                            + 3.0 * mt * t * t * cp2_x
                            + t * t * t * end_x;
                        let by = mt * mt * mt * start_y
                            + 3.0 * mt * mt * t * cp1_y
                            + 3.0 * mt * t * t * cp2_y
                            + t * t * t * end_y;
                        let tx = 3.0 * mt * mt * (cp1_x - start_x)
                            + 6.0 * mt * t * (cp2_x - cp1_x)
                            + 3.0 * t * t * (end_x - cp2_x);
                        let ty = 3.0 * mt * mt * (cp1_y - start_y)
                            + 6.0 * mt * t * (cp2_y - cp1_y)
                            + 3.0 * t * t * (end_y - cp2_y);
                        let tangent_length = tx.hypot(ty).max(1.0);
                        let nx = (-ty / tangent_length) * (tail_width * 0.42);
                        let ny = (tx / tangent_length) * (tail_width * 0.42);
                        ctx.begin_path();
                        ctx.move_to(bx - nx, by - ny);
                        ctx.line_to(bx + nx, by + ny);
                        ctx.stroke();
                    }
                }
            }

            ctx.restore();
            return;
        }

        if self.cat_breed == super::CatBreed::Ragdoll {
            // Bushy cocoa-taupe plume tail.
            ctx.set_shadow_color("rgba(48, 38, 30, 0.22)");
            ctx.set_shadow_blur(4.0);
            ctx.set_shadow_offset_y(1.5);
            ctx.set_stroke_color("#4e4239");
            ctx.set_line_width(23.5);
            ctx.begin_path();
            ctx.move_to(start_x, start_y);
            ctx.bezier_curve_to(cp1_x, cp1_y, cp2_x, cp2_y, end_x, end_y);
            ctx.stroke();

            ctx.set_shadow_blur(0.0);
            ctx.set_shadow_offset_y(0.0);
            let mut tail_gradient = Gradient::linear(start_x, start_y, end_x, end_y);
            tail_gradient.add_color_stop(0.0, "#8d7e6f");
            tail_gradient.add_color_stop(0.5, "#726356");
            tail_gradient.add_color_stop(1.0, "#584b40");
            ctx.set_stroke_gradient(tail_gradient);
            ctx.set_line_width(20.2);
            ctx.begin_path();
            ctx.move_to(start_x, start_y);
            ctx.bezier_curve_to(cp1_x, cp1_y, cp2_x, cp2_y, end_x, end_y);
            ctx.stroke();

            // Fluffy tip.
            ctx.set_fill_color("#584b40");
            ctx.begin_path();
            ctx.ellipse(end_x + 1.0, end_y, 12.2, 10.6, -0.15, 0.0, PI * 2.0, false);
            ctx.fill();

            // Soft creamy sheen along the upper edge.
            ctx.set_stroke_color("rgba(246, 240, 232, 0.26)");
            ctx.set_line_width(5.0);
            ctx.begin_path();
            ctx.move_to(start_x + 4.0, start_y - 4.0);
            ctx.bezier_curve_to(cp1_x - 2.0, cp1_y - 4.0, cp2_x - 3.0, cp2_y - 4.0, end_x - 3.0, end_y - 4.0);
            ctx.stroke();
        } else {
            ctx.set_shadow_color("rgba(18, 17, 18, 0.45)");
            ctx.set_shadow_blur(1.6);
            ctx.set_stroke_color(CAT_BLACK);
            ctx.set_line_width(16.0);
            ctx.begin_path();
            ctx.move_to(start_x, start_y);
            ctx.bezier_curve_to(cp1_x, cp1_y, cp2_x, cp2_y, end_x, end_y);
            ctx.stroke();
        }

        ctx.restore();
    }

    /// Draws the cat's body, head, ears, face and eyes. Custom skins are drawn by
    /// `draw_custom_cat_body` instead.
    ///
    /// @param ctx The canvas.
    /// @param breath Current breathing offset.
    /// @param purr_shift Horizontal purr jitter.
    pub(crate) fn draw_cat_body(&self, ctx: &mut Canvas, breath: f32, purr_shift: f32) {
        if let Some(skin) = &self.custom_skin {
            self.draw_custom_cat_body(ctx, skin, breath, purr_shift);
            return;
        }
        let is_ragdoll = self.cat_breed == super::CatBreed::Ragdoll;

        // Outline points of the silhouette, bent by the spine and tilted with the head.
        let mut left_points: Vec<Vec2> = Vec::with_capacity(BODY_SLICES.len());
        let mut right_points: Vec<Vec2> = Vec::with_capacity(BODY_SLICES.len());
        let belly_swell = 1.0 + self.typing_hunch * 0.04;
        let head_center = self.spine_point(0.24, breath);
        let hcx = head_center.x + purr_shift;
        let hcy = head_center.y;

        for (slice_t, slice_half_width) in BODY_SLICES {
            let sp = self.spine_point(slice_t, breath);
            let widen = if slice_t > 0.45 { belly_swell } else { 1.0 };
            let half_width = slice_half_width * widen;

            if slice_t <= 0.38 {
                let tilt_weight = if slice_t <= 0.30 { 1.0 } else { (0.42 - slice_t) / 0.12 };
                let angle = self.head_tilt * tilt_weight;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let rel_y = sp.y - hcy;
                let lx0 = sp.x + purr_shift - half_width - hcx;
                let rx0 = sp.x + purr_shift + half_width - hcx;
                left_points.push(Vec2::new(hcx + lx0 * cos_a - rel_y * sin_a, hcy + lx0 * sin_a + rel_y * cos_a));
                right_points.push(Vec2::new(hcx + rx0 * cos_a - rel_y * sin_a, hcy + rx0 * sin_a + rel_y * cos_a));
            } else {
                left_points.push(Vec2::new(sp.x + purr_shift - half_width, sp.y));
                right_points.push(Vec2::new(sp.x + purr_shift + half_width, sp.y));
            }
        }

        let crown_rel_y = self.spine_point(0.11, breath).y - hcy;
        let crown_point = Vec2::new(
            hcx - crown_rel_y * self.head_tilt.sin(),
            hcy + crown_rel_y * self.head_tilt.cos(),
        );
        let base_point = self.spine_point(1.0, breath);

        ctx.save();
        if is_ragdoll {
            ctx.set_shadow_color("rgba(26, 30, 40, 0.24)");
            ctx.set_shadow_blur(5.0);
            ctx.set_shadow_offset_y(2.0);
        } else {
            ctx.set_shadow_color("rgba(18, 17, 18, 0.42)");
            ctx.set_shadow_blur(1.6);
        }

        // 1) Round hind paws peek out when a front paw is lifted.
        let any_front_lifted = self.left_paw.idle_weight < 0.7 || self.right_paw.idle_weight < 0.7;
        if any_front_lifted {
            ctx.set_fill_color(if is_ragdoll { "#f7f5f0" } else { CAT_BLACK });
            ctx.set_stroke_color(INK_OUTLINE);
            ctx.set_line_width(1.9);
            let hind_paws = [
                (base_point.x - 42.0, base_point.y - 3.0, 14.5, 11.5),
                (base_point.x + 32.0, base_point.y - 1.0, 15.0, 11.5),
            ];
            for (foot_x, foot_y, radius_x, radius_y) in hind_paws {
                ctx.begin_path();
                ctx.ellipse(foot_x, foot_y, radius_x, radius_y, 0.0, 0.0, PI * 2.0, false);
                ctx.fill();
                if is_ragdoll {
                    ctx.save();
                    ctx.set_shadow_blur(0.0);
                    ctx.set_shadow_offset_y(0.0);
                    ctx.stroke();
                    ctx.restore();
                }
            }
        }

        // 2) The body silhouette.
        trace_body_path(ctx, &left_points, &right_points, crown_point, base_point, self.head_tilt, purr_shift);
        if is_ragdoll {
            // Creamy ivory base coat.
            let mut fur_gradient = Gradient::radial(hcx, hcy + 68.0, 18.0, base_point.x, base_point.y - 60.0, 148.0);
            fur_gradient.add_color_stop(0.0, "#fbf9f3");
            fur_gradient.add_color_stop(0.68, "#f4f0e6");
            fur_gradient.add_color_stop(1.0, "#e8e2d6");
            ctx.set_fill_gradient(fur_gradient);
            ctx.fill();

            // Taupe markings, clipped to the body silhouette.
            ctx.save();
            ctx.set_shadow_blur(0.0);
            ctx.set_shadow_offset_y(0.0);
            ctx.clip();

            // 2a) Taupe flanks around both thighs.
            let mid_torso = self.spine_point(0.72, breath);
            for sign in [-1.0_f32, 1.0] {
                let mut flank_gradient = Gradient::radial(
                    mid_torso.x + purr_shift + sign * 70.0,
                    mid_torso.y + 8.0,
                    14.0,
                    mid_torso.x + purr_shift + sign * 50.0,
                    mid_torso.y + 10.0,
                    72.0,
                );
                flank_gradient.add_color_stop(0.0, "rgba(130, 115, 100, 0.78)");
                flank_gradient.add_color_stop(0.50, "rgba(156, 141, 126, 0.45)");
                flank_gradient.add_color_stop(0.82, "rgba(178, 165, 150, 0.14)");
                flank_gradient.add_color_stop(1.0, "rgba(190, 178, 164, 0.0)");
                ctx.set_fill_gradient(flank_gradient);
                ctx.begin_path();
                ctx.ellipse(mid_torso.x + purr_shift + sign * 60.0, mid_torso.y + 8.0, 52.0, 84.0, sign * -0.14, 0.0, PI * 2.0, false);
                ctx.fill();
            }

            // 2b) Fluffy chest fur tufts under the chin.
            let neck = self.spine_point(0.47, breath);
            ctx.set_stroke_color("rgba(182, 170, 156, 0.36)");
            ctx.set_line_width(1.8);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.arc(neck.x + purr_shift, neck.y - 2.0, 34.0, 0.22 * PI, 0.78 * PI, false);
            ctx.stroke();
            ctx.begin_path();
            ctx.arc(neck.x + purr_shift - 16.0, neck.y + 18.0, 14.0, 0.18 * PI, 0.72 * PI, false);
            ctx.arc(neck.x + purr_shift + 16.0, neck.y + 18.0, 14.0, 0.28 * PI, 0.82 * PI, false);
            ctx.stroke();

            // 2c) Taupe head mask with a wide white blaze.
            ctx.save();
            ctx.translate(hcx, hcy);
            ctx.rotate(self.head_tilt);

            let mask_shift = self.face_turn_x * 0.72;
            let mut cap_gradient = Gradient::linear(0.0, -40.0, 0.0, 22.0);
            cap_gradient.add_color_stop(0.0, "#8d7d6e");
            cap_gradient.add_color_stop(0.42, "#9e8e7f");
            cap_gradient.add_color_stop(0.84, "#b3a495");
            cap_gradient.add_color_stop(1.0, "rgba(186, 173, 158, 0.14)");

            ctx.set_fill_gradient(cap_gradient);
            ctx.set_shadow_color("rgba(138, 122, 108, 0.45)");
            ctx.set_shadow_blur(4.8);

            ctx.begin_path();
            ctx.move_to(-74.0, -44.0);
            ctx.line_to(74.0, -44.0);
            ctx.line_to(70.0, 20.0);
            ctx.quadratic_curve_to(mask_shift + 42.0, 18.0, mask_shift + 26.0, 9.0);
            ctx.quadratic_curve_to(mask_shift + 17.0, 0.0, mask_shift + 13.5, -13.0);
            // Soft dome of the white blaze between the brows.
            ctx.quadratic_curve_to(mask_shift + 7.0, -21.5, mask_shift, -22.5);
            ctx.quadratic_curve_to(mask_shift - 7.0, -21.5, mask_shift - 13.5, -13.0);
            ctx.quadratic_curve_to(mask_shift - 17.0, 0.0, mask_shift - 26.0, 9.0);
            ctx.quadratic_curve_to(mask_shift - 42.0, 18.0, -70.0, 20.0);
            ctx.close_path();
            ctx.fill();

            // Lighter forehead highlight between the ears.
            ctx.set_shadow_blur(5.5);
            ctx.set_fill_color("rgba(208, 194, 176, 0.58)");
            ctx.begin_path();
            ctx.ellipse(mask_shift, -32.0, 18.0, 9.0, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();

            // Darker shading along the temples below the ears.
            ctx.set_fill_color("rgba(116, 100, 86, 0.40)");
            ctx.begin_path();
            ctx.ellipse(-44.0 + mask_shift * 0.4, -10.0, 21.0, 22.0, -0.18, 0.0, PI * 2.0, false);
            ctx.ellipse(44.0 + mask_shift * 0.4, -10.0, 21.0, 22.0, 0.18, 0.0, PI * 2.0, false);
            ctx.fill();

            ctx.restore(); // end of head space
            ctx.restore(); // end of body clip

            // Thin outline so the light fur stands out on any wallpaper.
            ctx.save();
            ctx.set_shadow_blur(0.0);
            ctx.set_shadow_offset_y(0.0);
            trace_body_path(ctx, &left_points, &right_points, crown_point, base_point, self.head_tilt, purr_shift);
            ctx.set_stroke_color(INK_OUTLINE);
            ctx.set_line_width(1.8);
            ctx.set_line_join(LineJoin::Round);
            ctx.stroke();
            ctx.restore();
        } else {
            ctx.set_fill_color(CAT_BLACK);
            ctx.fill();
        }

        // 3) Short rounded ears.
        ctx.save();
        ctx.translate(hcx, hcy);
        ctx.rotate(self.head_tilt);

        let left_twitch = (self.ear_twitch_left * PI).sin() * 0.12;
        let right_twitch = (self.ear_twitch_right * PI).sin() * 0.12;
        self.draw_one_ear(ctx, -1.0, left_twitch, is_ragdoll);
        self.draw_one_ear(ctx, 1.0, right_twitch, is_ragdoll);

        ctx.set_shadow_blur(0.0);
        ctx.set_shadow_offset_y(0.0);

        let w_shift = self.face_turn_x * 0.72;

        // 4) Ragdoll blush while petted, pink nose and kitten smile.
        if is_ragdoll {
            if self.squint_amount > 0.12 {
                ctx.set_fill_color(&format!("rgba(244, 146, 166, {:.3})", 0.42 * self.squint_amount));
                ctx.begin_path();
                ctx.ellipse(w_shift - 30.0, 13.0, 8.5, 4.6, -0.08, 0.0, PI * 2.0, false);
                ctx.ellipse(w_shift + 30.0, 13.0, 8.5, 4.6, 0.08, 0.0, PI * 2.0, false);
                ctx.fill();
            }

            ctx.set_fill_color("#ee8e9e");
            ctx.begin_path();
            ctx.move_to(w_shift - 4.1, 9.6);
            ctx.quadratic_curve_to(w_shift, 8.5, w_shift + 4.1, 9.6);
            ctx.quadratic_curve_to(w_shift + 3.2, 13.8, w_shift, 14.3);
            ctx.quadratic_curve_to(w_shift - 3.2, 13.8, w_shift - 4.1, 9.6);
            ctx.close_path();
            ctx.fill();

            // Small "w" mouth.
            ctx.set_stroke_color("rgba(72, 77, 92, 0.80)");
            ctx.set_line_width(1.35);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.move_to(w_shift - 5.8, 16.6);
            ctx.quadratic_curve_to(w_shift - 2.9, 19.6, w_shift, 16.4);
            ctx.quadratic_curve_to(w_shift + 2.9, 19.6, w_shift + 5.8, 16.6);
            ctx.stroke();
        }

        // 5) Whiskers, longer on the side the face turns away from.
        ctx.set_stroke_color(if is_ragdoll { "rgba(98, 86, 76, 0.52)" } else { "rgba(205, 208, 214, 0.54)" });
        ctx.set_line_width(if is_ragdoll { 1.15 } else { 1.05 });
        ctx.set_line_cap(LineCap::Round);
        let left_length = (12.0 - self.face_turn_x * 0.55).max(8.0);
        let right_length = (12.0 + self.face_turn_x * 0.55).max(8.0);
        for index in 0..3 {
            let whisker_y = 16.0 + index as f32 * 5.0;
            let slant = 2.0 + index as f32 * 1.1;
            ctx.begin_path();
            ctx.move_to(-34.0 + w_shift, whisker_y);
            ctx.line_to(-34.0 + w_shift - left_length, whisker_y + slant);
            ctx.stroke();

            ctx.begin_path();
            ctx.move_to(34.0 + w_shift, whisker_y);
            ctx.line_to(34.0 + w_shift + right_length, whisker_y + slant);
            ctx.stroke();
        }

        // 6) Eyes (chartreuse for the black cat, blue for the ragdoll).
        self.draw_eyes(ctx, w_shift, -1.0);

        ctx.restore(); // end of head
        ctx.restore();
    }

    /// Draws one ear in head space.
    ///
    /// @param ctx The canvas (already translated and rotated into head space).
    /// @param side_sign -1 for the left ear, 1 for the right ear.
    /// @param twitch Extra rotation from an ear twitch, in radians.
    /// @param is_ragdoll True to draw the ragdoll's coloured ear.
    fn draw_one_ear(&self, ctx: &mut Canvas, side_sign: f32, twitch: f32, is_ragdoll: bool) {
        ctx.save();
        ctx.translate(side_sign * 33.0 + self.face_turn_x * 0.18, -28.0);
        ctx.rotate(side_sign * (0.08 + twitch));

        if is_ragdoll {
            // Dark espresso rims fading to a latte base.
            let mut ear_gradient = Gradient::linear(0.0, -23.0, 0.0, 18.0);
            ear_gradient.add_color_stop(0.0, "#463b33");
            ear_gradient.add_color_stop(0.42, "#625449");
            ear_gradient.add_color_stop(0.82, "#8b7c6e");
            ear_gradient.add_color_stop(1.0, "#9c8d7e");
            ctx.set_fill_gradient(ear_gradient);
            trace_ear_path(ctx, side_sign);
            ctx.fill();

            // Outer rim, open at the bottom so there is no seam with the skull.
            ctx.save();
            ctx.set_shadow_blur(0.0);
            ctx.set_shadow_offset_y(0.0);
            ctx.set_stroke_color("#473c34");
            ctx.set_line_width(1.9);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.move_to(side_sign * 19.0, 17.0);
            ctx.bezier_curve_to(side_sign * 19.0, -8.0, side_sign * 12.0, -23.0, side_sign * 3.0, -21.0);
            ctx.bezier_curve_to(side_sign * -8.0, -18.0, side_sign * -16.0, -6.0, side_sign * -19.0, 5.0);
            ctx.stroke();

            // Dusty pink inner ear.
            ctx.set_fill_color("#dec4be");
            ctx.begin_path();
            ctx.move_to(side_sign * 11.0, 10.0);
            ctx.bezier_curve_to(side_sign * 11.0, -3.0, side_sign * 7.0, -13.0, side_sign * 2.0, -11.0);
            ctx.bezier_curve_to(side_sign * -4.0, -9.0, side_sign * -9.0, -1.0, side_sign * -10.0, 8.0);
            ctx.close_path();
            ctx.fill();

            // White ear furnishings.
            ctx.set_stroke_color("rgba(252, 249, 242, 0.90)");
            ctx.set_line_width(1.6);
            ctx.begin_path();
            ctx.move_to(side_sign * -7.0, 6.0);
            ctx.quadratic_curve_to(side_sign * -1.0, 3.0, side_sign * 5.0, 6.0);
            ctx.move_to(side_sign * -4.0, 10.0);
            ctx.quadratic_curve_to(side_sign * 2.0, 7.5, side_sign * 8.0, 10.0);
            ctx.stroke();
            ctx.restore();
        } else {
            ctx.set_fill_color(CAT_BLACK);
            trace_ear_path(ctx, side_sign);
            ctx.fill();
        }
        ctx.restore();
    }

    /// Draws both eyes in head space: closed lines (privacy), lazy slits (petting) or open eyes
    /// that follow the cursor.
    ///
    /// @param ctx The canvas (in head space).
    /// @param cx Horizontal centre between the eyes.
    /// @param eye_y Vertical eye position.
    fn draw_eyes(&self, ctx: &mut Canvas, cx: f32, eye_y: f32) {
        let is_ragdoll = self.cat_breed == super::CatBreed::Ragdoll;
        let blink_value = if self.is_blinking { 1.0 - (self.blink_progress - 1.0).abs() } else { 0.0 };
        let closedness = blink_value.max(self.squint_amount * 0.85).max(self.privacy_close).max(self.sleep_amount());
        let sleep_line = self.sleep_eye_line_amount();
        let sleep_line_color = if is_ragdoll { "#212533" } else { "#d4f044" };

        let eye_xs = [cx - 23.0, cx + 23.0];
        let curious = self.curious_level();
        let radius_x = (14.8 + self.typing_hunch * 1.0) * (1.0 + curious * 0.06);
        let radius_y = (13.4 - self.typing_hunch * 0.5) * (1.0 + curious * 0.09);

        for (index, eye_x) in eye_xs.into_iter().enumerate() {
            ctx.save();
            ctx.translate(eye_x, eye_y);

            if self.privacy_close > 0.45 || sleep_line >= 1.0 {
                ctx.set_stroke_color(if is_ragdoll { "#212533" } else { "#d4f044" });
                ctx.set_line_width(2.9);
                ctx.set_line_cap(LineCap::Round);
                ctx.begin_path();
                ctx.move_to(-13.5, -0.5);
                ctx.quadratic_curve_to(0.0, 5.2, 13.5, -0.5);
                ctx.stroke();
                ctx.restore();
                continue;
            }

            if self.squint_amount > 0.38 && blink_value < 0.3 {
                if is_ragdoll {
                    // Lazy slits with a blue shimmer underneath.
                    ctx.set_stroke_color("rgba(174, 224, 255, 0.72)");
                    ctx.set_line_width(4.4);
                    ctx.set_line_cap(LineCap::Round);
                    ctx.begin_path();
                    ctx.move_to(-12.5, 0.2);
                    ctx.quadratic_curve_to(0.0, 2.2, 12.5, 1.8);
                    ctx.stroke();

                    ctx.set_stroke_color("#212533");
                    ctx.set_line_width(2.7);
                    ctx.begin_path();
                    ctx.move_to(-13.5, -0.5);
                    ctx.quadratic_curve_to(0.0, 1.4, 13.5, 1.2);
                    ctx.stroke();
                } else {
                    // Thin chartreuse slits.
                    ctx.set_stroke_color("#d4f044");
                    ctx.set_line_width(2.9);
                    ctx.set_line_cap(LineCap::Round);
                    ctx.begin_path();
                    ctx.move_to(-13.5, -0.5);
                    ctx.quadratic_curve_to(0.0, 1.4, 13.5, 1.2);
                    ctx.stroke();
                }
                ctx.restore();
                continue;
            }

            let open_radius_y = (radius_y * (1.0 - closedness)).max(1.2);
            let curious_tilt = (if index == 0 { -0.1 } else { 0.1 }) * (0.25 + self.typing_hunch * 0.75);
            ctx.rotate(curious_tilt * (1.0 - sleep_line));

            if is_ragdoll {
                let mut iris_gradient = Gradient::linear(0.0, -open_radius_y, 0.0, open_radius_y);
                iris_gradient.add_color_stop(0.0, "#2c5594");
                iris_gradient.add_color_stop(0.50, "#5a94d6");
                iris_gradient.add_color_stop(1.0, "#a3d2f2");
                ctx.set_fill_gradient(iris_gradient);
            } else {
                ctx.set_fill_color("#d4f044");
            }

            ctx.begin_path();
            ctx.ellipse(0.0, 0.0, radius_x, open_radius_y, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();

            ctx.save();
            ctx.clip();

            if open_radius_y > 3.2 {
                // Pupil follows the cursor and rests low when looking at the keyboard.
                // The ragdoll always has catchlights; the black cat gets them when curious.
                let pupil_color = if is_ragdoll { "#101624" } else { "#0e0e10" };
                super::curious::draw_pupil(
                    ctx,
                    self.eye_look_x,
                    self.eye_look_y,
                    radius_x,
                    open_radius_y,
                    curious,
                    pupil_color,
                    is_ragdoll,
                );
            }

            ctx.restore();

            if is_ragdoll && open_radius_y > 2.2 {
                // Thin rim around the iris.
                ctx.set_stroke_color("rgba(34, 39, 54, 0.85)");
                ctx.set_line_width(1.25);
                ctx.begin_path();
                ctx.ellipse(0.0, 0.0, radius_x, open_radius_y, 0.0, 0.0, PI * 2.0, false);
                ctx.stroke();
            }

            if sleep_line > 0.0 {
                super::sleep::draw_sleeping_eye_line(ctx, sleep_line_color, sleep_line);
            }

            ctx.restore();
        }
    }

    /// Draws a plump arm from shoulder (sx, sy) to a round paw at (px, py). Custom skins are
    /// drawn by `draw_custom_plump_arm` instead.
    ///
    /// @param ctx The canvas.
    /// @param side Which paw this is.
    /// @param sx Shoulder x.
    /// @param sy Shoulder y.
    /// @param px Paw x.
    /// @param py Paw y.
    /// @param squash How much the paw is squashed by a key impact (0..1).
    /// @param with_crease True to draw the light separation lines where the arm crosses the body.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_plump_cat_arm(
        &self,
        ctx: &mut Canvas,
        side: PawSide,
        sx: f32,
        sy: f32,
        px: f32,
        py: f32,
        squash: f32,
        with_crease: bool,
    ) {
        let dx = px - sx;
        let dy = py - sy;
        let length = dx.hypot(dy).max(1.0);
        let ux = dx / length;
        let uy = dy / length;
        let nx = -uy;
        let ny = ux;

        let wrist_distance = (length - 8.0).max(6.0);
        let wx = sx + ux * wrist_distance;
        let wy = sy + uy * wrist_distance;

        let shoulder_radius = 14.2;
        let wrist_radius = 9.4;
        let paw_radius_x = 14.5 * (1.0 + squash * 0.14);
        let paw_radius_y = 11.8 * (1.0 - squash * 0.12);

        if let Some(skin) = &self.custom_skin {
            self.draw_custom_plump_arm(ctx, skin, side, sx, sy, px, py, squash, with_crease);
            return;
        }

        let shoulder = Vec2::new(sx, sy);
        let wrist = Vec2::new(wx, wy);
        let normal = Vec2::new(nx, ny);
        let is_ragdoll = self.cat_breed == super::CatBreed::Ragdoll;
        ctx.save();

        if is_ragdoll {
            // White mitten arm that blends into the white chest.
            let mut arm_gradient = Gradient::linear(sx, sy, px, py);
            arm_gradient.add_color_stop(0.0, "#f5f1e8");
            arm_gradient.add_color_stop(0.5, "#f9f6ef");
            arm_gradient.add_color_stop(1.0, "#fcfaf4");
            ctx.set_fill_gradient(arm_gradient);

            trace_arm_path(ctx, shoulder, wrist, normal, shoulder_radius, wrist_radius);
            ctx.fill();

            // White shoulder merge.
            ctx.begin_path();
            ctx.arc(sx, sy, shoulder_radius, 0.0, PI * 2.0, false);
            ctx.fill();

            // Taupe patch on the viewer's right forearm, like the real cat.
            if side == PawSide::Right && length > 26.0 {
                ctx.save();
                trace_arm_path(ctx, shoulder, wrist, normal, shoulder_radius, wrist_radius);
                ctx.clip();
                let spot_x = sx + ux * (length * 0.64) + nx * 2.0;
                let spot_y = sy + uy * (length * 0.64) + ny * 2.0;
                let arm_angle = dy.atan2(dx);
                ctx.set_fill_color("rgba(134, 118, 102, 0.84)");
                ctx.set_shadow_color("rgba(134, 118, 102, 0.45)");
                ctx.set_shadow_blur(2.5);
                ctx.begin_path();
                ctx.ellipse(spot_x, spot_y, (length * 0.16).min(13.5), 8.2, arm_angle, 0.0, PI * 2.0, false);
                ctx.fill();
                ctx.restore();
            }

            // Soft contact shadow only under the paw ball.
            ctx.save();
            ctx.set_shadow_color("rgba(46, 38, 30, 0.22)");
            ctx.set_shadow_blur(4.0);
            ctx.set_shadow_offset_y(1.5);
            ctx.set_fill_color("#fcfaf4");
            ctx.begin_path();
            ctx.ellipse(px, py, paw_radius_x, paw_radius_y, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();
            ctx.restore();

            // Taupe side lines that fade out towards the shoulder.
            ctx.set_line_width(1.65);
            ctx.set_line_cap(LineCap::Round);
            for sign in [-1.0_f32, 1.0] {
                let start_fraction = if with_crease { 0.16 } else { 0.08 };
                let esx = sx + ux * (length * start_fraction) + nx * sign * (shoulder_radius * (1.0 - start_fraction * 0.16));
                let esy = sy + uy * (length * start_fraction) + ny * sign * (shoulder_radius * (1.0 - start_fraction * 0.16));
                let eex = wx + nx * sign * wrist_radius;
                let eey = wy + ny * sign * wrist_radius;

                let mut edge_gradient = Gradient::linear(esx, esy, eex, eey);
                edge_gradient.add_color_stop(0.0, "rgba(93, 81, 72, 0.0)");
                edge_gradient.add_color_stop(0.24, "rgba(93, 81, 72, 0.78)");
                edge_gradient.add_color_stop(1.0, "rgba(93, 81, 72, 0.92)");

                ctx.set_stroke_gradient(edge_gradient);
                ctx.begin_path();
                ctx.move_to(esx, esy);
                ctx.quadratic_curve_to(
                    (sx + wx) * 0.5 + nx * sign * (shoulder_radius * 0.84),
                    (sy + wy) * 0.5 + ny * sign * (shoulder_radius * 0.84),
                    eex,
                    eey,
                );
                ctx.stroke();
            }

            // Outline arc around the paw so it reads against white keys.
            let paw_angle = dy.atan2(dx);
            ctx.set_stroke_color("rgba(93, 81, 72, 0.92)");
            ctx.begin_path();
            ctx.ellipse(px, py, paw_radius_x, paw_radius_y, 0.0, paw_angle - PI * 0.62, paw_angle + PI * 0.62, false);
            ctx.stroke();

            ctx.restore();
            return;
        }

        ctx.set_shadow_color("rgba(18, 17, 18, 0.42)");
        ctx.set_shadow_blur(1.6);
        ctx.set_fill_color(CAT_BLACK);

        // Arm tapering from shoulder to wrist.
        trace_arm_path(ctx, shoulder, wrist, normal, shoulder_radius, wrist_radius);
        ctx.fill();

        // Shoulder merge.
        ctx.begin_path();
        ctx.arc(sx, sy, shoulder_radius, 0.0, PI * 2.0, false);
        ctx.fill();

        // Round velvet paw ball.
        ctx.begin_path();
        ctx.ellipse(px, py, paw_radius_x, paw_radius_y, 0.0, 0.0, PI * 2.0, false);
        ctx.fill();

        ctx.set_shadow_blur(0.0);

        // Light separation lines where the arm crosses the black body.
        if with_crease && length > 28.0 {
            let start_fraction = 0.18;
            let end_fraction = 0.72;
            for sign in [-1.0_f32, 1.0] {
                let csx = sx + ux * (length * start_fraction) + nx * sign * (shoulder_radius * 0.86);
                let csy = sy + uy * (length * start_fraction) + ny * sign * (shoulder_radius * 0.86);
                let cex = sx + ux * (length * end_fraction) + nx * sign * (wrist_radius * 1.02);
                let cey = sy + uy * (length * end_fraction) + ny * sign * (wrist_radius * 1.02);

                let mut crease_gradient = Gradient::linear(csx, csy, cex, cey);
                crease_gradient.add_color_stop(0.0, "rgba(216, 218, 222, 0.0)");
                crease_gradient.add_color_stop(0.22, "rgba(216, 218, 222, 0.65)");
                crease_gradient.add_color_stop(0.84, "rgba(216, 218, 222, 0.56)");
                crease_gradient.add_color_stop(1.0, "rgba(216, 218, 222, 0.0)");

                ctx.set_stroke_gradient(crease_gradient);
                ctx.set_line_width(1.3);
                ctx.set_line_cap(LineCap::Round);
                ctx.begin_path();
                ctx.move_to(csx, csy);
                ctx.line_to(cex, cey);
                ctx.stroke();
            }
        }

        ctx.restore();
    }

    /// Draws a bent arm (shoulder → elbow → paw curled against the belly). Custom skins are
    /// drawn by `draw_custom_bent_arm` instead.
    ///
    /// @param ctx The canvas.
    /// @param paw The paw.
    /// @param breath Current breathing offset.
    /// @param alpha Blend weight of the curled pose (0..1).
    fn draw_bent_curled_arm(&self, ctx: &mut Canvas, paw: &PawState, breath: f32, alpha: f32) {
        let shoulder = self.get_live_shoulder(paw, breath);
        let curled = self.get_live_curled_paw_pos(paw, breath);
        let sign = if paw.side == PawSide::Left { -1.0 } else { 1.0 };
        // The elbow bends slightly outward and down before the forearm curls inward.
        let elbow_x = shoulder.x + sign * 8.0;
        let elbow_y = curled.y + 8.0;

        if let Some(skin) = &self.custom_skin {
            self.draw_custom_bent_arm(ctx, skin, paw, breath, alpha);
            return;
        }

        let is_ragdoll = self.cat_breed == super::CatBreed::Ragdoll;
        ctx.save();
        ctx.set_global_alpha(alpha);
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_join(LineJoin::Round);

        // The bent arm itself.
        ctx.set_stroke_color(if is_ragdoll { "#f6f2ea" } else { CAT_BLACK });
        ctx.set_line_width(20.0);
        ctx.begin_path();
        ctx.move_to(shoulder.x, shoulder.y);
        ctx.quadratic_curve_to(elbow_x, elbow_y, curled.x, curled.y);
        ctx.stroke();

        // Ragdoll taupe patch on the viewer's right forearm.
        if is_ragdoll && paw.side == PawSide::Right {
            ctx.save();
            ctx.set_fill_color("rgba(134, 118, 102, 0.78)");
            ctx.begin_path();
            ctx.ellipse((elbow_x + curled.x) * 0.5 + 2.0, (elbow_y + curled.y) * 0.5 + 1.0, 7.5, 5.2, -0.35, 0.0, PI * 2.0, false);
            ctx.fill();
            ctx.restore();
        }

        ctx.set_fill_color(if is_ragdoll { "#fcfaf4" } else { CAT_BLACK });
        ctx.begin_path();
        ctx.ellipse(curled.x, curled.y, 12.5, 11.0, sign * 0.2, 0.0, PI * 2.0, false);
        ctx.fill();

        // Contour along the forearm and paw.
        let mut contour_gradient = Gradient::linear(elbow_x, elbow_y - 12.0, curled.x, curled.y + 12.0);
        if is_ragdoll {
            contour_gradient.add_color_stop(0.0, "rgba(93, 81, 72, 0.0)");
            contour_gradient.add_color_stop(0.28, "rgba(93, 81, 72, 0.68)");
            contour_gradient.add_color_stop(0.86, "rgba(93, 81, 72, 0.78)");
            contour_gradient.add_color_stop(1.0, "rgba(93, 81, 72, 0.35)");
        } else {
            contour_gradient.add_color_stop(0.0, "rgba(214, 216, 220, 0.0)");
            contour_gradient.add_color_stop(0.30, "rgba(214, 216, 220, 0.65)");
            contour_gradient.add_color_stop(0.85, "rgba(220, 222, 226, 0.72)");
            contour_gradient.add_color_stop(1.0, "rgba(220, 222, 226, 0.25)");
        }

        ctx.set_stroke_gradient(contour_gradient);
        ctx.set_line_width(if is_ragdoll { 1.65 } else { 1.4 });
        ctx.begin_path();
        ctx.move_to(elbow_x + sign * 8.0, elbow_y - 16.0);
        ctx.quadratic_curve_to(elbow_x + sign * 4.0, elbow_y + 12.0, curled.x + sign * 2.0, curled.y + 10.0);
        ctx.quadratic_curve_to(curled.x - sign * 11.0, curled.y + 8.0, curled.x - sign * 12.0, curled.y - 4.0);
        ctx.stroke();

        ctx.restore();
    }

    /// Draws one front paw, blending its three poses by weight: seated leg, curled arm and
    /// active (poking or mouse-holding) arm with motion ghosts.
    ///
    /// @param ctx The canvas.
    /// @param paw The paw.
    /// @param breath Current breathing offset.
    pub(crate) fn draw_paw(&self, ctx: &mut Canvas, paw: &PawState, breath: f32) {
        // 1) Seated front leg, or a planted diagonal support leg while the other paw mouses.
        if paw.idle_weight > 0.05 {
            ctx.save();
            ctx.set_global_alpha(paw.idle_weight);

            if paw.side == PawSide::Right && self.left_paw.mode == PawMode::Mouse {
                let shoulder = self.get_live_shoulder(paw, breath);
                let base = self.spine_point(1.0, breath);
                self.draw_plump_cat_arm(ctx, PawSide::Right, shoulder.x - 4.0, shoulder.y + 8.0, base.x + 28.0, base.y - 2.0, 0.0, true);
            } else {
                // Seated leg: an outer curved leg line and an inner leg-gap line.
                let top = self.spine_point(0.66, breath);
                let middle = self.spine_point(0.84, breath);
                let bottom = self.spine_point(1.0, breath);
                let sign = if paw.side == PawSide::Left { -1.0 } else { 1.0 };

                let outer_top_x = top.x + sign * 41.0;
                let outer_bottom_x = bottom.x + sign * 35.0;
                let inner_top_x = top.x + sign * 6.5;
                let inner_bottom_x = bottom.x + sign * 9.5;

                if let Some(skin) = &self.custom_skin {
                    self.draw_custom_idle_leg_details(
                        ctx,
                        skin,
                        paw,
                        top,
                        middle,
                        bottom,
                        sign,
                        outer_top_x,
                        outer_bottom_x,
                        inner_top_x,
                        inner_bottom_x,
                    );
                    ctx.restore();
                    return;
                }

                // Ragdoll taupe patch on the viewer's right seated leg.
                let is_ragdoll = self.cat_breed == super::CatBreed::Ragdoll;
                if is_ragdoll && paw.side == PawSide::Right {
                    ctx.save();
                    ctx.set_fill_color("rgba(134, 118, 102, 0.80)");
                    ctx.set_shadow_color("rgba(134, 118, 102, 0.40)");
                    ctx.set_shadow_blur(3.0);
                    ctx.begin_path();
                    ctx.ellipse(middle.x + 21.0, middle.y + 12.0, 9.5, 13.5, 0.22, 0.0, PI * 2.0, false);
                    ctx.fill();
                    ctx.restore();
                }

                let mut line_gradient = Gradient::linear(top.x, top.y, bottom.x, bottom.y);
                if is_ragdoll {
                    line_gradient.add_color_stop(0.0, "rgba(93, 81, 72, 0.0)");
                    line_gradient.add_color_stop(0.18, "rgba(93, 81, 72, 0.62)");
                    line_gradient.add_color_stop(0.92, "rgba(93, 81, 72, 0.68)");
                    line_gradient.add_color_stop(1.0, "rgba(93, 81, 72, 0.28)");
                } else {
                    line_gradient.add_color_stop(0.0, "rgba(210, 212, 216, 0.0)");
                    line_gradient.add_color_stop(0.18, "rgba(210, 212, 216, 0.64)");
                    line_gradient.add_color_stop(0.92, "rgba(210, 212, 216, 0.62)");
                    line_gradient.add_color_stop(1.0, "rgba(210, 212, 216, 0.25)");
                }

                ctx.set_stroke_gradient(line_gradient);
                ctx.set_line_width(if is_ragdoll { 1.55 } else { 1.4 });
                ctx.set_line_cap(LineCap::Round);

                // Outer leg line, curving inward towards the paw.
                ctx.begin_path();
                ctx.move_to(outer_top_x, top.y);
                ctx.quadratic_curve_to(middle.x + sign * 41.0, middle.y, outer_bottom_x, bottom.y - 1.0);
                ctx.stroke();

                // Inner leg-gap line.
                ctx.begin_path();
                ctx.move_to(inner_top_x, top.y + 10.0);
                ctx.line_to(inner_bottom_x, bottom.y - 1.0);
                ctx.stroke();
            }
            ctx.restore();
        }

        // 2) Bent arm with the paw curled against the chest.
        if paw.curl_weight > 0.05 {
            self.draw_bent_curled_arm(ctx, paw, breath, paw.curl_weight);
        }

        // 3) Poking or mouse-holding arm.
        let active_arm_weight = (1.0 - paw.idle_weight - paw.curl_weight).max(0.0);
        if active_arm_weight > 0.05 {
            let shoulder = self.get_live_shoulder(paw, breath);
            let paw_x = paw.pos.x;
            let mouse_click_drop = if paw.mode == PawMode::Mouse {
                self.mouse_left_press.max(self.mouse_right_press).max(self.mouse_middle_press) * 1.8
            } else {
                0.0
            };
            let paw_y = paw.pos.y + mouse_click_drop;

            ctx.save();
            ctx.set_global_alpha((active_arm_weight * 1.2).min(1.0));

            for smear in &paw.smears {
                ctx.save();
                ctx.set_global_alpha((smear.alpha * active_arm_weight).max(0.0));
                self.draw_plump_cat_arm(ctx, paw.side, smear.sx, smear.sy, smear.px, smear.py, 0.0, false);
                ctx.restore();
            }

            // Plain velvet in open air while mousing; separation lines when poking across the body.
            self.draw_plump_cat_arm(ctx, paw.side, shoulder.x, shoulder.y, paw_x, paw_y, paw.impact_squash, paw.mode != PawMode::Mouse);
            ctx.restore();
        }
    }
}
