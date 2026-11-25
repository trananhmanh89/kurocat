//! Drawing of custom (MCP / `cats.json`) cats: body, coat pattern, spots, ears, face, eyes and arms.
//!
//! Custom cats share the black cat's silhouette and animation; only colours and markings differ.
//! All colours come from a [`ResolvedSkin`].

use super::{Engine, PawSide, PawState, Vec2, BODY_SLICES};
use crate::cats::CatSpot;
use crate::render::{Canvas, Gradient, LineCap, LineJoin};
use crate::skins::{CatPattern, ResolvedSkin};
use std::f32::consts::PI;

/// Points of the cat outline, shared by the fill, the clip and the outline stroke.
struct BodyOutline {
    left_points: Vec<Vec2>,
    right_points: Vec<Vec2>,
    crown: Vec2,
    base: Vec2,
    purr_shift: f32,
    head_tilt: f32,
}

/// Traces the closed body outline (head, flanks and seat) as the current path.
///
/// @param ctx The canvas.
/// @param outline The outline points.
fn trace_body_path(ctx: &mut Canvas, outline: &BodyOutline) {
    let left = &outline.left_points;
    let right = &outline.right_points;
    let crown_cos = outline.head_tilt.cos();
    let crown_sin = outline.head_tilt.sin();
    ctx.begin_path();
    ctx.move_to(
        left[0].x + 14.0 * crown_cos + 6.0 * crown_sin,
        left[0].y + 14.0 * crown_sin - 6.0 * crown_cos,
    );
    ctx.quadratic_curve_to(
        outline.crown.x,
        outline.crown.y - 2.0,
        right[0].x - 14.0 * crown_cos + 6.0 * crown_sin,
        right[0].y - 14.0 * crown_sin - 6.0 * crown_cos,
    );
    ctx.line_to(right[0].x, right[0].y);
    for index in 0..right.len() - 1 {
        let p0 = right[index];
        let p1 = right[index + 1];
        ctx.quadratic_curve_to(p0.x, p0.y, (p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
    }
    let last_right = right[right.len() - 1];
    let last_left = left[left.len() - 1];
    ctx.line_to(last_right.x, last_right.y);
    ctx.quadratic_curve_to(outline.base.x + outline.purr_shift, outline.base.y + 6.0, last_left.x, last_left.y);
    let mut index = left.len() - 1;
    while index > 0 {
        let p0 = left[index];
        let p1 = left[index - 1];
        ctx.quadratic_curve_to(p0.x, p0.y, (p0.x + p1.x) * 0.5, (p0.y + p1.y) * 0.5);
        index -= 1;
    }
    ctx.line_to(left[0].x, left[0].y);
    ctx.close_path();
}

/// Returns the spot-list name of an arm.
///
/// @param side Which paw.
/// @return "leftArm" or "rightArm".
fn arm_part_name(side: PawSide) -> &'static str {
    match side {
        PawSide::Left => "leftArm",
        PawSide::Right => "rightArm",
    }
}

/// Fills one spot ellipse with the spot's colour and opacity.
///
/// @param ctx The canvas.
/// @param spot The spot (for colour and rotation).
/// @param x Centre x.
/// @param y Centre y.
/// @param radius_x Horizontal radius.
/// @param radius_y Vertical radius.
/// @param rotation Ellipse rotation in radians.
/// @param alpha Opacity.
#[allow(clippy::too_many_arguments)]
fn fill_spot(ctx: &mut Canvas, spot: &CatSpot, x: f32, y: f32, radius_x: f32, radius_y: f32, rotation: f32, alpha: f32) {
    ctx.save();
    ctx.set_global_alpha(alpha);
    ctx.set_fill_color(&spot.color);
    ctx.begin_path();
    ctx.ellipse(x, y, radius_x, radius_y, rotation, 0.0, PI * 2.0, false);
    ctx.fill();
    ctx.restore();
}

impl Engine {
    /// Draws the body, coat pattern, spots, ears and face of a custom cat.
    ///
    /// @param ctx The canvas, in pet coordinates.
    /// @param s The resolved skin.
    /// @param breath Breathing offset (about -1..1).
    /// @param purr_shift Small horizontal jitter while purring.
    pub(crate) fn draw_custom_cat_body(&self, ctx: &mut Canvas, s: &ResolvedSkin, breath: f32, purr_shift: f32) {
        let mut left_points: Vec<Vec2> = Vec::new();
        let mut right_points: Vec<Vec2> = Vec::new();
        let belly_swell = 1.0 + self.typing_hunch * 0.04;
        let head_center = self.spine_point(0.24, breath);
        let hcx = head_center.x + purr_shift;
        let hcy = head_center.y;

        for (slice_t, slice_half_width) in BODY_SLICES {
            let sp = self.spine_point(slice_t, breath);
            let widen = if slice_t > 0.45 { belly_swell } else { 1.0 };
            let hw = slice_half_width * widen;

            if slice_t <= 0.38 {
                // The head part of the outline rotates with the head tilt.
                let tilt_weight = if slice_t <= 0.30 { 1.0 } else { (0.42 - slice_t) / 0.12 };
                let angle = self.head_tilt * tilt_weight;
                let cos_a = angle.cos();
                let sin_a = angle.sin();
                let rel_y = sp.y - hcy;
                let lx0 = sp.x + purr_shift - hw - hcx;
                let rx0 = sp.x + purr_shift + hw - hcx;
                left_points.push(Vec2::new(hcx + lx0 * cos_a - rel_y * sin_a, hcy + lx0 * sin_a + rel_y * cos_a));
                right_points.push(Vec2::new(hcx + rx0 * cos_a - rel_y * sin_a, hcy + rx0 * sin_a + rel_y * cos_a));
            } else {
                left_points.push(Vec2::new(sp.x + purr_shift - hw, sp.y));
                right_points.push(Vec2::new(sp.x + purr_shift + hw, sp.y));
            }
        }

        let crown_rel_y = self.spine_point(0.11, breath).y - hcy;
        let crown = Vec2::new(
            hcx - crown_rel_y * self.head_tilt.sin(),
            hcy + crown_rel_y * self.head_tilt.cos(),
        );
        let base = self.spine_point(1.0, breath);
        let outline = BodyOutline {
            left_points,
            right_points,
            crown,
            base,
            purr_shift,
            head_tilt: self.head_tilt,
        };

        ctx.save();
        ctx.set_shadow_color(if s.is_dark_base { "rgba(18, 17, 18, 0.42)" } else { "rgba(30, 32, 42, 0.22)" });
        ctx.set_shadow_blur(if s.is_dark_base { 1.6 } else { 5.0 });
        if !s.is_dark_base {
            ctx.set_shadow_offset_y(2.0);
        }

        // 1) Hind paws on the desk, visible when a front paw is lifted.
        let any_front_lifted = self.left_paw.idle_weight < 0.7 || self.right_paw.idle_weight < 0.7;
        if any_front_lifted {
            let edge_color = s.paw_outline_color.as_ref().or(s.outline_color.as_ref());
            ctx.set_fill_color(&s.paw_color);
            match edge_color {
                Some(color) => ctx.set_stroke_color(color),
                None => ctx.set_stroke_color("transparent"),
            }
            ctx.set_line_width(1.85);
            let hind_paws = [
                (base.x - 42.0, base.y - 3.0, 14.5, 11.5),
                (base.x + 32.0, base.y - 1.0, 15.0, 11.5),
            ];
            for (foot_x, foot_y, radius_x, radius_y) in hind_paws {
                ctx.begin_path();
                ctx.ellipse(foot_x, foot_y, radius_x, radius_y, 0.0, 0.0, PI * 2.0, false);
                ctx.fill();
                if edge_color.is_some() {
                    ctx.save();
                    ctx.set_shadow_blur(0.0);
                    ctx.set_shadow_offset_y(0.0);
                    ctx.stroke();
                    ctx.restore();
                }
            }
        }

        // 2) The body silhouette with a soft radial fur gradient.
        trace_body_path(ctx, &outline);
        let mut fur_gradient = Gradient::radial(hcx, hcy + 65.0, 18.0, base.x, base.y - 60.0, 150.0);
        fur_gradient.add_color_stop(0.0, &s.fur_color);
        fur_gradient.add_color_stop(1.0, &s.fur_secondary_color);
        ctx.set_fill_gradient(fur_gradient);
        ctx.fill();

        // 3) Coat pattern and custom spots, clipped to the body.
        ctx.save();
        ctx.set_shadow_blur(0.0);
        ctx.set_shadow_offset_y(0.0);
        ctx.clip();

        let mid_torso = self.spine_point(0.70, breath);
        let chest = self.spine_point(0.56, breath);

        if let Some(flank_color) = &s.flank_color {
            for sign in [-1.0_f32, 1.0] {
                ctx.save();
                ctx.set_global_alpha(0.62);
                ctx.set_fill_color(flank_color);
                ctx.set_shadow_color(flank_color);
                ctx.set_shadow_blur(10.0);
                ctx.begin_path();
                ctx.ellipse(
                    mid_torso.x + purr_shift + sign * 62.0,
                    mid_torso.y + 6.0,
                    48.0,
                    82.0,
                    sign * -0.14,
                    0.0,
                    PI * 2.0,
                    false,
                );
                ctx.fill();
                ctx.restore();
            }
        }

        if let Some(bib_color) = &s.belly_bib_color {
            ctx.set_fill_color(bib_color);
            ctx.begin_path();
            ctx.ellipse(chest.x + purr_shift, chest.y + 38.0, 36.0, 62.0, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();
        }

        if s.pattern == CatPattern::Tabby {
            if let Some(stripe_color) = &s.stripe_color {
                ctx.set_stroke_color(stripe_color);
                ctx.set_line_cap(LineCap::Round);
                ctx.set_line_width(4.6);
                ctx.set_global_alpha(0.65);
                for sign in [-1.0_f32, 1.0] {
                    for index in 0..4 {
                        let stripe_y = mid_torso.y - 26.0 + index as f32 * 20.0;
                        ctx.begin_path();
                        ctx.move_to(mid_torso.x + purr_shift + sign * 74.0, stripe_y);
                        ctx.quadratic_curve_to(
                            mid_torso.x + purr_shift + sign * 56.0,
                            stripe_y + 3.0,
                            mid_torso.x + purr_shift + sign * 42.0,
                            stripe_y + 7.0,
                        );
                        ctx.stroke();
                    }
                }
                ctx.set_global_alpha(1.0);
            }
        }

        if s.pattern == CatPattern::Calico {
            let orange_patch = s.mask_color.clone().unwrap_or_else(|| "#ea9446".to_string());
            let dark_patch = s.calico_patch_color.clone().unwrap_or_else(|| "#3c3634".to_string());
            ctx.save();
            ctx.set_shadow_blur(4.0);
            ctx.set_shadow_color(&orange_patch);
            ctx.set_fill_color(&orange_patch);
            ctx.begin_path();
            ctx.ellipse(mid_torso.x + purr_shift - 54.0, mid_torso.y + 10.0, 38.0, 52.0, -0.2, 0.0, PI * 2.0, false);
            ctx.fill();

            ctx.set_shadow_color(&dark_patch);
            ctx.set_fill_color(&dark_patch);
            ctx.begin_path();
            ctx.ellipse(mid_torso.x + purr_shift + 56.0, mid_torso.y - 4.0, 36.0, 56.0, 0.18, 0.0, PI * 2.0, false);
            ctx.fill();
            ctx.restore();
        }

        for spot in &s.spots {
            if spot.part != "body" {
                continue;
            }
            fill_spot(
                ctx,
                spot,
                mid_torso.x + purr_shift + spot.x as f32 * 65.0,
                mid_torso.y + spot.y as f32 * 65.0,
                (spot.rx as f32).max(2.0),
                (spot.ry as f32).max(2.0),
                spot.rotation as f32,
                spot.opacity as f32,
            );
        }

        // Face markings, drawn in head space so they turn with the head tilt.
        ctx.save();
        ctx.translate(hcx, hcy);
        ctx.rotate(self.head_tilt);
        let mask_shift = self.face_turn_x * 0.72;
        self.draw_custom_face_pattern(ctx, s, mask_shift);

        for spot in &s.spots {
            if spot.part != "head" {
                continue;
            }
            fill_spot(
                ctx,
                spot,
                mask_shift + spot.x as f32 * 52.0,
                spot.y as f32 * 36.0,
                (spot.rx as f32).max(2.0),
                (spot.ry as f32).max(2.0),
                spot.rotation as f32,
                spot.opacity as f32,
            );
        }

        ctx.restore(); // end of head-space markings
        ctx.restore(); // end of body clip

        if let Some(outline_color) = &s.outline_color {
            ctx.save();
            ctx.set_shadow_blur(0.0);
            ctx.set_shadow_offset_y(0.0);
            trace_body_path(ctx, &outline);
            ctx.set_stroke_color(outline_color);
            ctx.set_line_width(1.8);
            ctx.set_line_join(LineJoin::Round);
            ctx.stroke();
            ctx.restore();
        }

        // 4) Ears, face features and eyes, in head space.
        ctx.save();
        ctx.translate(hcx, hcy);
        ctx.rotate(self.head_tilt);

        let left_twitch = (self.ear_twitch_left * PI).sin() * 0.12;
        let right_twitch = (self.ear_twitch_right * PI).sin() * 0.12;
        self.draw_custom_ear(ctx, s, -1.0, left_twitch);
        self.draw_custom_ear(ctx, s, 1.0, right_twitch);

        ctx.set_shadow_blur(0.0);
        ctx.set_shadow_offset_y(0.0);
        let w_shift = self.face_turn_x * 0.72;

        // Blush while being petted.
        if let Some(blush_color) = &s.blush_color {
            if self.squint_amount > 0.12 {
                ctx.save();
                ctx.set_global_alpha(0.42 * self.squint_amount);
                ctx.set_fill_color(blush_color);
                ctx.begin_path();
                ctx.ellipse(w_shift - 30.0, 13.0, 8.5, 4.6, -0.08, 0.0, PI * 2.0, false);
                ctx.ellipse(w_shift + 30.0, 13.0, 8.5, 4.6, 0.08, 0.0, PI * 2.0, false);
                ctx.fill();
                ctx.restore();
            }
        }

        if let Some(nose_color) = &s.nose_color {
            ctx.set_fill_color(nose_color);
            ctx.begin_path();
            ctx.move_to(w_shift - 4.1, 9.6);
            ctx.quadratic_curve_to(w_shift, 8.5, w_shift + 4.1, 9.6);
            ctx.quadratic_curve_to(w_shift + 3.2, 13.8, w_shift, 14.3);
            ctx.quadratic_curve_to(w_shift - 3.2, 13.8, w_shift - 4.1, 9.6);
            ctx.close_path();
            ctx.fill();
        }
        if let Some(mouth_color) = &s.mouth_color {
            ctx.set_stroke_color(mouth_color);
            ctx.set_line_width(1.35);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.move_to(w_shift - 5.8, 16.6);
            ctx.quadratic_curve_to(w_shift - 2.9, 19.6, w_shift, 16.4);
            ctx.quadratic_curve_to(w_shift + 2.9, 19.6, w_shift + 5.8, 16.6);
            ctx.stroke();
        }

        // Whiskers, longer on the side the face turns away from.
        ctx.set_stroke_color(&s.whisker_color);
        ctx.set_line_width(1.1);
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

        self.draw_custom_eyes(ctx, s, w_shift, -1.0);

        ctx.restore();
        ctx.restore();
    }

    /// Draws the face markings of the coat pattern (bicolor mask, tuxedo chin, siamese mask,
    /// calico patches or tabby "M" and cheek stripes). Expects head-space coordinates.
    ///
    /// @param ctx The canvas, translated and rotated to the head centre.
    /// @param s The resolved skin.
    /// @param mask_shift Horizontal shift of the face markings when the head turns.
    fn draw_custom_face_pattern(&self, ctx: &mut Canvas, s: &ResolvedSkin, mask_shift: f32) {
        match s.pattern {
            CatPattern::Bicolor => {
                let Some(mask_color) = &s.mask_color else {
                    return;
                };
                ctx.save();
                ctx.set_fill_color(mask_color);
                ctx.set_shadow_color(mask_color);
                ctx.set_shadow_blur(4.5);
                ctx.begin_path();
                ctx.move_to(-74.0, -44.0);
                ctx.line_to(74.0, -44.0);
                ctx.line_to(70.0, 20.0);
                ctx.quadratic_curve_to(mask_shift + 42.0, 18.0, mask_shift + 26.0, 9.0);
                ctx.quadratic_curve_to(mask_shift + 17.0, 0.0, mask_shift + 13.5, -13.0);
                ctx.quadratic_curve_to(mask_shift + 7.0, -21.5, mask_shift, -22.5);
                ctx.quadratic_curve_to(mask_shift - 7.0, -21.5, mask_shift - 13.5, -13.0);
                ctx.quadratic_curve_to(mask_shift - 17.0, 0.0, mask_shift - 26.0, 9.0);
                ctx.quadratic_curve_to(mask_shift - 42.0, 18.0, -70.0, 20.0);
                ctx.close_path();
                ctx.fill();
                if let Some(bridge_color) = &s.forehead_bridge_color {
                    ctx.set_fill_color(bridge_color);
                    ctx.set_shadow_blur(5.5);
                    ctx.begin_path();
                    ctx.ellipse(mask_shift, -32.0, 18.0, 9.0, 0.0, 0.0, PI * 2.0, false);
                    ctx.fill();
                }
                ctx.restore();
            }
            CatPattern::Tuxedo => {
                let Some(bib_color) = &s.belly_bib_color else {
                    return;
                };
                ctx.set_fill_color(bib_color);
                ctx.begin_path();
                ctx.ellipse(mask_shift, 22.0, 26.0, 15.0, 0.0, 0.0, PI * 2.0, false);
                ctx.fill();
            }
            CatPattern::Siamese => {
                let Some(mask_color) = &s.mask_color else {
                    return;
                };
                ctx.save();
                ctx.set_fill_color(mask_color);
                ctx.set_shadow_color(mask_color);
                ctx.set_shadow_blur(9.0);
                ctx.begin_path();
                ctx.ellipse(mask_shift, 3.0, 42.0, 27.0, 0.0, 0.0, PI * 2.0, false);
                ctx.fill();
                ctx.restore();
            }
            CatPattern::Calico => {
                let orange_patch = s.mask_color.clone().unwrap_or_else(|| "#ea9446".to_string());
                let dark_patch = s.calico_patch_color.clone().unwrap_or_else(|| "#3c3634".to_string());
                ctx.save();
                ctx.set_shadow_blur(4.0);
                ctx.set_fill_color(&orange_patch);
                ctx.set_shadow_color(&orange_patch);
                ctx.begin_path();
                ctx.ellipse(-34.0 + mask_shift * 0.4, -18.0, 34.0, 28.0, -0.25, 0.0, PI * 2.0, false);
                ctx.fill();
                ctx.set_fill_color(&dark_patch);
                ctx.set_shadow_color(&dark_patch);
                ctx.begin_path();
                ctx.ellipse(36.0 + mask_shift * 0.4, -16.0, 32.0, 26.0, 0.25, 0.0, PI * 2.0, false);
                ctx.fill();
                ctx.restore();
            }
            CatPattern::Tabby => {
                let Some(stripe_color) = &s.stripe_color else {
                    return;
                };
                ctx.set_stroke_color(stripe_color);
                ctx.set_line_width(3.2);
                ctx.set_line_cap(LineCap::Round);
                ctx.set_line_join(LineJoin::Round);
                ctx.set_global_alpha(0.78);
                // The "M" on the forehead.
                ctx.begin_path();
                ctx.move_to(mask_shift - 11.0, -30.0);
                ctx.line_to(mask_shift - 8.0, -19.0);
                ctx.move_to(mask_shift, -32.0);
                ctx.line_to(mask_shift, -18.0);
                ctx.move_to(mask_shift + 11.0, -30.0);
                ctx.line_to(mask_shift + 8.0, -19.0);
                ctx.stroke();
                // Cheek stripes.
                ctx.set_line_width(2.8);
                for sign in [-1.0_f32, 1.0] {
                    ctx.begin_path();
                    ctx.move_to(sign * 58.0, 6.0);
                    ctx.line_to(sign * 42.0, 9.0);
                    ctx.move_to(sign * 56.0, 14.0);
                    ctx.line_to(sign * 41.0, 16.0);
                    ctx.stroke();
                }
                ctx.set_global_alpha(1.0);
            }
            CatPattern::Solid => {}
        }
    }

    /// Draws one ear of a custom cat. Expects head-space coordinates.
    ///
    /// @param ctx The canvas, translated and rotated to the head centre.
    /// @param s The resolved skin.
    /// @param side_sign -1 for the left ear, 1 for the right ear.
    /// @param twitch Extra rotation from an ear twitch, in radians.
    fn draw_custom_ear(&self, ctx: &mut Canvas, s: &ResolvedSkin, side_sign: f32, twitch: f32) {
        ctx.save();
        ctx.translate(side_sign * 33.0 + self.face_turn_x * 0.18, -28.0);
        ctx.rotate(side_sign * (0.08 + twitch));

        let base_ear_color = if s.pattern == CatPattern::Calico && side_sign < 0.0 {
            s.mask_color.clone().unwrap_or_else(|| s.ear_outer_color.clone())
        } else {
            s.ear_outer_color.clone()
        };
        let mut ear_gradient = Gradient::linear(0.0, -23.0, 0.0, 18.0);
        ear_gradient.add_color_stop(0.0, s.ear_tip_color.as_deref().unwrap_or(&base_ear_color));
        ear_gradient.add_color_stop(1.0, &base_ear_color);
        ctx.set_fill_gradient(ear_gradient);
        ctx.begin_path();
        ctx.move_to(side_sign * 19.0, 20.0);
        ctx.bezier_curve_to(side_sign * 19.0, -8.0, side_sign * 12.0, -23.0, side_sign * 3.0, -21.0);
        ctx.bezier_curve_to(side_sign * -8.0, -18.0, side_sign * -16.0, -6.0, side_sign * -21.0, 8.0);
        ctx.close_path();
        ctx.fill();

        ctx.set_shadow_blur(0.0);
        ctx.set_shadow_offset_y(0.0);

        if let Some(outline_color) = &s.outline_color {
            ctx.set_stroke_color(outline_color);
            ctx.set_line_width(1.85);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.move_to(side_sign * 19.0, 17.0);
            ctx.bezier_curve_to(side_sign * 19.0, -8.0, side_sign * 12.0, -23.0, side_sign * 3.0, -21.0);
            ctx.bezier_curve_to(side_sign * -8.0, -18.0, side_sign * -16.0, -6.0, side_sign * -19.0, 5.0);
            ctx.stroke();
        }

        if let Some(inner_color) = &s.ear_inner_color {
            ctx.set_fill_color(inner_color);
            ctx.begin_path();
            ctx.move_to(side_sign * 11.0, 10.0);
            ctx.bezier_curve_to(side_sign * 11.0, -3.0, side_sign * 7.0, -13.0, side_sign * 2.0, -11.0);
            ctx.bezier_curve_to(side_sign * -4.0, -9.0, side_sign * -9.0, -1.0, side_sign * -10.0, 8.0);
            ctx.close_path();
            ctx.fill();
        }

        if let Some(tuft_color) = &s.ear_tuft_color {
            ctx.set_stroke_color(tuft_color);
            ctx.set_line_width(1.5);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.move_to(side_sign * -6.0, 6.0);
            ctx.quadratic_curve_to(side_sign * -1.0, 3.0, side_sign * 5.0, 6.0);
            ctx.move_to(side_sign * -3.0, 10.0);
            ctx.quadratic_curve_to(side_sign * 2.0, 7.5, side_sign * 7.0, 10.0);
            ctx.stroke();
        }
        ctx.restore();
    }

    /// Draws both eyes of a custom cat: open with iris and pupil, squinting while petted,
    /// or closed in privacy mode. Expects head-space coordinates.
    ///
    /// @param ctx The canvas, translated and rotated to the head centre.
    /// @param s The resolved skin.
    /// @param cx Horizontal centre between the eyes.
    /// @param eye_y Vertical eye position.
    pub(crate) fn draw_custom_eyes(&self, ctx: &mut Canvas, s: &ResolvedSkin, cx: f32, eye_y: f32) {
        let blink_value = if self.is_blinking { 1.0 - (self.blink_progress - 1.0).abs() } else { 0.0 };
        let closedness = blink_value.max(self.squint_amount * 0.85).max(self.privacy_close).max(self.sleep_amount());
        let sleep_line = self.sleep_eye_line_amount();
        let eye_xs = [cx - 23.0, cx + 23.0];
        let radius_x = 14.8 + self.typing_hunch * 1.0;
        let radius_y = 13.4 - self.typing_hunch * 0.5;

        for (index, eye_x) in eye_xs.iter().enumerate() {
            ctx.save();
            ctx.translate(*eye_x, eye_y);

            if self.privacy_close > 0.45 || sleep_line >= 1.0 {
                ctx.set_stroke_color(&s.squint_color);
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
                if let Some(glow_color) = &s.squint_glow_color {
                    ctx.set_stroke_color(glow_color);
                    ctx.set_line_width(4.4);
                    ctx.set_line_cap(LineCap::Round);
                    ctx.begin_path();
                    ctx.move_to(-12.5, 0.2);
                    ctx.quadratic_curve_to(0.0, 2.2, 12.5, 1.8);
                    ctx.stroke();
                }
                ctx.set_stroke_color(&s.squint_color);
                ctx.set_line_width(2.8);
                ctx.set_line_cap(LineCap::Round);
                ctx.begin_path();
                ctx.move_to(-13.5, -0.5);
                ctx.quadratic_curve_to(0.0, 1.4, 13.5, 1.2);
                ctx.stroke();
                ctx.restore();
                continue;
            }

            let open_radius_y = (radius_y * (1.0 - closedness)).max(1.2);
            let tilt_direction = if index == 0 { -0.1 } else { 0.1 };
            let curious_tilt = tilt_direction * (0.25 + self.typing_hunch * 0.75);
            ctx.rotate(curious_tilt * (1.0 - sleep_line));

            let mut iris_gradient = Gradient::linear(0.0, -open_radius_y, 0.0, open_radius_y);
            iris_gradient.add_color_stop(0.0, &s.eye_color_top);
            iris_gradient.add_color_stop(1.0, &s.eye_color_bottom);
            ctx.set_fill_gradient(iris_gradient);
            ctx.begin_path();
            ctx.ellipse(0.0, 0.0, radius_x, open_radius_y, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();

            ctx.save();
            ctx.clip();
            if open_radius_y > 3.2 {
                let pupil_x = self.eye_look_x;
                let pupil_y = self.eye_look_y;
                ctx.set_fill_color(&s.pupil_color);
                ctx.begin_path();
                ctx.ellipse(pupil_x, pupil_y, 6.2, 8.8, 0.0, 0.0, PI * 2.0, false);
                ctx.fill();

                if s.eye_catchlight {
                    ctx.set_fill_color("rgba(255, 255, 255, 0.94)");
                    ctx.begin_path();
                    ctx.arc(pupil_x - 3.6, pupil_y - 4.0, 2.5, 0.0, PI * 2.0, false);
                    ctx.fill();
                    ctx.set_fill_color("rgba(255, 255, 255, 0.72)");
                    ctx.begin_path();
                    ctx.arc(pupil_x + 3.2, pupil_y + 3.2, 1.2, 0.0, PI * 2.0, false);
                    ctx.fill();
                }
            }
            ctx.restore();

            if let Some(rim_color) = &s.eye_rim_color {
                if open_radius_y > 2.2 {
                    ctx.set_stroke_color(rim_color);
                    ctx.set_line_width(1.25);
                    ctx.begin_path();
                    ctx.ellipse(0.0, 0.0, radius_x, open_radius_y, 0.0, 0.0, PI * 2.0, false);
                    ctx.stroke();
                }
            }

            if sleep_line > 0.0 {
                super::sleep::draw_sleeping_eye_line(ctx, &s.squint_color, sleep_line);
            }

            ctx.restore();
        }
    }

    /// Draws a reaching arm (poking a key or holding the mouse) of a custom cat: a tapered
    /// sleeve from the shoulder to the wrist, spots, and a round mitten paw.
    ///
    /// @param ctx The canvas, in pet coordinates.
    /// @param s The resolved skin.
    /// @param side Which arm.
    /// @param sx Shoulder x.
    /// @param sy Shoulder y.
    /// @param px Paw x.
    /// @param py Paw y.
    /// @param squash How much the paw is squashed by an impact (0..1).
    /// @param with_crease Whether to draw the inner arm crease lines.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_custom_plump_arm(
        &self,
        ctx: &mut Canvas,
        s: &ResolvedSkin,
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

        ctx.save();
        ctx.set_fill_color(&s.arm_color);
        trace_arm_path(ctx, sx, sy, wx, wy, nx, ny, shoulder_radius, wrist_radius);
        ctx.fill();

        ctx.begin_path();
        ctx.arc(sx, sy, shoulder_radius, 0.0, PI * 2.0, false);
        ctx.fill();

        // Custom arm spots, clipped to the sleeve.
        let arm_part = arm_part_name(side);
        let arm_spots: Vec<&CatSpot> = s.spots.iter().filter(|spot| spot.part == arm_part).collect();
        if !arm_spots.is_empty() && length > 24.0 {
            let arm_angle = dy.atan2(dx);
            ctx.save();
            trace_arm_path(ctx, sx, sy, wx, wy, nx, ny, shoulder_radius, wrist_radius);
            ctx.clip();
            for spot in arm_spots {
                let fraction = (spot.x as f32).clamp(0.1, 0.9);
                ctx.set_global_alpha(spot.opacity as f32);
                ctx.set_fill_color(&spot.color);
                ctx.begin_path();
                ctx.ellipse(
                    sx + ux * (length * fraction) + nx * spot.y as f32,
                    sy + uy * (length * fraction) + ny * spot.y as f32,
                    (spot.rx as f32).max(2.0),
                    (spot.ry as f32).max(2.0),
                    arm_angle + spot.rotation as f32,
                    0.0,
                    PI * 2.0,
                    false,
                );
                ctx.fill();
            }
            ctx.restore();
        }

        // Round mitten paw.
        ctx.save();
        if !s.is_dark_base {
            ctx.set_shadow_color("rgba(36, 32, 30, 0.22)");
            ctx.set_shadow_blur(4.0);
            ctx.set_shadow_offset_y(1.5);
        }
        ctx.set_fill_color(&s.paw_color);
        ctx.begin_path();
        ctx.ellipse(px, py, paw_radius_x, paw_radius_y, 0.0, 0.0, PI * 2.0, false);
        ctx.fill();
        ctx.restore();

        let edge_color = s.paw_outline_color.as_ref().or(s.outline_color.as_ref());
        if let Some(edge_color) = edge_color {
            ctx.set_line_width(1.65);
            ctx.set_line_cap(LineCap::Round);
            for sign in [-1.0_f32, 1.0] {
                let start_fraction = if with_crease { 0.16 } else { 0.08 };
                let edge_reach = shoulder_radius * (1.0 - start_fraction * 0.16);
                let edge_start_x = sx + ux * (length * start_fraction) + nx * sign * edge_reach;
                let edge_start_y = sy + uy * (length * start_fraction) + ny * sign * edge_reach;
                ctx.set_stroke_color(edge_color);
                ctx.set_global_alpha(0.85);
                ctx.begin_path();
                ctx.move_to(edge_start_x, edge_start_y);
                ctx.quadratic_curve_to(
                    (sx + wx) * 0.5 + nx * sign * (shoulder_radius * 0.84),
                    (sy + wy) * 0.5 + ny * sign * (shoulder_radius * 0.84),
                    wx + nx * sign * wrist_radius,
                    wy + ny * sign * wrist_radius,
                );
                ctx.stroke();
            }
            let paw_angle = dy.atan2(dx);
            ctx.set_global_alpha(0.92);
            ctx.set_stroke_color(edge_color);
            ctx.begin_path();
            ctx.ellipse(
                px,
                py,
                paw_radius_x,
                paw_radius_y,
                0.0,
                paw_angle - PI * 0.62,
                paw_angle + PI * 0.62,
                false,
            );
            ctx.stroke();
        } else if let Some(crease_color) = &s.arm_crease_color {
            if with_crease && length > 28.0 {
                for sign in [-1.0_f32, 1.0] {
                    let crease_start_x = sx + ux * (length * 0.18) + nx * sign * (shoulder_radius * 0.86);
                    let crease_start_y = sy + uy * (length * 0.18) + ny * sign * (shoulder_radius * 0.86);
                    let crease_end_x = sx + ux * (length * 0.72) + nx * sign * (wrist_radius * 1.02);
                    let crease_end_y = sy + uy * (length * 0.72) + ny * sign * (wrist_radius * 1.02);
                    ctx.set_stroke_color(crease_color);
                    ctx.set_line_width(1.3);
                    ctx.set_line_cap(LineCap::Round);
                    ctx.begin_path();
                    ctx.move_to(crease_start_x, crease_start_y);
                    ctx.line_to(crease_end_x, crease_end_y);
                    ctx.stroke();
                }
            }
        }

        ctx.restore();
    }

    /// Draws the elbow-bent arm with the paw curled against the belly, for a custom cat.
    ///
    /// @param ctx The canvas, in pet coordinates.
    /// @param s The resolved skin.
    /// @param paw The paw state.
    /// @param breath Breathing offset.
    /// @param alpha Opacity, used to blend between poses.
    pub(crate) fn draw_custom_bent_arm(&self, ctx: &mut Canvas, s: &ResolvedSkin, paw: &PawState, breath: f32, alpha: f32) {
        let shoulder = self.get_live_shoulder(paw, breath);
        let curled = self.get_live_curled_paw_pos(paw, breath);
        let sign = if paw.side == PawSide::Left { -1.0 } else { 1.0 };
        let elbow_x = shoulder.x + sign * 8.0;
        let elbow_y = curled.y + 8.0;

        ctx.save();
        ctx.set_global_alpha(alpha);
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_join(LineJoin::Round);

        ctx.set_stroke_color(&s.arm_color);
        ctx.set_line_width(20.0);
        ctx.begin_path();
        ctx.move_to(shoulder.x, shoulder.y);
        ctx.quadratic_curve_to(elbow_x, elbow_y, curled.x, curled.y);
        ctx.stroke();

        let arm_part = arm_part_name(paw.side);
        for spot in &s.spots {
            if spot.part != arm_part {
                continue;
            }
            fill_spot(
                ctx,
                spot,
                (elbow_x + curled.x) * 0.5,
                (elbow_y + curled.y) * 0.5,
                (spot.rx as f32).min(8.0),
                (spot.ry as f32).min(6.0),
                -0.3 * sign,
                alpha * spot.opacity as f32,
            );
        }

        ctx.set_fill_color(&s.paw_color);
        ctx.begin_path();
        ctx.ellipse(curled.x, curled.y, 12.5, 11.0, sign * 0.2, 0.0, PI * 2.0, false);
        ctx.fill();

        ctx.set_stroke_color(s.paw_outline_color.as_deref().unwrap_or(&s.leg_line_color));
        ctx.set_line_width(1.55);
        ctx.begin_path();
        ctx.move_to(elbow_x + sign * 8.0, elbow_y - 16.0);
        ctx.quadratic_curve_to(elbow_x + sign * 4.0, elbow_y + 12.0, curled.x + sign * 2.0, curled.y + 10.0);
        ctx.quadratic_curve_to(curled.x - sign * 11.0, curled.y + 8.0, curled.x - sign * 12.0, curled.y - 4.0);
        ctx.stroke();
        ctx.restore();
    }

    /// Draws the custom-skin details on a seated (idle) front leg: arm spots, light mittens
    /// when the paw colour differs from the fur, and the leg contour lines.
    ///
    /// @param ctx The canvas, in pet coordinates.
    /// @param s The resolved skin.
    /// @param paw The paw state.
    /// @param top_sp Spine point at the top of the leg.
    /// @param mid_sp Spine point at the middle of the leg.
    /// @param bot_sp Spine point at the bottom of the leg.
    /// @param sign -1 for the left leg, 1 for the right leg.
    /// @param outer_top_x Outer contour x at the top.
    /// @param outer_bot_x Outer contour x at the bottom.
    /// @param inner_top_x Inner contour x at the top.
    /// @param inner_bot_x Inner contour x at the bottom.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn draw_custom_idle_leg_details(
        &self,
        ctx: &mut Canvas,
        s: &ResolvedSkin,
        paw: &PawState,
        top_sp: Vec2,
        mid_sp: Vec2,
        bot_sp: Vec2,
        sign: f32,
        outer_top_x: f32,
        outer_bot_x: f32,
        inner_top_x: f32,
        inner_bot_x: f32,
    ) {
        let arm_part = arm_part_name(paw.side);
        for spot in &s.spots {
            if spot.part != arm_part {
                continue;
            }
            fill_spot(
                ctx,
                spot,
                mid_sp.x + sign * 21.0,
                mid_sp.y + (spot.x as f32 - 0.5) * 28.0,
                (spot.rx as f32 * 0.8).max(3.0),
                (spot.ry as f32 * 1.1).max(3.0),
                0.2 * sign,
                spot.opacity as f32,
            );
        }

        // White mittens at the bottom of the seated legs when the paws differ from the fur.
        if s.paw_color != s.fur_color {
            ctx.save();
            ctx.set_fill_color(&s.paw_color);
            ctx.begin_path();
            ctx.ellipse(bot_sp.x + sign * 22.0, bot_sp.y - 4.0, 13.5, 9.5, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();
            ctx.restore();
        }

        ctx.set_stroke_color(&s.leg_line_color);
        ctx.set_line_width(1.5);
        ctx.set_line_cap(LineCap::Round);
        ctx.begin_path();
        ctx.move_to(outer_top_x, top_sp.y);
        ctx.quadratic_curve_to(mid_sp.x + sign * 41.0, mid_sp.y, outer_bot_x, bot_sp.y - 1.0);
        ctx.stroke();

        ctx.begin_path();
        ctx.move_to(inner_top_x, top_sp.y + 10.0);
        ctx.line_to(inner_bot_x, bot_sp.y - 1.0);
        ctx.stroke();
    }
}

/// Traces the tapered sleeve of a reaching arm as the current path.
///
/// @param ctx The canvas.
/// @param sx Shoulder x.
/// @param sy Shoulder y.
/// @param wx Wrist x.
/// @param wy Wrist y.
/// @param nx Unit normal x (perpendicular to the arm).
/// @param ny Unit normal y.
/// @param shoulder_radius Half-width at the shoulder.
/// @param wrist_radius Half-width at the wrist.
#[allow(clippy::too_many_arguments)]
fn trace_arm_path(
    ctx: &mut Canvas,
    sx: f32,
    sy: f32,
    wx: f32,
    wy: f32,
    nx: f32,
    ny: f32,
    shoulder_radius: f32,
    wrist_radius: f32,
) {
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
