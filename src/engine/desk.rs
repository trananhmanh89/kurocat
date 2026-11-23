//! Drawing of the desk props: the mini mouse, the keyboard, key bubbles and click badges.

use super::{now_ms, CatBreed, Engine, KeyboardOrientation};
use crate::keyboard_layout::{layout, KeyAccent, KeySpec, KeyboardLayoutId};
use crate::render::{Canvas, FontSpec, Gradient, LineCap, LineJoin, TextAlign, TextBaseline};
use std::f32::consts::PI;

/// Colour of the key bed between the key caps.
const KEY_BED_COLOR: &str = "#d4d6d1";

/// What the cached keyboard layer was drawn for. The position is not part of it: the layer is
/// placed with sub-pixel precision, so the bouncing keyboard reuses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyboardCacheKey {
    scale_and_skew: [u32; 4],
    layout: KeyboardLayoutId,
    orientation: KeyboardOrientation,
}

/// The keyboard at rest (no key pressed) drawn into its own layer.
pub(crate) struct KeyboardCache {
    key: KeyboardCacheKey,
    layer: Canvas,
    /// Where the keyboard centre lies inside the layer, in device pixels.
    origin_x: f32,
    origin_y: f32,
}

/// Sizes shared by everything drawn on the keyboard.
struct KeyboardGeometry {
    is_cat_facing: bool,
    is_108: bool,
    inner_w: f32,
    inner_h: f32,
    top_half_w: f32,
    bottom_half_w: f32,
    half_h: f32,
    row_h: f32,
    unit_w: f32,
    gap: f32,
    total_rows: f32,
    total_units: f32,
}

/// Position and size of one key cap, in keyboard coordinates.
struct KeyRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

/// Starts a new path holding a rounded rectangle (the old `roundRect` helper).
///
/// @param ctx The canvas.
/// @param x Left.
/// @param y Top.
/// @param width Width.
/// @param height Height.
/// @param radius Corner radius (clamped to half the shorter side).
pub(crate) fn round_rect_path(ctx: &mut Canvas, x: f32, y: f32, width: f32, height: f32, radius: f32) {
    ctx.begin_path();
    ctx.round_rect(x, y, width, height, radius);
}

/// Starts a new path holding a rounded trapezoid centred on x = 0 (the keyboard deck shape).
///
/// @param ctx The canvas.
/// @param top_half_width Half width of the top (far) edge.
/// @param bottom_half_width Half width of the bottom (near) edge.
/// @param top_y Y of the top edge.
/// @param bottom_y Y of the bottom edge.
/// @param radius Corner radius.
pub(crate) fn trapezoid_path(
    ctx: &mut Canvas,
    top_half_width: f32,
    bottom_half_width: f32,
    top_y: f32,
    bottom_y: f32,
    radius: f32,
) {
    ctx.begin_path();
    ctx.move_to(-top_half_width + radius, top_y);
    ctx.line_to(top_half_width - radius, top_y);
    ctx.quadratic_curve_to(top_half_width, top_y, top_half_width + 1.0, top_y + radius);
    ctx.line_to(bottom_half_width, bottom_y - radius);
    ctx.quadratic_curve_to(bottom_half_width, bottom_y, bottom_half_width - radius, bottom_y);
    ctx.line_to(-bottom_half_width + radius, bottom_y);
    ctx.quadratic_curve_to(-bottom_half_width, bottom_y, -bottom_half_width, bottom_y - radius);
    ctx.line_to(-top_half_width - 1.0, top_y + radius);
    ctx.quadratic_curve_to(-top_half_width, top_y, -top_half_width + radius, top_y);
    ctx.close_path();
}

/// Draws a small padlock icon centred on a point, in the current fill colour.
///
/// @param ctx The canvas.
/// @param center_x Centre x of the icon.
/// @param center_y Centre y of the icon.
/// @param size Icon height in pixels.
fn draw_padlock(ctx: &mut Canvas, center_x: f32, center_y: f32, size: f32) {
    let body_width = size * 0.78;
    let body_height = size * 0.56;
    let body_top = center_y + size * 0.5 - body_height;
    let shackle_radius = body_width * 0.3;
    let shackle_center_y = body_top - size * 0.08;

    ctx.save();
    ctx.set_line_width(size * 0.14);
    ctx.set_line_cap(LineCap::Round);
    ctx.begin_path();
    ctx.move_to(center_x - shackle_radius, body_top + 0.5);
    ctx.line_to(center_x - shackle_radius, shackle_center_y);
    ctx.arc(center_x, shackle_center_y, shackle_radius, PI, PI * 2.0, false);
    ctx.line_to(center_x + shackle_radius, body_top + 0.5);
    ctx.stroke();

    round_rect_path(ctx, center_x - body_width / 2.0, body_top, body_width, body_height, size * 0.12);
    ctx.fill();
    ctx.restore();
}

impl Engine {
    /// Returns the top and side colours of a pressed key for the current character.
    ///
    /// @return (top colour, side colour).
    fn pressed_key_colors(&self) -> (String, String) {
        if let Some(skin) = &self.custom_skin {
            return (skin.accent_color.clone(), skin.accent_side_color.clone());
        }
        let (top, side) = match self.cat_breed {
            CatBreed::DaFeiYu => ("#5cc8ff", "#2563eb"),
            CatBreed::Ragdoll => ("#7ac7ff", "#4690d8"),
            CatBreed::Black => ("#d2f244", "#8cb014"),
        };
        (top.to_string(), side.to_string())
    }

    /// Draws the small mouse to the left of the keyboard, with lit buttons while clicking.
    ///
    /// @param ctx The canvas, in logical pet coordinates.
    pub(crate) fn draw_mini_mouse(&self, ctx: &mut Canvas) {
        let mouse_x = self.mouse_base_x + self.mouse_offset_x;
        let mouse_y = self.mouse_base_y + self.mouse_offset_y;
        let rx = 18.5;
        let ry = 23.0;
        let left_press = self.mouse_left_press;
        let right_press = self.mouse_right_press;
        let middle_press = self.mouse_middle_press.max(self.mouse_scroll_flash);
        let colors = self.get_mouse_button_colors();
        let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;

        ctx.save();
        ctx.translate(mouse_x, mouse_y);
        ctx.rotate(-0.06);

        // Soft base rim with a drop shadow.
        ctx.save();
        ctx.set_shadow_color("rgba(0, 0, 0, 0.28)");
        ctx.set_shadow_blur(7.0);
        ctx.set_shadow_offset_y(3.5);
        ctx.set_fill_color("#c8cac4");
        ctx.begin_path();
        ctx.ellipse(0.0, 1.8, rx, ry, 0.0, 0.0, PI * 2.0, false);
        ctx.fill();
        ctx.restore();

        // Palm shell.
        let mut palm_gradient = Gradient::linear(0.0, -ry, 0.0, 2.0);
        palm_gradient.add_color_stop(0.0, "#ecece7");
        palm_gradient.add_color_stop(1.0, "#f4f4f0");
        ctx.set_fill_gradient(palm_gradient);
        ctx.begin_path();
        ctx.ellipse(0.0, 0.0, rx, ry, 0.0, 0.0, PI * 2.0, false);
        ctx.fill();

        // Clip to the shell so the button plates stay inside it.
        ctx.save();
        ctx.begin_path();
        ctx.ellipse(0.0, 0.0, rx - 0.5, ry - 0.5, 0.0, 0.0, PI * 2.0, false);
        ctx.clip();

        // Cat-facing: palm at the top, buttons at the bottom, left button on +X.
        // Viewer-facing: everything flipped, so the buttons are at the top and left is on -X.
        ctx.save();
        if !is_cat_facing {
            ctx.scale(-1.0, -1.0);
        }

        // Left button plate.
        ctx.begin_path();
        ctx.move_to(0.0, 0.5);
        ctx.quadratic_curve_to(rx * 0.52, 0.4, rx + 2.0, -0.8);
        ctx.line_to(rx + 2.0, ry + 2.0);
        ctx.line_to(0.0, ry + 2.0);
        ctx.close_path();
        if left_press > 0.08 {
            ctx.set_fill_color(&colors.left);
        } else {
            ctx.set_fill_color("#fafaf7");
        }
        ctx.fill();
        if left_press > 0.08 {
            ctx.set_fill_color(&format!("rgba(18, 22, 28, {:.3})", 0.18 * left_press));
            ctx.fill_rect(0.0, -1.0, rx + 2.0, 3.8);
            ctx.fill_rect(0.0, -1.0, 2.2, ry + 2.0);
        }

        // Right button plate.
        ctx.begin_path();
        ctx.move_to(-rx - 2.0, -0.8);
        ctx.quadratic_curve_to(-rx * 0.52, 0.4, 0.0, 0.5);
        ctx.line_to(0.0, ry + 2.0);
        ctx.line_to(-rx - 2.0, ry + 2.0);
        ctx.close_path();
        if right_press > 0.08 {
            ctx.set_fill_color(&colors.right);
        } else {
            ctx.set_fill_color("#fafaf7");
        }
        ctx.fill();
        if right_press > 0.08 {
            ctx.set_fill_color(&format!("rgba(18, 22, 28, {:.3})", 0.18 * right_press));
            ctx.fill_rect(-rx - 2.0, -1.0, rx + 2.0, 3.8);
            ctx.fill_rect(-2.2, -1.0, 2.2, ry + 2.0);
        }

        // Seam between palm and buttons, and the split between the two buttons.
        ctx.set_stroke_color("rgba(42, 45, 52, 0.56)");
        ctx.set_line_width(1.25);
        ctx.begin_path();
        ctx.move_to(-rx - 2.0, -0.8);
        ctx.quadratic_curve_to(0.0, 1.8, rx + 2.0, -0.8);
        ctx.stroke();

        ctx.set_stroke_color("rgba(42, 45, 52, 0.66)");
        ctx.set_line_width(1.3);
        ctx.begin_path();
        ctx.move_to(0.0, 0.6);
        ctx.line_to(0.0, ry + 1.0);
        ctx.stroke();

        // Scroll wheel housing and wheel (lit on middle click or scroll).
        ctx.set_fill_color("#2d2f36");
        round_rect_path(ctx, -2.5, 4.2, 5.0, 9.4, 2.5);
        ctx.fill();

        if middle_press > 0.12 {
            ctx.set_fill_color(&colors.middle);
        } else {
            ctx.set_fill_color("#6b6f78");
        }
        let wheel_shift = self.mouse_middle_press * 0.8;
        round_rect_path(ctx, -1.7, 5.0 + wheel_shift, 3.4, 7.6, 1.7);
        ctx.fill();

        ctx.set_stroke_color("rgba(255, 255, 255, 0.45)");
        ctx.set_line_width(0.8);
        ctx.begin_path();
        ctx.move_to(-1.0, 8.8 + wheel_shift);
        ctx.line_to(1.0, 8.8 + wheel_shift);
        ctx.stroke();

        ctx.restore(); // end of the orientation flip

        // Upright "L" / "R" legends on the button tips.
        ctx.set_font(FontSpec::new(800, 7.6));
        ctx.set_text_align(TextAlign::Center);
        ctx.set_text_baseline(TextBaseline::Middle);
        let left_label_x = if is_cat_facing { 7.4 } else { -7.4 };
        let right_label_x = if is_cat_facing { -7.4 } else { 7.4 };
        let label_y = if is_cat_facing { 14.0 } else { -14.0 };
        let press_direction = if is_cat_facing { 0.9 } else { -0.9 };
        if left_press > 0.12 {
            ctx.set_fill_color("#14161a");
        } else {
            ctx.set_fill_color("rgba(72, 76, 86, 0.56)");
        }
        ctx.fill_text("L", left_label_x, label_y + left_press * press_direction);
        if right_press > 0.12 {
            ctx.set_fill_color("#ffffff");
        } else {
            ctx.set_fill_color("rgba(72, 76, 86, 0.56)");
        }
        ctx.fill_text("R", right_label_x, label_y + right_press * press_direction);

        ctx.restore(); // end of the shell clip

        // Outer outline.
        ctx.set_stroke_color("#2b2d34");
        ctx.set_line_width(1.5);
        ctx.begin_path();
        ctx.ellipse(0.0, 0.0, rx, ry, 0.0, 0.0, PI * 2.0, false);
        ctx.stroke();

        ctx.restore();
    }

    /// Draws the keyboard deck with all key caps, and the password-guard cover when active.
    ///
    /// The keyboard at rest is drawn once into a cached layer; each frame only the keys that
    /// are moving are drawn on top of it.
    ///
    /// @param ctx The canvas, in logical pet coordinates.
    pub(crate) fn draw_keyboard(&self, ctx: &mut Canvas) {
        ctx.save();
        ctx.translate(self.kb_center_x, self.kb_center_y + self.kb_bounce * 1.1);
        ctx.rotate(self.kb_tilt_rad);
        if self.draw_keyboard_at_rest_from_cache(ctx) {
            let geometry = self.keyboard_geometry();
            let (pressed_top, pressed_side) = self.pressed_key_colors();
            for key in layout(self.keyboard_layout).keys.iter() {
                let press = self.key_press_amount(key.id);
                if press > 0.0005 {
                    self.draw_moving_key(ctx, key, press, &geometry, &pressed_top, &pressed_side);
                }
            }
            if self.privacy_close > 0.08 {
                self.draw_privacy_cover(ctx, geometry.top_half_w, geometry.bottom_half_w, geometry.half_h);
            }
        } else {
            self.draw_keyboard_contents(ctx, false);
        }
        ctx.restore();
    }

    /// @param key_id The key id.
    /// @return How far the key is pressed down (0..1).
    fn key_press_amount(&self, key_id: &str) -> f32 {
        match self.key_anims.get(key_id) {
            Some(anim) => anim.press_amount,
            None => 0.0,
        }
    }

    /// @return The sizes of the current keyboard.
    fn keyboard_geometry(&self) -> KeyboardGeometry {
        let keyboard = layout(self.keyboard_layout);
        let is_108 = self.keyboard_layout == KeyboardLayoutId::Full108;
        let pad_x = 7.0;
        let pad_y = 6.0;
        let inner_w = self.kb_width - pad_x * 2.0;
        let inner_h = self.kb_height - pad_y * 2.0;
        KeyboardGeometry {
            is_cat_facing: self.keyboard_orientation == KeyboardOrientation::Cat,
            is_108,
            inner_w,
            inner_h,
            // Desk perspective: the far (top) edge is narrower than the near (bottom) edge.
            top_half_w: (self.kb_width * 0.955) / 2.0,
            bottom_half_w: (self.kb_width * 1.035) / 2.0,
            half_h: self.kb_height / 2.0,
            row_h: inner_h / keyboard.total_rows,
            unit_w: inner_w / keyboard.total_units,
            gap: if is_108 { 0.85 } else { 0.9 },
            total_rows: keyboard.total_rows,
            total_units: keyboard.total_units,
        }
    }

    /// Works out where a key cap is drawn.
    ///
    /// @param key The key.
    /// @param press How far it is pressed down (0..1).
    /// @param geometry The keyboard sizes.
    /// @return The key cap rectangle.
    fn key_rect(&self, key: &KeySpec, press: f32, geometry: &KeyboardGeometry) -> KeyRect {
        let row_y = key.row_y;
        let row_span = key.row_span;
        let screen_row_top = if geometry.is_cat_facing { geometry.total_rows - (row_y + row_span) } else { row_y };
        let screen_u_start = if geometry.is_cat_facing {
            geometry.total_units - (key.u_start + key.u_width)
        } else {
            key.u_start
        };

        let row_center_fraction = (screen_row_top + row_span * 0.5) / geometry.total_rows;
        let depth_scale = 0.94 + row_center_fraction * 0.09;

        let raw_x = -geometry.inner_w / 2.0 + screen_u_start * geometry.unit_w + geometry.gap * 0.5;
        let raw_w = key.u_width * geometry.unit_w - geometry.gap;
        KeyRect {
            x: raw_x * depth_scale,
            w: raw_w * depth_scale,
            y: -geometry.inner_h / 2.0 + screen_row_top * geometry.row_h + geometry.gap * 0.5 + press * 2.0,
            h: row_span * geometry.row_h - geometry.gap - 0.8,
        }
    }

    /// Draws the keyboard at rest from the cached layer, redrawing the layer when the
    /// keyboard's size, rotation, layout or orientation changed.
    ///
    /// @param ctx The canvas, already moved and rotated to the keyboard centre.
    /// @return False when the layer could not be used (the caller then draws directly).
    fn draw_keyboard_at_rest_from_cache(&self, ctx: &mut Canvas) -> bool {
        let transform = ctx.transform();
        let key = KeyboardCacheKey {
            scale_and_skew: [
                transform.sx.to_bits(),
                transform.kx.to_bits(),
                transform.ky.to_bits(),
                transform.sy.to_bits(),
            ],
            layout: self.keyboard_layout,
            orientation: self.keyboard_orientation,
        };

        let mut cache = self.keyboard_cache.borrow_mut();
        let is_current = match cache.as_ref() {
            Some(existing) => existing.key == key,
            None => false,
        };
        if !is_current {
            // Size of the layer: the deck plus room for its drop shadow, without translation.
            // Shadows ignore the transform, so the shadow needs about 30 device pixels at any
            // scale; convert that to keyboard units.
            let scale = (transform.sx * transform.sx + transform.ky * transform.ky).sqrt().max(0.01);
            let margin = (30.0 / scale).max(26.0);
            let half_w = (self.kb_width * 1.035) / 2.0 + margin;
            let top = -self.kb_height / 2.0 - margin;
            let bottom = self.kb_height / 2.0 + 5.5 + margin;
            let corners = [(-half_w, top), (half_w, top), (half_w, bottom), (-half_w, bottom)];
            let mut min_x = f32::MAX;
            let mut min_y = f32::MAX;
            let mut max_x = f32::MIN;
            let mut max_y = f32::MIN;
            for (local_x, local_y) in corners {
                let device_x = transform.sx * local_x + transform.kx * local_y;
                let device_y = transform.ky * local_x + transform.sy * local_y;
                min_x = min_x.min(device_x);
                min_y = min_y.min(device_y);
                max_x = max_x.max(device_x);
                max_y = max_y.max(device_y);
            }
            let width = (max_x - min_x).ceil() as i32 + 2;
            let height = (max_y - min_y).ceil() as i32 + 2;
            if width > 4096 || height > 4096 {
                return false;
            }
            let origin_x = -min_x.floor() + 1.0;
            let origin_y = -min_y.floor() + 1.0;
            let mut layer = Canvas::new(width as u32, height as u32);
            layer.set_transform(transform.sx, transform.ky, transform.kx, transform.sy, origin_x, origin_y);
            self.draw_keyboard_contents(&mut layer, true);
            *cache = Some(KeyboardCache { key, layer, origin_x, origin_y });
        }
        if let Some(existing) = cache.as_ref() {
            ctx.composite_layer_at(
                &existing.layer,
                transform.tx - existing.origin_x,
                transform.ty - existing.origin_y,
            );
        }
        true
    }

    /// Draws the deck and all key caps.
    ///
    /// @param ctx The canvas, already moved and rotated to the keyboard centre.
    /// @param at_rest True to draw every key unpressed and leave out the password cover
    ///   (used for the cached layer).
    fn draw_keyboard_contents(&self, ctx: &mut Canvas, at_rest: bool) {
        let geometry = self.keyboard_geometry();
        let top_half_w = geometry.top_half_w;
        let bottom_half_w = geometry.bottom_half_w;
        let half_h = geometry.half_h;

        ctx.save();
        ctx.set_shadow_color("rgba(0, 0, 0, 0.34)");
        ctx.set_shadow_blur(11.0);
        ctx.set_shadow_offset_y(5.0);
        ctx.set_fill_color("#191a1e");
        trapezoid_path(ctx, top_half_w, bottom_half_w, -half_h + 3.5, half_h + 5.5, 6.5);
        ctx.fill();
        ctx.restore();

        ctx.set_fill_color("#24262c");
        ctx.set_stroke_color("#363840");
        ctx.set_line_width(1.1);
        trapezoid_path(ctx, top_half_w, bottom_half_w, -half_h, half_h, 6.5);
        ctx.fill();
        ctx.stroke();

        // Light grey key bed.
        ctx.set_fill_color(KEY_BED_COLOR);
        trapezoid_path(ctx, top_half_w - 5.0, bottom_half_w - 5.0, -half_h + 4.0, half_h - 4.0, 3.5);
        ctx.fill();

        let (pressed_top, pressed_side) = self.pressed_key_colors();
        for key in layout(self.keyboard_layout).keys.iter() {
            let press = if at_rest { 0.0 } else { self.key_press_amount(key.id) };
            self.draw_key(ctx, key, press, &geometry, &pressed_top, &pressed_side);
        }

        if !at_rest && self.privacy_close > 0.08 {
            self.draw_privacy_cover(ctx, top_half_w, bottom_half_w, half_h);
        }
    }

    /// Draws a moving key over the cached keyboard: its slot is first painted with the key
    /// bed colour, then the key is drawn at its pressed position.
    ///
    /// @param ctx The canvas, already moved and rotated to the keyboard centre.
    /// @param key The key.
    /// @param press How far it is pressed down (0..1).
    /// @param geometry The keyboard sizes.
    /// @param pressed_top Cap colour of a pressed key.
    /// @param pressed_side Side colour of a pressed key.
    fn draw_moving_key(
        &self,
        ctx: &mut Canvas,
        key: &KeySpec,
        press: f32,
        geometry: &KeyboardGeometry,
        pressed_top: &str,
        pressed_side: &str,
    ) {
        let rest = self.key_rect(key, 0.0, geometry);
        let slot_x = rest.x - 0.4;
        let slot_y = rest.y - 0.4;
        let slot_w = rest.w + 0.8;
        let slot_h = rest.h + 1.4 + 0.65;
        ctx.save();
        ctx.begin_path();
        ctx.rect(slot_x, slot_y, slot_w, slot_h);
        ctx.clip();
        ctx.set_fill_color(KEY_BED_COLOR);
        ctx.fill_rect(slot_x, slot_y, slot_w, slot_h);
        ctx.restore();
        // Not clipped: a pressed key moves down and its side may overlap the gap below.
        self.draw_key(ctx, key, press, geometry, pressed_top, pressed_side);
    }

    /// Draws one key cap with its side and legend.
    ///
    /// @param ctx The canvas, already moved and rotated to the keyboard centre.
    /// @param key The key.
    /// @param press How far it is pressed down (0..1).
    /// @param geometry The keyboard sizes.
    /// @param pressed_top Cap colour of a pressed key.
    /// @param pressed_side Side colour of a pressed key.
    fn draw_key(
        &self,
        ctx: &mut Canvas,
        key: &KeySpec,
        press: f32,
        geometry: &KeyboardGeometry,
        pressed_top: &str,
        pressed_side: &str,
    ) {
        let rect = self.key_rect(key, press, geometry);
        let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);

        let (top_color, side_color): (&str, &str) = if press > 0.14 {
            (pressed_top, pressed_side)
        } else if key.accent == KeyAccent::Orange {
            ("#f4a524", "#c27a10")
        } else if key.accent == KeyAccent::Modifier {
            ("#efefeb", "#c7c9c3")
        } else {
            ("#f8f8f5", "#d0d2cc")
        };

        ctx.set_fill_color(side_color);
        round_rect_path(ctx, x, y + 1.4, w, h, 1.8);
        ctx.fill();

        ctx.set_fill_color(top_color);
        round_rect_path(ctx, x, y, w, h - 0.9, 1.8);
        ctx.fill();

        // Legends are upside down (rotated 180°) when the keyboard faces the cat.
        let legend = if key.id == "space" {
            ""
        } else if key.id == "esc" {
            "Esc"
        } else {
            key.label
        };
        if legend.is_empty() {
            return;
        }
        let legend_length = legend.chars().count();
        let font_size = if geometry.is_108 {
            if legend_length >= 3 {
                5.0
            } else if legend_length == 2 {
                5.8
            } else {
                7.2
            }
        } else if legend_length >= 3 {
            5.6
        } else if legend_length == 2 {
            6.4
        } else {
            7.8
        };
        ctx.save();
        ctx.translate(x + w * 0.5, y + (h - 0.9) * 0.50);
        if geometry.is_cat_facing {
            ctx.rotate(PI);
        }
        ctx.set_font(FontSpec::new(700, font_size));
        if press > 0.14 {
            ctx.set_fill_color("#111214");
        } else if key.accent == KeyAccent::Orange {
            ctx.set_fill_color("#ffffff");
        } else {
            ctx.set_fill_color("#2c2f36");
        }
        ctx.set_text_align(TextAlign::Center);
        ctx.set_text_baseline(TextBaseline::Middle);
        ctx.fill_text(legend, 0.0, 0.0);
        ctx.restore();
    }

    /// Draws the dark cover with a padlock that hides the keys while a password is typed.
    ///
    /// @param ctx The canvas, already moved to the keyboard centre.
    /// @param top_half_w Half width of the deck's top edge.
    /// @param bottom_half_w Half width of the deck's bottom edge.
    /// @param half_h Half height of the deck.
    fn draw_privacy_cover(&self, ctx: &mut Canvas, top_half_w: f32, bottom_half_w: f32, half_h: f32) {
        ctx.save();
        ctx.set_global_alpha(self.privacy_close);
        ctx.set_fill_color("rgba(15, 20, 32, 0.74)");
        trapezoid_path(ctx, top_half_w - 2.0, bottom_half_w - 2.0, -half_h + 1.5, half_h - 1.5, 5.5);
        ctx.fill();
        ctx.set_stroke_color("rgba(244, 114, 182, 0.45)");
        ctx.set_line_width(1.4);
        ctx.stroke();

        let typing_recently = now_ms() - self.privacy_typing_at < 350.0;
        let dots = if typing_recently { " • • •" } else { "" };
        let base_label = if self.cat_breed == CatBreed::DaFeiYu {
            "No peeking · keys hidden"
        } else {
            "Typing password · keys hidden"
        };
        let label = format!("{}{}", base_label, dots);

        ctx.set_font(FontSpec::new(700, 11.5));
        ctx.set_fill_color("#fbcfe8");
        ctx.set_stroke_color("#fbcfe8");
        ctx.set_text_align(TextAlign::Left);
        ctx.set_text_baseline(TextBaseline::Middle);

        // Centre the padlock and the label together. The width ignores the dots so the
        // text does not jump sideways while they blink.
        let icon_size = 11.0;
        let icon_gap = 5.0;
        let text_width = ctx.measure_text(base_label);
        let group_width = icon_size * 0.78 + icon_gap + text_width;
        let start_x = -group_width / 2.0;
        draw_padlock(ctx, start_x + icon_size * 0.39, 0.0, icon_size);
        ctx.fill_text(&label, start_x + icon_size * 0.78 + icon_gap, 0.0);
        ctx.restore();
    }

    /// Draws the keycap bubbles that pop up above the keyboard.
    ///
    /// @param ctx The canvas, in logical pet coordinates.
    pub(crate) fn draw_key_bubbles(&self, ctx: &mut Canvas) {
        let now = now_ms();
        for bubble in self.bubbles.iter() {
            let age = now - bubble.created_at;
            let t = (age / bubble.lifetime).min(1.0) as f32;

            let mut scale = 1.0;
            if t < 0.11 {
                let pop = t / 0.11;
                scale = 0.62 + (pop * PI * 0.65).sin() * 0.44;
            } else if t > 0.76 {
                scale = 1.0 - ((t - 0.76) / 0.24) * 0.1;
            }
            let alpha = if t < 0.72 { 1.0 } else { (1.0 - (t - 0.72) / 0.28).max(0.0) };

            ctx.save();
            ctx.set_global_alpha(alpha);
            ctx.translate(bubble.x, bubble.y);
            ctx.rotate(bubble.rot);
            ctx.scale(scale, scale);

            ctx.set_font(FontSpec::new(700, 12.0));
            let text_width = ctx.measure_text(&bubble.label);
            let box_w = if bubble.is_space { 40.0 } else { (text_width + 13.0).max(24.0) };
            let box_h = 24.0;
            let box_x = -box_w / 2.0;
            let box_y = -box_h / 2.0;

            ctx.set_shadow_color("rgba(0, 0, 0, 0.22)");
            ctx.set_shadow_blur(5.0);
            ctx.set_shadow_offset_y(2.0);
            ctx.set_fill_color("#c8cac6");
            round_rect_path(ctx, box_x, box_y + 2.2, box_w, box_h, 5.5);
            ctx.fill();

            ctx.set_shadow_blur(0.0);
            ctx.set_fill_color("#ffffff");
            ctx.set_stroke_color("rgba(38, 40, 46, 0.42)");
            ctx.set_line_width(1.05);
            round_rect_path(ctx, box_x, box_y, box_w, box_h - 1.3, 5.5);
            ctx.fill();
            ctx.stroke();

            if bubble.is_space {
                ctx.set_stroke_color("#222429");
                ctx.set_line_width(1.55);
                ctx.set_line_cap(LineCap::Round);
                ctx.set_line_join(LineJoin::Round);
                ctx.begin_path();
                ctx.move_to(-9.0, -1.0);
                ctx.line_to(-9.0, 2.0);
                ctx.line_to(9.0, 2.0);
                ctx.line_to(9.0, -1.0);
                ctx.stroke();
            } else {
                ctx.set_fill_color("#1e2025");
                ctx.set_text_align(TextAlign::Center);
                ctx.set_text_baseline(TextBaseline::Middle);
                ctx.fill_text(&bubble.label, 0.0, -0.5);
            }

            ctx.restore();
        }
    }

    /// Draws the "LMB" / "RMB" / "MMB" badges above the mouse.
    ///
    /// @param ctx The canvas, in logical pet coordinates.
    pub(crate) fn draw_mouse_badges(&self, ctx: &mut Canvas) {
        let now = now_ms();
        let count = self.mouse_badges.len();
        for (index, badge) in self.mouse_badges.iter().enumerate() {
            let t = ((now - badge.created_at) / badge.lifetime).min(1.0) as f32;
            let mut scale = 1.0;
            if t < 0.14 {
                scale = 0.65 + ((t / 0.14) * PI * 0.65).sin() * 0.42;
            } else if t > 0.72 {
                scale = 1.0 - ((t - 0.72) / 0.28) * 0.12;
            }
            let alpha = if t < 0.68 { 1.0 } else { (1.0 - (t - 0.68) / 0.32).max(0.0) };
            let lift_y = t * 14.0 + (count - 1 - index) as f32 * 18.0;

            ctx.save();
            ctx.set_global_alpha(alpha);
            ctx.translate(badge.x, badge.y - lift_y);
            ctx.scale(scale, scale);

            let box_w = 34.0;
            let box_h = 19.0;
            let box_x = -box_w / 2.0;
            let box_y = -box_h / 2.0;

            ctx.set_shadow_color("rgba(0, 0, 0, 0.24)");
            ctx.set_shadow_blur(4.0);
            ctx.set_shadow_offset_y(1.5);
            ctx.set_fill_color("#22242a");
            round_rect_path(ctx, box_x, box_y + 1.6, box_w, box_h, 5.0);
            ctx.fill();

            ctx.set_shadow_blur(0.0);
            ctx.set_fill_color(&badge.color);
            ctx.set_stroke_color("rgba(20, 22, 28, 0.45)");
            ctx.set_line_width(1.0);
            round_rect_path(ctx, box_x, box_y, box_w, box_h - 1.0, 5.0);
            ctx.fill();
            ctx.stroke();

            ctx.set_font(FontSpec::new(800, 9.5));
            if badge.label == "RMB" && badge.color == "#ff7b3b" {
                ctx.set_fill_color("#ffffff");
            } else {
                ctx.set_fill_color("#14161a");
            }
            ctx.set_text_align(TextAlign::Center);
            ctx.set_text_baseline(TextBaseline::Middle);
            ctx.fill_text(&badge.label, 0.0, -0.3);

            ctx.restore();
        }
    }
}
