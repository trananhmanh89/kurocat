//! Input handling, the per-frame animation update and the render order of the pet.

use super::*;
use crate::cats::CatSkinConfig;
use crate::keyboard_layout::{format_bubble_for_vk, keys_108, keys_65, layout, resolve_key_by_vk, KeySpec};
use crate::render::Canvas;
use std::f32::consts::PI;

/// Moves `current` towards `target` with framerate-independent exponential smoothing.
///
/// @param current The current value.
/// @param target The value to move towards.
/// @param rate How fast to follow (higher is faster).
/// @param dt Frame time in seconds.
/// @return The new value.
fn approach(current: f32, target: f32, rate: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-rate * dt).exp())
}

/// Creates the resting state of a front paw.
///
/// @param side Which paw.
/// @param cat_center_x Horizontal centre of the cat.
/// @param cat_base_y Seated base of the cat.
/// @return The paw state.
fn new_paw(side: PawSide, cat_center_x: f32, cat_base_y: f32) -> PawState {
    let (shoulder_half_x, offset_x) = match side {
        PawSide::Left => (-32.0, -18.0),
        PawSide::Right => (32.0, 18.0),
    };
    let start = Vec2::new(cat_center_x + offset_x, cat_base_y);
    PawState {
        side,
        shoulder_norm_y: 0.50,
        shoulder_half_x,
        pos: start,
        vel: Vec2::default(),
        target: start,
        mode: PawMode::Idle,
        idle_weight: 1.0,
        curl_weight: 0.0,
        poke_time: 1.0,
        impact_squash: 0.0,
        last_active_at: -10000.0,
        smears: Vec::new(),
    }
}

/// Colours used for the mouse buttons and their click badges.
pub(crate) struct MouseButtonColors {
    pub left: String,
    pub right: String,
    pub middle: String,
}

/// Mouse button that was clicked.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Checks whether a hex colour is "warm" (orange/yellow), in which case the right mouse button
/// uses blue instead of orange so the two buttons stay easy to tell apart.
///
/// @param color CSS hex colour.
/// @return True for warm colours.
fn is_warm_accent(color: &str) -> bool {
    let lower = color.to_ascii_lowercase();
    let characters: Vec<char> = lower.chars().collect();
    if characters.len() < 4 || characters[0] != '#' {
        return false;
    }
    let first = characters[1];
    let second = characters[2];
    let third = characters[3];
    let is_hex = second.is_ascii_hexdigit();
    if first == 'f' && is_hex && "89abc".contains(third) {
        return true;
    }
    if first == 'f' && second == 'f' && "789abc".contains(third) {
        return true;
    }
    if first == 'e' && "89abcdef".contains(second) && "789a".contains(third) {
        return true;
    }
    false
}

impl Engine {
    /// Creates the pet in its resting state (black cat, 65% keyboard facing the cat).
    ///
    /// @return The engine.
    pub fn new() -> Engine {
        let mut key_anims = HashMap::new();
        for key in keys_65().iter().chain(keys_108().iter()) {
            key_anims.entry(key.id).or_insert(KeyAnim {
                pressed: false,
                press_amount: 0.0,
                last_hit_at: -10000.0,
            });
        }
        let cat_center_x = 196.0;
        let cat_base_y = 308.0;
        Engine {
            last_time: now_ms(),
            elapsed: 0.0,
            show_key_bubbles: true,
            privacy_mode: false,
            cat_breed: CatBreed::Black,
            custom_skin: None,
            keyboard_layout: KeyboardLayoutId::Compact65,
            keyboard_orientation: KeyboardOrientation::Cat,
            render_scale: 1.0,
            dafeiyu: DaFeiYu::new(),
            privacy_close: 0.0,
            privacy_typing_at: -10000.0,
            last_mouse_nx: 0.5,
            last_dt: 1.0 / 60.0,
            logical_w: 420.0,
            logical_h: 440.0,
            cat_center_x,
            cat_top_y: 82.0,
            cat_base_y,
            cat_height: 226.0,
            kb_center_x: 214.0,
            kb_center_y: 353.0,
            kb_width: 266.0,
            kb_height: 74.0,
            kb_tilt_rad: 3.4 * PI / 180.0,
            mouse_base_x: 46.0,
            mouse_base_y: 338.0,
            mouse_offset_x: 0.0,
            mouse_offset_y: 0.0,
            mouse_target_offset_x: 0.0,
            mouse_target_offset_y: 0.0,
            mouse_left_down: false,
            mouse_right_down: false,
            mouse_middle_down: false,
            mouse_left_press: 0.0,
            mouse_right_press: 0.0,
            mouse_middle_press: 0.0,
            mouse_scroll_flash: 0.0,
            last_mouse_move_at: -10000.0,
            mouse_badges: Vec::new(),
            cursor_rel_x: 0.5,
            cursor_rel_y: 0.5,
            cursor_over_cat: false,
            eye_look_x: 0.0,
            eye_look_y: 0.0,
            face_turn_x: 0.0,
            spine_lean: 0.0,
            head_tilt: 0.0,
            typing_hunch: 0.0,
            typing_sway: 0.0,
            typing_sway_vel: 0.0,
            typing_groove_phase: 0.0,
            last_key_bias: 0.0,
            strike_rock: 0.0,
            strike_rock_vel: 0.0,
            body_bob: 0.0,
            kb_bounce: 0.0,
            next_blink_at: 2.8,
            blink_progress: 0.0,
            is_blinking: false,
            last_pet_cursor_x: None,
            last_pet_cursor_y: None,
            pet_energy: 0.0,
            squint_amount: 0.0,
            ear_twitch_left: 0.0,
            ear_twitch_right: 0.0,
            recent_key_times: Vec::new(),
            typing_energy: 0.0,
            last_any_key_at: -10000.0,
            left_paw: new_paw(PawSide::Left, cat_center_x, cat_base_y),
            right_paw: new_paw(PawSide::Right, cat_center_x, cat_base_y),
            last_space_paw: PawSide::Left,
            key_anims,
            bubbles: Vec::new(),
            keyboard_cache: std::cell::RefCell::new(None),
            popup_counter: 1,
        }
    }

    /// Turns the password guard on or off. While on, key presses are hidden.
    ///
    /// @param active True while a password field has focus.
    pub fn set_privacy_mode(&mut self, active: bool) {
        self.privacy_mode = active;
        self.dafeiyu.on_privacy_mode_changed(active);
        if active {
            self.bubbles.clear();
            for anim in self.key_anims.values_mut() {
                anim.pressed = false;
                anim.press_amount = 0.0;
            }
        }
    }

    /// Records a hidden key press while the password guard is on (shows the typing dots).
    pub fn on_privacy_typing(&mut self) {
        let now = now_ms();
        self.privacy_typing_at = now;
        if self.cat_breed == CatBreed::DaFeiYu {
            self.dafeiyu.on_privacy_typing(now);
        }
    }

    /// Sets the pet scale used when drawing.
    ///
    /// @param scale Scale from 0.2 to 1.5.
    pub fn set_render_scale(&mut self, scale: f32) {
        self.render_scale = scale.clamp(0.2, 1.5);
    }

    /// Updates the positions of the cat, keyboard and mouse for the current layout and character.
    fn sync_layout_positions(&mut self) {
        let is_big_fish = self.cat_breed == CatBreed::DaFeiYu;
        if self.keyboard_layout == KeyboardLayoutId::Full108 {
            self.logical_w = 540.0;
            self.logical_h = 456.0;
            self.cat_center_x = 278.0;
            self.cat_top_y = 66.0;
            self.cat_base_y = 292.0;
            self.kb_center_x = 290.0;
            self.kb_center_y = 356.0;
            self.kb_width = 398.0;
            self.kb_height = 96.0;
            self.kb_tilt_rad = 2.4 * PI / 180.0;
            self.mouse_base_x = if is_big_fish { 54.0 } else { 46.0 };
            self.mouse_base_y = if is_big_fish { 332.0 } else { 342.0 };
        } else {
            self.logical_w = 420.0;
            self.logical_h = 440.0;
            self.cat_center_x = 196.0;
            self.cat_top_y = 82.0;
            self.cat_base_y = 308.0;
            self.kb_center_x = if is_big_fish { 234.0 } else { 214.0 };
            self.kb_center_y = 353.0;
            self.kb_width = 266.0;
            self.kb_height = 74.0;
            self.kb_tilt_rad = 3.4 * PI / 180.0;
            self.mouse_base_x = if is_big_fish { 72.0 } else { 46.0 };
            self.mouse_base_y = if is_big_fish { 322.0 } else { 338.0 };
        }
    }

    /// Chooses which character to draw.
    ///
    /// @param breed_id The cat id ("black", "ragdoll", "dafeiyu" or a custom id).
    /// @param skin The cat's configuration, when known.
    pub fn set_cat_appearance(&mut self, breed_id: &str, skin: Option<&CatSkinConfig>) {
        let is_builtin_skin = match skin {
            Some(config) => config.builtin,
            None => true,
        };
        if breed_id == "dafeiyu" {
            self.cat_breed = CatBreed::DaFeiYu;
            self.custom_skin = None;
        } else if (breed_id == "black" || breed_id == "ragdoll") && is_builtin_skin {
            self.cat_breed = if breed_id == "ragdoll" { CatBreed::Ragdoll } else { CatBreed::Black };
            self.custom_skin = None;
        } else if let Some(config) = skin {
            self.cat_breed = CatBreed::Black;
            self.custom_skin = Some(crate::skins::resolve_custom_skin(config));
        } else {
            self.cat_breed = CatBreed::Black;
            self.custom_skin = None;
        }
        self.sync_layout_positions();
    }

    /// Chooses the keyboard layout and which way it faces.
    ///
    /// @param layout_id 65% or 108-key.
    /// @param orientation Facing the cat or the viewer.
    pub fn set_keyboard_config(&mut self, layout_id: KeyboardLayoutId, orientation: KeyboardOrientation) {
        let layout_changed = self.keyboard_layout != layout_id;
        self.keyboard_layout = layout_id;
        self.keyboard_orientation = orientation;
        self.sync_layout_positions();
        if layout_changed {
            let left_start = Vec2::new(self.cat_center_x - 18.0, self.cat_base_y);
            let right_start = Vec2::new(self.cat_center_x + 18.0, self.cat_base_y);
            self.left_paw.pos = left_start;
            self.left_paw.target = left_start;
            self.right_paw.pos = right_start;
            self.right_paw.target = right_start;
        }
    }

    /// Collects the values the whale-girl character needs from the main engine.
    ///
    /// @return The layout snapshot.
    pub(crate) fn build_dafeiyu_layout(&self) -> DaFeiYuLayout {
        let mouse_x = self.mouse_base_x + self.mouse_offset_x;
        let mouse_y = self.mouse_base_y + self.mouse_offset_y;
        let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;
        let local_button_x = if self.mouse_left_down {
            -6.5
        } else if self.mouse_right_down {
            6.5
        } else {
            0.0
        };
        let local_button_y = -10.5;
        let angle = self.kb_tilt_rad + if is_cat_facing { PI } else { 0.0 };
        let cos = angle.cos();
        let sin = angle.sin();
        DaFeiYuLayout {
            logical_w: self.logical_w,
            kb_center_x: self.kb_center_x,
            kb_center_y: self.kb_center_y,
            kb_width: self.kb_width,
            kb_height: self.kb_height,
            mouse_btn_x: mouse_x + local_button_x * cos - local_button_y * sin,
            mouse_btn_y: mouse_y + local_button_x * sin + local_button_y * cos,
            mouse_nx: self.last_mouse_nx,
            mouse_left: self.mouse_left_down,
            mouse_right: self.mouse_right_down,
            mouse_active_at: self.last_mouse_move_at,
            pet_energy: self.pet_energy,
            privacy_mode: self.privacy_mode,
            privacy_typing_at: self.privacy_typing_at,
            is_108: self.keyboard_layout == KeyboardLayoutId::Full108,
        }
    }

    // ----- Flexible spine -------------------------------------------------------------------

    /// Returns a point on the cat's bending spine.
    ///
    /// @param t Position along the body: 0 = ear tips, 1 = seated base on the desk.
    /// @param breath Breathing offset in pixels.
    /// @return The point in logical pixels.
    pub(crate) fn spine_point(&self, t: f32, breath: f32) -> Vec2 {
        let u = (1.0 - t).clamp(0.0, 1.0);
        let shear_curve = u * u * (3.0 - 2.0 * u);
        let lean_amplitude = if self.keyboard_layout == KeyboardLayoutId::Full108 { 76.0 } else { 46.0 };
        let lean_x = (self.spine_lean * lean_amplitude + self.typing_sway * 25.0 + self.strike_rock * 10.0) * shear_curve;
        let sway_dip_y = self.typing_sway.abs() * 4.0 * shear_curve;
        let hunch_y = (self.typing_hunch * 13.0 + self.body_bob * 4.8 + breath) * shear_curve + sway_dip_y;
        let lean_drop_y = self.spine_lean.abs() * 6.0 * shear_curve;
        Vec2::new(
            self.cat_center_x + lean_x,
            self.cat_top_y + t * self.cat_height + hunch_y + lean_drop_y,
        )
    }

    /// Returns the current shoulder position of a paw.
    ///
    /// @param paw The paw.
    /// @param breath Breathing offset in pixels.
    /// @return The shoulder point.
    pub(crate) fn get_live_shoulder(&self, paw: &PawState, breath: f32) -> Vec2 {
        let spine = self.spine_point(paw.shoulder_norm_y, breath);
        Vec2::new(spine.x + paw.shoulder_half_x, spine.y)
    }

    /// Returns where a curled paw rests near the lower belly.
    ///
    /// @param paw The paw.
    /// @param breath Breathing offset in pixels.
    /// @return The curled paw position.
    pub(crate) fn get_live_curled_paw_pos(&self, paw: &PawState, breath: f32) -> Vec2 {
        let spine = self.spine_point(0.84, breath);
        let phase = if paw.side == PawSide::Left { 0.0 } else { PI };
        let bob = (self.elapsed as f32 * 12.0 + phase).sin() * (1.5 + self.typing_energy * 2.0);
        let offset_x = if paw.side == PawSide::Left { -24.0 } else { 24.0 };
        Vec2::new(spine.x + offset_x, spine.y + bob)
    }

    /// Returns a paw by side.
    ///
    /// @param side Which paw.
    /// @return The paw.
    fn paw(&self, side: PawSide) -> &PawState {
        match side {
            PawSide::Left => &self.left_paw,
            PawSide::Right => &self.right_paw,
        }
    }

    /// Returns a paw by side, for changing it.
    ///
    /// @param side Which paw.
    /// @return The paw.
    fn paw_mut(&mut self, side: PawSide) -> &mut PawState {
        match side {
            PawSide::Left => &mut self.left_paw,
            PawSide::Right => &mut self.right_paw,
        }
    }

    // ----- Input ----------------------------------------------------------------------------

    /// Handles a global key press.
    ///
    /// @param vk Windows virtual-key code.
    /// @param repeat True for auto-repeat events while the key is held.
    pub fn on_key_down(&mut self, vk: u32, repeat: bool) {
        if self.privacy_mode {
            if !repeat {
                self.on_privacy_typing();
            }
            return;
        }
        let now = now_ms();
        let Some(spec) = resolve_key_by_vk(vk, self.keyboard_layout) else {
            return;
        };
        if let Some(anim) = self.key_anims.get_mut(spec.id) {
            anim.pressed = true;
            anim.press_amount = 1.0;
            anim.last_hit_at = now;
        }
        if repeat {
            return;
        }

        self.last_any_key_at = now;
        self.recent_key_times.push(now);
        if self.recent_key_times.len() > 26 {
            self.recent_key_times.remove(0);
        }
        self.body_bob = (self.body_bob + 0.65).min(1.15);
        self.kb_bounce = (self.kb_bounce + 0.45).min(1.0);

        if self.cat_breed == CatBreed::DaFeiYu {
            let point = self.get_key_world_center(spec);
            let dafeiyu_layout = self.build_dafeiyu_layout();
            self.dafeiyu.on_key_down(vk, point, &dafeiyu_layout, repeat, now);
            if self.show_key_bubbles {
                let label = format_bubble_for_vk(vk, Some(spec));
                if !label.is_empty() {
                    self.spawn_bubble(&label, spec.id == "space", point.x, now);
                }
            }
            return;
        }

        if random() < 0.16 {
            if random() < 0.5 {
                self.ear_twitch_left = 1.0;
            } else {
                self.ear_twitch_right = 1.0;
            }
        }

        let keyboard = layout(self.keyboard_layout);
        let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;
        let key_column_center = spec.u_start + spec.u_width * 0.5;
        let screen_key_column = if is_cat_facing {
            keyboard.total_units - key_column_center
        } else {
            key_column_center
        };
        let middle_column = keyboard.total_units * 0.5;
        let key_bias = ((screen_key_column - middle_column) / (keyboard.total_units * 0.34)).clamp(-1.0, 1.0);
        self.last_key_bias = key_bias;
        self.typing_groove_phase += 1.22;

        let mut target_point = self.get_key_world_center(spec);
        let actively_mousing = self.mouse_left_down || self.mouse_right_down || now - self.last_mouse_move_at < 320.0;
        let split_column = if self.keyboard_layout == KeyboardLayoutId::Full108 {
            if is_cat_facing {
                12.2
            } else {
                8.4
            }
        } else {
            7.5
        };

        let chosen_side: PawSide;
        if spec.id == "space" {
            if actively_mousing {
                chosen_side = PawSide::Right;
            } else {
                self.last_space_paw = if self.last_space_paw == PawSide::Left { PawSide::Right } else { PawSide::Left };
                chosen_side = self.last_space_paw;
            }
            let space_u = if is_cat_facing {
                if chosen_side == PawSide::Left {
                    spec.u_start + spec.u_width * 0.68
                } else {
                    spec.u_start + spec.u_width * 0.32
                }
            } else if chosen_side == PawSide::Left {
                spec.u_start + spec.u_width * 0.32
            } else {
                spec.u_start + spec.u_width * 0.68
            };
            target_point = self.project_keyboard_point(space_u, spec.row_y + 0.50);
        } else if actively_mousing {
            chosen_side = PawSide::Right;
        } else if screen_key_column < split_column {
            chosen_side = PawSide::Left;
        } else {
            chosen_side = PawSide::Right;
        }

        let side_sign = if chosen_side == PawSide::Left { -1.0 } else { 1.0 };
        self.strike_rock_vel += side_sign * (1.75 + self.typing_energy * 1.25);
        if !actively_mousing {
            self.typing_sway_vel += (key_bias * 2.1 + self.typing_groove_phase.cos() * 1.65)
                * (0.9 + self.typing_energy * 0.45);
        }

        let mut chosen = self.paw(chosen_side).clone();
        let previous_shoulder = self.get_live_shoulder(&chosen, 0.0);
        let travel = (target_point.x - chosen.pos.x).hypot(target_point.y - chosen.pos.y);
        if chosen.mode == PawMode::Poke && travel > 22.0 {
            chosen.smears.push(SmearFrame {
                sx: previous_shoulder.x,
                sy: previous_shoulder.y,
                px: chosen.pos.x,
                py: chosen.pos.y,
                alpha: 0.28,
            });
            if chosen.smears.len() > 3 {
                chosen.smears.remove(0);
            }
        }
        if chosen.mode == PawMode::Curled || chosen.mode == PawMode::Idle {
            let start_point = self.get_live_curled_paw_pos(&chosen, 0.0);
            chosen.pos = start_point;
            chosen.vel = Vec2::default();
        }
        chosen.mode = PawMode::Poke;
        chosen.target = self.clamp_to_keyboard_surface(target_point);
        chosen.poke_time = 0.0;
        chosen.impact_squash = 0.85;
        chosen.last_active_at = now;
        *self.paw_mut(chosen_side) = chosen;

        let other_side = if chosen_side == PawSide::Left { PawSide::Right } else { PawSide::Left };
        let other = self.paw_mut(other_side);
        if other.mode == PawMode::Idle && !actively_mousing {
            other.mode = PawMode::Curled;
            other.last_active_at = now;
        }

        if self.show_key_bubbles {
            let label = format_bubble_for_vk(vk, Some(spec));
            if !label.is_empty() {
                self.spawn_bubble(&label, spec.id == "space", target_point.x, now);
            }
        }
    }

    /// Handles a global key release.
    ///
    /// @param vk Windows virtual-key code.
    pub fn on_key_up(&mut self, vk: u32) {
        self.dafeiyu.on_key_up(vk);
        if self.privacy_mode {
            return;
        }
        let Some(spec) = resolve_key_by_vk(vk, self.keyboard_layout) else {
            return;
        };
        if let Some(anim) = self.key_anims.get_mut(spec.id) {
            anim.pressed = false;
        }
    }

    /// Handles a global mouse move.
    ///
    /// @param nx Cursor x across the whole virtual screen (0..1).
    /// @param ny Cursor y across the whole virtual screen (0..1).
    pub fn on_global_mouse_move(&mut self, nx: f32, ny: f32) {
        let now = now_ms();
        let is_big_fish = self.cat_breed == CatBreed::DaFeiYu;
        self.last_mouse_nx = nx;
        self.mouse_target_offset_x = (nx - 0.5) * if is_big_fish { 14.0 } else { 28.0 };
        self.mouse_target_offset_y = (ny - 0.5) * if is_big_fish { 11.0 } else { 20.0 };
        self.last_mouse_move_at = now;
        self.left_paw.mode = PawMode::Mouse;
        self.left_paw.last_active_at = now;
    }

    /// Returns the colours of the left, right and middle mouse buttons for the current character.
    ///
    /// @return The button colours.
    pub(crate) fn get_mouse_button_colors(&self) -> MouseButtonColors {
        let primary = if let Some(skin) = &self.custom_skin {
            skin.accent_color.clone()
        } else {
            match self.cat_breed {
                CatBreed::DaFeiYu => "#5cc8ff".to_string(),
                CatBreed::Ragdoll => "#7ac7ff".to_string(),
                CatBreed::Black => "#d4f044".to_string(),
            }
        };
        let scroll = if let Some(skin) = &self.custom_skin {
            skin.accent_side_color.clone()
        } else {
            match self.cat_breed {
                CatBreed::DaFeiYu => "#2563eb".to_string(),
                CatBreed::Ragdoll => "#4690d8".to_string(),
                CatBreed::Black => "#92ba12".to_string(),
            }
        };
        let right = if is_warm_accent(&primary) { "#58c7ff" } else { "#ff7b3b" };
        MouseButtonColors {
            left: primary,
            right: right.to_string(),
            middle: scroll,
        }
    }

    /// Adds an "LMB" / "RMB" / "MMB" badge above the mouse.
    ///
    /// @param button Which button was clicked.
    /// @param now Current time in milliseconds.
    fn spawn_mouse_badge(&mut self, button: MouseButton, now: f64) {
        if !self.show_key_bubbles {
            return;
        }
        let colors = self.get_mouse_button_colors();
        let (label, color) = match button {
            MouseButton::Left => ("LMB", colors.left),
            MouseButton::Right => ("RMB", colors.right),
            MouseButton::Middle => ("MMB", colors.middle),
        };
        let mouse_x = self.mouse_base_x + self.mouse_offset_x;
        let mouse_y = self.mouse_base_y + self.mouse_offset_y;
        let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;
        let side_shift = match button {
            MouseButton::Left => if is_cat_facing { 10.0 } else { -10.0 },
            MouseButton::Right => if is_cat_facing { -10.0 } else { 10.0 },
            MouseButton::Middle => 0.0,
        };
        if self.mouse_badges.len() >= 2 {
            self.mouse_badges.remove(0);
        }
        self.popup_counter += 1;
        self.mouse_badges.push(MouseBadge {
            label: label.to_string(),
            color,
            x: (mouse_x + side_shift).max(24.0),
            y: mouse_y - 30.0,
            created_at: now,
            lifetime: 560.0,
        });
    }

    /// Handles a global mouse button press.
    ///
    /// @param button "left", "right" or "middle".
    pub fn on_mouse_down(&mut self, button: &str) {
        let now = now_ms();
        match button {
            "left" => {
                self.mouse_left_down = true;
                self.mouse_left_press = 1.0;
                self.spawn_mouse_badge(MouseButton::Left, now);
            }
            "right" => {
                self.mouse_right_down = true;
                self.mouse_right_press = 1.0;
                self.spawn_mouse_badge(MouseButton::Right, now);
            }
            "middle" => {
                self.mouse_middle_down = true;
                self.mouse_middle_press = 1.0;
                self.spawn_mouse_badge(MouseButton::Middle, now);
            }
            _ => {}
        }
        self.last_mouse_move_at = now;
        self.dafeiyu.reset_left_hand_for_mouse();
        self.left_paw.mode = PawMode::Mouse;
        self.left_paw.impact_squash = 0.72;
        self.left_paw.last_active_at = now;
    }

    /// Handles a global mouse button release.
    ///
    /// @param button "left", "right" or "middle".
    pub fn on_mouse_up(&mut self, button: &str) {
        match button {
            "left" => self.mouse_left_down = false,
            "right" => self.mouse_right_down = false,
            "middle" => self.mouse_middle_down = false,
            _ => {}
        }
    }

    /// Handles a global mouse wheel turn.
    pub fn on_mouse_wheel(&mut self) {
        let now = now_ms();
        self.mouse_scroll_flash = 1.0;
        self.last_mouse_move_at = now;
        self.dafeiyu.reset_left_hand_for_mouse();
        self.left_paw.mode = PawMode::Mouse;
        self.left_paw.last_active_at = now;
    }

    /// Handles the cursor position relative to the pet (eye tracking and petting).
    ///
    /// @param rel_x Cursor x relative to the pet (0..1 inside).
    /// @param rel_y Cursor y relative to the pet (0..1 inside).
    /// @param over_cat True when the cursor hovers over the cat.
    pub fn on_window_cursor(&mut self, rel_x: f32, rel_y: f32, over_cat: bool) {
        self.cursor_rel_x = rel_x;
        self.cursor_rel_y = rel_y;
        self.cursor_over_cat = over_cat;
        if over_cat && !self.mouse_left_down && !self.mouse_right_down {
            let point_x = rel_x * self.logical_w;
            let point_y = rel_y * self.logical_h;
            if let (Some(last_x), Some(last_y)) = (self.last_pet_cursor_x, self.last_pet_cursor_y) {
                let distance = (point_x - last_x).hypot(point_y - last_y);
                if distance > 1.5 && distance < 80.0 {
                    self.pet_energy = (self.pet_energy + distance * 0.011).min(1.0);
                }
            }
            self.last_pet_cursor_x = Some(point_x);
            self.last_pet_cursor_y = Some(point_y);
        } else {
            self.last_pet_cursor_x = None;
            self.last_pet_cursor_y = None;
        }
    }

    // ----- Keyboard projection --------------------------------------------------------------

    /// Maps a position on the keyboard (in key units) to logical pixels on the tilted deck.
    ///
    /// @param u_column Column in key units.
    /// @param row_position Row in key rows.
    /// @return The point in logical pixels.
    pub(crate) fn project_keyboard_point(&self, u_column: f32, row_position: f32) -> Vec2 {
        let keyboard = layout(self.keyboard_layout);
        let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;
        let screen_u = if is_cat_facing { keyboard.total_units - u_column } else { u_column };
        let screen_row = if is_cat_facing { keyboard.total_rows - row_position } else { row_position };

        let pad_x = 7.0;
        let pad_y = 6.0;
        let inner_w = self.kb_width - pad_x * 2.0;
        let inner_h = self.kb_height - pad_y * 2.0;
        let local_x = -inner_w / 2.0 + (screen_u / keyboard.total_units) * inner_w;
        let local_y = -inner_h / 2.0 + (screen_row / keyboard.total_rows) * inner_h;

        let depth_factor = 0.94 + (screen_row / keyboard.total_rows) * 0.09;
        let point_x = local_x * depth_factor;
        let point_y = local_y;
        let cos = self.kb_tilt_rad.cos();
        let sin = self.kb_tilt_rad.sin();
        Vec2::new(
            self.kb_center_x + point_x * cos - point_y * sin,
            self.kb_center_y + point_x * sin + point_y * cos,
        )
    }

    /// Returns where a paw should hit a key.
    ///
    /// @param spec The key.
    /// @return The point in logical pixels.
    pub(crate) fn get_key_world_center(&self, spec: &KeySpec) -> Vec2 {
        let keyboard = layout(self.keyboard_layout);
        let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;
        let u_clamped = (spec.u_start + spec.u_width * 0.5).clamp(0.65, keyboard.total_units - 0.65);
        let row_y = spec.row_y;
        let row_span = spec.row_span;
        let mut row_position = row_y + row_span * 0.5;
        if is_cat_facing && row_y <= 0.1 {
            row_position = row_y + 0.68;
        } else if !is_cat_facing && row_y + row_span >= keyboard.total_rows - 0.1 {
            row_position = row_y + row_span - 0.68;
        }
        self.project_keyboard_point(u_clamped, row_position)
    }

    /// Keeps a point inside the keyboard surface so the paw pad never hangs over the edge.
    ///
    /// @param point The point in logical pixels.
    /// @return The clamped point.
    pub(crate) fn clamp_to_keyboard_surface(&self, point: Vec2) -> Vec2 {
        let dx = point.x - self.kb_center_x;
        let dy = point.y - self.kb_center_y;
        let cos = (-self.kb_tilt_rad).cos();
        let sin = (-self.kb_tilt_rad).sin();
        let local_x = dx * cos - dy * sin;
        let local_y = dx * sin + dy * cos;

        let max_x = self.kb_width * 0.5 - 15.0;
        let min_y = -self.kb_height * 0.5 + 8.0;
        let max_y = self.kb_height * 0.5 - 14.0;
        let clamped_x = local_x.clamp(-max_x, max_x);
        let clamped_y = local_y.clamp(min_y, max_y);

        let back_cos = self.kb_tilt_rad.cos();
        let back_sin = self.kb_tilt_rad.sin();
        Vec2::new(
            self.kb_center_x + clamped_x * back_cos - clamped_y * back_sin,
            self.kb_center_y + clamped_x * back_sin + clamped_y * back_cos,
        )
    }

    /// Adds a keycap bubble above the keyboard.
    ///
    /// @param label Text in the bubble.
    /// @param is_space True for the space bar (drawn as a ␣ glyph).
    /// @param key_x Logical x of the pressed key.
    /// @param now Current time in milliseconds.
    fn spawn_bubble(&mut self, label: &str, is_space: bool, key_x: f32, now: f64) {
        if self.bubbles.len() >= 6 {
            self.bubbles.remove(0);
        }
        let min_x = if self.keyboard_layout == KeyboardLayoutId::Full108 { 145.0 } else { 125.0 };
        let max_x = self.logical_w - 105.0;
        let base_x = (key_x * 0.45 + self.kb_center_x * 0.55).min(max_x).max(min_x);
        let stagger = ((self.popup_counter % 3) as f32) - 1.0;
        let stagger_y = 258.0 + stagger * 10.0 + (random() - 0.5) * 6.0;
        self.popup_counter += 1;
        self.bubbles.push(KeyBubble {
            label: label.to_string(),
            is_space,
            x: base_x,
            target_x: base_x,
            y: 292.0,
            target_y: stagger_y,
            rot: (random() - 0.5) * 4.5 * PI / 180.0,
            created_at: now,
            lifetime: 940.0,
        });
        self.layout_bubbles();
    }

    /// Spreads the bubbles out in one row centred above the keyboard.
    fn layout_bubbles(&mut self) {
        let count = self.bubbles.len();
        if count == 0 {
            return;
        }
        let mut widths: Vec<f32> = Vec::with_capacity(count);
        for bubble in &self.bubbles {
            let width = if bubble.is_space {
                40.0
            } else {
                (bubble.label.chars().count() as f32 * 7.5 + 14.0).max(25.0)
            };
            widths.push(width);
        }
        let gap = 3.5;
        let total_width: f32 = widths.iter().sum::<f32>() + gap * (count as f32 - 1.0);
        let anchor = self.kb_center_x + 12.0;
        let left_bound = if self.keyboard_layout == KeyboardLayoutId::Full108 { 140.0 } else { 126.0 };
        let right_bound = self.logical_w - 82.0;
        let mut cursor = (anchor - total_width * 0.5).min(right_bound - total_width).max(left_bound);
        for (index, width) in widths.iter().enumerate() {
            self.bubbles[index].target_x = cursor + width * 0.5;
            cursor += width + gap;
        }
    }

    // ----- Simulation -----------------------------------------------------------------------

    /// Advances the animation using the real time since the previous call.
    ///
    /// @param now Current time in milliseconds (from `now_ms`).
    pub fn tick(&mut self, now: f64) {
        let dt = ((now - self.last_time) / 1000.0).clamp(0.001, 0.05) as f32;
        self.last_time = now;
        self.update(dt, now);
    }

    /// Returns true when nothing is moving except breathing, so the window can draw less often.
    ///
    /// @param now Current time in milliseconds.
    /// @return True when idle.
    pub fn is_idle(&self, now: f64) -> bool {
        let quiet_for = 5000.0;
        if now - self.last_any_key_at < quiet_for || now - self.last_mouse_move_at < quiet_for {
            return false;
        }
        if now - self.privacy_typing_at < quiet_for {
            return false;
        }
        if self.pet_energy > 0.01 || self.squint_amount > 0.01 || self.cursor_over_cat {
            return false;
        }
        if !self.bubbles.is_empty() || !self.mouse_badges.is_empty() {
            return false;
        }
        if self.mouse_left_down || self.mouse_right_down || self.mouse_middle_down {
            return false;
        }
        let privacy_target = if self.privacy_mode { 1.0 } else { 0.0 };
        if (self.privacy_close - privacy_target).abs() > 0.01 {
            return false;
        }
        for anim in self.key_anims.values() {
            if anim.pressed || anim.press_amount > 0.0 {
                return false;
            }
        }
        for paw in [&self.left_paw, &self.right_paw] {
            if paw.mode != PawMode::Idle || paw.idle_weight < 0.99 || !paw.smears.is_empty() {
                return false;
            }
        }
        self.spine_lean.abs() < 0.01 && self.typing_sway.abs() < 0.01 && self.typing_hunch < 0.01
    }

    /// Advances all animation by one frame.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in milliseconds.
    pub fn update(&mut self, dt: f32, now: f64) {
        self.elapsed += dt as f64;

        let mouse_lerp = 1.0 - (-28.0 * dt).exp();
        self.mouse_offset_x += (self.mouse_target_offset_x - self.mouse_offset_x) * mouse_lerp;
        self.mouse_offset_y += (self.mouse_target_offset_y - self.mouse_offset_y) * mouse_lerp;
        if !self.mouse_left_down && self.mouse_left_press > 0.0 {
            self.mouse_left_press = (self.mouse_left_press - dt * 5.8).max(0.0);
        }
        if !self.mouse_right_down && self.mouse_right_press > 0.0 {
            self.mouse_right_press = (self.mouse_right_press - dt * 5.8).max(0.0);
        }
        if !self.mouse_middle_down && self.mouse_middle_press > 0.0 {
            self.mouse_middle_press = (self.mouse_middle_press - dt * 5.8).max(0.0);
        }
        self.mouse_scroll_flash = (self.mouse_scroll_flash - dt * 4.5).max(0.0);
        self.mouse_badges.retain(|badge| now - badge.created_at < badge.lifetime);

        while let Some(oldest) = self.recent_key_times.first() {
            if now - *oldest > 950.0 {
                self.recent_key_times.remove(0);
            } else {
                break;
            }
        }
        let target_typing_energy = (self.recent_key_times.len() as f32 / 7.0).min(1.0);
        self.typing_energy = approach(self.typing_energy, target_typing_energy, 9.0, dt);

        let is_typing_recently = now - self.last_any_key_at < 650.0;
        let target_hunch = if is_typing_recently { 0.45 + self.typing_energy * 0.55 } else { 0.0 };
        self.typing_hunch = approach(self.typing_hunch, target_hunch, 10.0, dt);

        let rock_acceleration = -self.strike_rock * 140.0 - self.strike_rock_vel * 14.0;
        self.strike_rock_vel += rock_acceleration * dt;
        self.strike_rock = (self.strike_rock + self.strike_rock_vel * dt).clamp(-1.0, 1.0);

        self.body_bob = (self.body_bob - dt * 11.5).max(0.0);
        self.kb_bounce = (self.kb_bounce - dt * 15.0).max(0.0);

        // Playful left-right sway of the body and head while typing.
        let pure_typing_active = is_typing_recently && self.left_paw.mode != PawMode::Mouse;
        if pure_typing_active {
            self.typing_groove_phase += dt * (4.8 + self.typing_energy * 5.2);
        }
        let groove_wave = self.typing_groove_phase.sin() * (0.58 + self.typing_energy * 0.44);
        let target_typing_sway = if pure_typing_active {
            (groove_wave * 0.72 + self.last_key_bias * 0.55).clamp(-1.1, 1.1)
        } else if self.left_paw.mode == PawMode::Mouse && is_typing_recently {
            self.typing_groove_phase.sin() * 0.16
        } else {
            0.0
        };
        let sway_acceleration = (target_typing_sway - self.typing_sway) * 118.0 - self.typing_sway_vel * 12.5;
        self.typing_sway_vel += sway_acceleration * dt;
        self.typing_sway = (self.typing_sway + self.typing_sway_vel * dt).clamp(-1.2, 1.2);

        // Lean the head and chest towards the mouse while the hips stay in place.
        let mut target_lean = 0.0;
        if self.left_paw.mode == PawMode::Mouse {
            let mouse_sweep_bias = (self.mouse_offset_x / 18.0) * 0.22;
            target_lean = if self.right_paw.mode == PawMode::Poke {
                -0.52 + mouse_sweep_bias
            } else {
                -0.96 + mouse_sweep_bias
            };
        }
        self.spine_lean = approach(self.spine_lean, target_lean, 10.0, dt);

        let desired_head_tilt = self.typing_sway * 0.18
            + self.typing_hunch * (self.strike_rock * 0.065 - 0.02)
            + self.spine_lean * 0.085;
        self.head_tilt = approach(self.head_tilt, desired_head_tilt, 12.0, dt);

        let pet_decay = if self.cursor_over_cat { 0.22 } else { 0.65 };
        self.pet_energy = (self.pet_energy - dt * pet_decay).max(0.0);
        self.last_dt = dt;
        let want_squint = if self.pet_energy > 0.3 { 1.0 } else { 0.0 };
        self.squint_amount = approach(self.squint_amount, want_squint, 12.0, dt);
        let want_privacy = if self.privacy_mode { 1.0 } else { 0.0 };
        self.privacy_close = approach(self.privacy_close, want_privacy, 14.0, dt);
        let purr = if !self.privacy_mode && self.squint_amount > 0.25 { self.squint_amount } else { 0.0 };
        crate::audio::set_purr_intensity(purr);
        if self.cat_breed == CatBreed::DaFeiYu {
            let dafeiyu_layout = self.build_dafeiyu_layout();
            self.dafeiyu.update(dt, now, &dafeiyu_layout);
        }

        self.ear_twitch_left = (self.ear_twitch_left - dt * 8.0).max(0.0);
        self.ear_twitch_right = (self.ear_twitch_right - dt * 8.0).max(0.0);

        if !self.is_blinking && self.elapsed >= self.next_blink_at {
            self.is_blinking = true;
            self.blink_progress = 0.0;
        }
        if self.is_blinking {
            self.blink_progress += dt * 12.0;
            if self.blink_progress >= 2.0 {
                self.is_blinking = false;
                self.blink_progress = 0.0;
                self.next_blink_at = self.elapsed + 2.8 + random() as f64 * 3.6;
            }
        }

        // Eyes follow the cursor, or glance down at the keys while typing.
        let eye_position = self.spine_point(0.24, 0.0);
        let eye_center_rel_x = eye_position.x / self.logical_w;
        let eye_center_rel_y = eye_position.y / self.logical_h;
        let mut dx = self.cursor_rel_x - eye_center_rel_x;
        let mut dy = self.cursor_rel_y - eye_center_rel_y;
        let distance = dx.hypot(dy);
        if distance > 0.001 {
            let magnitude = (distance * 1.9).min(1.0);
            dx = dx / distance * magnitude;
            dy = dy / distance * magnitude;
        }
        let mouse_recently_moved = now - self.last_mouse_move_at < 2400.0;
        let pure_typing_glance = if mouse_recently_moved { 0.0 } else { self.typing_hunch };
        let target_eye_x = dx * 7.0 * (1.0 - pure_typing_glance * 0.45)
            + pure_typing_glance * (self.typing_sway * 3.8 + self.last_key_bias * 2.2);
        let target_eye_y = dy * 5.2 * (1.0 - pure_typing_glance * 0.45) + pure_typing_glance * 5.8;
        self.eye_look_x = approach(self.eye_look_x, target_eye_x, 20.0, dt);
        self.eye_look_y = approach(self.eye_look_y, target_eye_y, 20.0, dt);

        let target_face_turn = dx * 4.0 + self.spine_lean * 8.0 + self.typing_sway * 8.5 + pure_typing_glance * 1.5;
        self.face_turn_x = approach(self.face_turn_x, target_face_turn, 11.0, dt);

        for anim in self.key_anims.values_mut() {
            if !anim.pressed && anim.press_amount > 0.0 {
                anim.press_amount = (anim.press_amount - dt * 5.0).max(0.0);
            }
        }

        self.update_paw(PawSide::Left, dt, now);
        self.update_paw(PawSide::Right, dt, now);

        let count_before = self.bubbles.len();
        self.bubbles.retain(|bubble| now - bubble.created_at < bubble.lifetime);
        if self.bubbles.len() != count_before {
            self.layout_bubbles();
        }
        for bubble in self.bubbles.iter_mut() {
            let age = ((now - bubble.created_at) / bubble.lifetime) as f32;
            bubble.x = approach(bubble.x, bubble.target_x, 18.0, dt);
            let desired_y = bubble.target_y - age * 10.0;
            bubble.y = approach(bubble.y, desired_y, 16.0, dt);
        }
    }

    /// Advances one paw: chooses its mode, its target and moves it towards the target.
    ///
    /// @param side Which paw.
    /// @param dt Frame time in seconds.
    /// @param now Current time in milliseconds.
    fn update_paw(&mut self, side: PawSide, dt: f32, now: f64) {
        let mut paw = self.paw(side).clone();
        let live_mouse_x = self.mouse_base_x + self.mouse_offset_x;
        let live_mouse_y = self.mouse_base_y + self.mouse_offset_y;

        match paw.mode {
            PawMode::Mouse => {
                let is_cat_facing = self.keyboard_orientation == KeyboardOrientation::Cat;
                let left_right_bias = self.mouse_left_press - self.mouse_right_press;
                let any_click_press = self.mouse_left_press.max(self.mouse_right_press).max(self.mouse_middle_press);
                if is_cat_facing {
                    // Cat-facing: palm heel at the top, left button on the inner side.
                    paw.target = Vec2::new(
                        live_mouse_x + 1.2 + left_right_bias * 6.4,
                        live_mouse_y - 11.5 + any_click_press * 3.6,
                    );
                } else {
                    // Viewer-facing: palm heel at the bottom, left button on the left.
                    paw.target = Vec2::new(
                        live_mouse_x + 0.5 - left_right_bias * 6.4,
                        live_mouse_y + 10.0 - any_click_press * 3.4,
                    );
                }
                let still_holding = self.mouse_left_down
                    || self.mouse_right_down
                    || self.mouse_middle_down
                    || now - self.last_mouse_move_at < 2200.0;
                if !still_holding {
                    paw.mode = PawMode::Idle;
                }
            }
            PawMode::Poke => {
                paw.poke_time += dt;
                if paw.poke_time > 0.13 {
                    let mouse_active = self.mouse_left_down || self.mouse_right_down || now - self.last_mouse_move_at < 450.0;
                    if paw.side == PawSide::Left && mouse_active {
                        paw.mode = PawMode::Mouse;
                    } else if now - self.last_any_key_at < 650.0 {
                        paw.mode = PawMode::Curled;
                    } else {
                        paw.mode = PawMode::Idle;
                    }
                }
            }
            PawMode::Curled => {
                if now - self.last_any_key_at > 680.0 {
                    paw.mode = PawMode::Idle;
                }
            }
            PawMode::Idle => {}
        }

        if paw.mode == PawMode::Curled {
            paw.target = self.get_live_curled_paw_pos(&paw, 0.0);
        } else if paw.mode == PawMode::Idle {
            let base = self.spine_point(1.0, 0.0);
            let offset_x = if paw.side == PawSide::Left { -19.0 } else { 19.0 };
            paw.target = Vec2::new(base.x + offset_x, base.y);
        }

        let want_idle = if paw.mode == PawMode::Idle { 1.0 } else { 0.0 };
        let want_curl = if paw.mode == PawMode::Curled { 1.0 } else { 0.0 };
        paw.idle_weight = approach(paw.idle_weight, want_idle, 20.0, dt);
        paw.curl_weight = approach(paw.curl_weight, want_curl, 20.0, dt);

        // Framerate-independent following with no spring overshoot.
        let follow_rate = match paw.mode {
            PawMode::Poke => 42.0,
            PawMode::Mouse => 32.0,
            _ => 22.0,
        };
        paw.pos.x = approach(paw.pos.x, paw.target.x, follow_rate, dt);
        paw.pos.y = approach(paw.pos.y, paw.target.y, follow_rate, dt);
        paw.vel = Vec2::default();

        // While poking a key, the paw tip always stays on the keyboard surface.
        if paw.mode == PawMode::Poke {
            let clamped = self.clamp_to_keyboard_surface(paw.pos);
            if clamped.y < paw.pos.y || (clamped.x - paw.pos.x).abs() > 0.01 {
                paw.pos.x = clamped.x;
                paw.pos.y = paw.pos.y.min(clamped.y);
            }
        }

        paw.impact_squash = (paw.impact_squash - dt * 7.5).max(0.0);
        for smear in paw.smears.iter_mut() {
            smear.alpha -= dt * 3.8;
        }
        paw.smears.retain(|smear| smear.alpha > 0.02);

        *self.paw_mut(side) = paw;
    }

    // ----- Rendering ------------------------------------------------------------------------

    pub fn render(&self, ctx: &mut Canvas, device_scale: f32) {
        ctx.clear();
        let canvas_width = ctx.width() as f32;
        let canvas_height = ctx.height() as f32;

        ctx.save();
        let fit_scale = (canvas_width / self.logical_w).min(canvas_height / self.logical_h);
        let uniform_scale = fit_scale.min(self.render_scale * device_scale);
        let draw_width = self.logical_w * uniform_scale;
        let draw_height = self.logical_h * uniform_scale;
        ctx.translate(canvas_width - draw_width, canvas_height - draw_height);
        ctx.scale(uniform_scale, uniform_scale);

        let elapsed = self.elapsed as f32;
        let breath = (elapsed * 2.1).sin();
        let purr_shift = if self.squint_amount > 0.4 {
            (elapsed * 22.0).sin() * 0.45 * self.squint_amount
        } else {
            0.0
        };

        if self.cat_breed == CatBreed::DaFeiYu {
            let now = now_ms();
            let dafeiyu_layout = self.build_dafeiyu_layout();
            self.dafeiyu.render_body(ctx, now, &dafeiyu_layout);
            self.draw_mini_mouse(ctx);
            self.draw_keyboard(ctx);
            self.dafeiyu.render_foreground(ctx, self.last_dt, now, &dafeiyu_layout);
            if self.show_key_bubbles && !self.privacy_mode {
                self.draw_key_bubbles(ctx);
                self.draw_mouse_badges(ctx);
            }
            ctx.restore();
            return;
        }

        // Layer 1: tail. Layer 2: mouse and keyboard. Layer 3: body and head.
        // Layer 4: front paws. Layer 5: key bubbles and click badges.
        self.draw_tail(ctx);
        self.draw_mini_mouse(ctx);
        self.draw_keyboard(ctx);
        self.draw_cat_body(ctx, breath, purr_shift);
        self.draw_paw(ctx, &self.left_paw, breath);
        self.draw_paw(ctx, &self.right_paw, breath);
        if self.show_key_bubbles && !self.privacy_mode {
            self.draw_key_bubbles(ctx);
            self.draw_mouse_badges(ctx);
        }
        ctx.restore();
    }
}

impl Default for Engine {
    /// @return A new engine in its resting state.
    fn default() -> Engine {
        Engine::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_accent_detection_matches_original_rules() {
        assert!(is_warm_accent("#f4a524"));
        assert!(is_warm_accent("#ff9900"));
        assert!(!is_warm_accent("#d4f044"));
        assert!(!is_warm_accent("#5cc8ff"));
    }
}
