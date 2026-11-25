//! DaFeiYu ("Big Fat Fish"), the whale-girl companion character.
//!
//! The body uses MIT-licensed sprites from `PC2005-cloud/dsh-pet` (via `QCYTSN/dsh-dafeiyu`),
//! drawn without arms or tail. The round chibi hands, maid sleeves and the ocean-blue whale tail
//! are drawn as vectors. The tail slaps keys on the right edge of the keyboard. She blushes
//! when typing gets fast, keeps her left hand on the mouse while the right hand types, and
//! covers her eyes with both hands while a password is being typed.
//!
//! Drawing happens in two layers: [`DaFeiYu::render_body`] runs before the keyboard and mouse
//! are drawn, [`DaFeiYu::render_foreground`] runs after them.

use super::{now_ms, random, Vec2};
use crate::render::{Canvas, Gradient, LineCap, LineJoin, Pixmap};
use std::collections::HashSet;
use std::f32::consts::PI;

/// Everything DaFeiYu needs to know about the rest of the scene for one frame.
#[derive(Clone, Copy, Debug, Default)]
pub struct DaFeiYuLayout {
    /// Logical width of the pet area.
    pub logical_w: f32,
    /// Keyboard centre x.
    pub kb_center_x: f32,
    /// Keyboard centre y.
    pub kb_center_y: f32,
    /// Keyboard width.
    pub kb_width: f32,
    /// Keyboard height.
    pub kb_height: f32,
    /// Where the left hand rests on the mouse (x).
    pub mouse_btn_x: f32,
    /// Where the left hand rests on the mouse (y).
    pub mouse_btn_y: f32,
    /// Horizontal cursor position on screen, 0..1.
    pub mouse_nx: f32,
    /// Left mouse button is held.
    pub mouse_left: bool,
    /// Right mouse button is held.
    pub mouse_right: bool,
    /// Time of the last mouse activity (ms).
    pub mouse_active_at: f64,
    /// How much she is being petted, 0..1.
    pub pet_energy: f32,
    /// A password field has focus.
    pub privacy_mode: bool,
    /// Time of the last key press while in privacy mode (ms).
    pub privacy_typing_at: f64,
    /// The 108-key keyboard is shown.
    pub is_108: bool,
    /// How asleep she is: 0 = awake, 1 = asleep.
    pub sleep: f32,
}

/// Facial expression, one sprite each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expression {
    Idle,
    Blink,
    Shy,
    Happy,
}

/// One of the two hands.
#[derive(Clone, Debug)]
struct HandState {
    x: f32,
    y: f32,
    target_x: f32,
    target_y: f32,
    last_at: f64,
    squash: f32,
    held_vks: HashSet<u32>,
}

impl HandState {
    /// @param x Starting position x.
    /// @param y Starting position y.
    /// @return A resting hand.
    fn new(x: f32, y: f32) -> HandState {
        HandState {
            x,
            y,
            target_x: x,
            target_y: y,
            last_at: -10000.0,
            squash: 0.0,
            held_vks: HashSet::new(),
        }
    }
}

/// The whale tail.
#[derive(Clone, Debug)]
struct TailState {
    x: f32,
    y: f32,
    target_x: f32,
    target_y: f32,
    slap_at: f64,
    release_at: f64,
    slap_target_x: f32,
    slap_target_y: f32,
    held_vks: HashSet<u32>,
    squash: f32,
    /// 0 = idle wag, 1 = fully slapped down on the key.
    phase: f32,
}

/// A water ring where the tail hit the keyboard.
#[derive(Clone, Copy, Debug)]
struct WaterRipple {
    x: f32,
    y: f32,
    radius: f32,
    life: f32,
}

/// Kind of floating particle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParticleKind {
    Heart,
    Drop,
}

/// A small heart or water drop floating up.
#[derive(Clone, Copy, Debug)]
struct FloatingParticle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    kind: ParticleKind,
}

/// Where the character is drawn this frame.
#[derive(Clone, Copy, Debug)]
struct Geometry {
    char_center_x: f32,
    draw_w: f32,
    draw_h: f32,
    pivot_off: f32,
    pivot_y: f32,
    tilt: f32,
}

impl Geometry {
    /// Converts a point on the sprite (0..1 on both axes) into pet coordinates, following the
    /// body tilt.
    ///
    /// @param norm_x Horizontal position on the sprite, 0..1.
    /// @param norm_y Vertical position on the sprite, 0..1.
    /// @return The point in pet coordinates.
    fn sprite_point_to_world(&self, norm_x: f32, norm_y: f32) -> Vec2 {
        let local_x = (norm_x - 0.5) * self.draw_w;
        let local_y = norm_y * self.draw_h - self.draw_h + self.pivot_off;
        let cos = self.tilt.cos();
        let sin = self.tilt.sin();
        Vec2::new(
            self.char_center_x + local_x * cos - local_y * sin,
            self.pivot_y + local_x * sin + local_y * cos,
        )
    }
}

/// Decodes one embedded sprite.
///
/// @param bytes PNG file contents.
/// @return The image, or a 1×1 transparent image when decoding fails.
fn load_sprite(bytes: &[u8]) -> Pixmap {
    match Pixmap::decode_png(bytes) {
        Ok(image) => image,
        Err(_) => Pixmap::new(1, 1).expect("1x1 pixmap"),
    }
}

/// Clamps a value into a range.
///
/// @param value The value.
/// @param min Lower bound.
/// @param max Upper bound.
/// @return The clamped value.
fn clamp(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

/// The whale-girl character: sprites plus all of her animation state.
pub struct DaFeiYu {
    sprite_idle: Pixmap,
    sprite_blink: Pixmap,
    sprite_shy: Pixmap,
    sprite_happy: Pixmap,

    last_key_at: f64,
    key_times: Vec<f64>,
    typing_heat: f32,
    blush_level: f32,
    /// 0 = calm, 1 = shy blush, 2 = happy flush.
    mood_tier: u8,
    mood_changed_at: f64,
    next_blink_at: f64,
    blink_until: f64,

    body_shift_x: f32,
    body_shift_vel: f32,
    body_shift_target: f32,
    body_tilt: f32,
    body_tilt_vel: f32,
    body_tilt_target: f32,
    body_dive_y: f32,
    body_bob: f32,

    left_hand: HandState,
    right_hand: HandState,
    tail: TailState,

    ripples: Vec<WaterRipple>,
    particles: Vec<FloatingParticle>,
}

impl Default for DaFeiYu {
    /// @return A new character (same as [`DaFeiYu::new`]).
    fn default() -> DaFeiYu {
        DaFeiYu::new()
    }
}

impl DaFeiYu {
    /// Creates the character and decodes her sprites.
    ///
    /// @return The character in her resting pose.
    pub fn new() -> DaFeiYu {
        DaFeiYu {
            sprite_idle: load_sprite(include_bytes!("../../assets/dafeiyu/notail_idle.png")),
            sprite_blink: load_sprite(include_bytes!("../../assets/dafeiyu/notail_blink.png")),
            sprite_shy: load_sprite(include_bytes!("../../assets/dafeiyu/notail_shy.png")),
            sprite_happy: load_sprite(include_bytes!("../../assets/dafeiyu/notail_happy2.png")),

            last_key_at: -10000.0,
            key_times: Vec::new(),
            typing_heat: 0.0,
            blush_level: 0.0,
            mood_tier: 0,
            mood_changed_at: -10000.0,
            next_blink_at: now_ms() + 3600.0 + random() as f64 * 2600.0,
            blink_until: 0.0,

            body_shift_x: 0.0,
            body_shift_vel: 0.0,
            body_shift_target: 0.0,
            body_tilt: 0.0,
            body_tilt_vel: 0.0,
            body_tilt_target: 0.0,
            body_dive_y: 0.0,
            body_bob: 0.0,

            left_hand: HandState::new(155.0, 312.0),
            right_hand: HandState::new(235.0, 312.0),
            tail: TailState {
                x: 320.0,
                y: 280.0,
                target_x: 320.0,
                target_y: 280.0,
                slap_at: -10000.0,
                release_at: -10000.0,
                slap_target_x: 340.0,
                slap_target_y: 355.0,
                held_vks: HashSet::new(),
                squash: 0.0,
                phase: 0.0,
            },

            ripples: Vec::new(),
            particles: Vec::new(),
        }
    }

    /// Returns where the sleeping "Z" bubbles appear: above and to the right of her head.
    ///
    /// @param layout The scene layout.
    /// @return The point in logical pixels.
    pub fn sleep_z_anchor(&self, layout: &DaFeiYuLayout) -> Vec2 {
        Vec2::new(character_x(layout) + self.body_shift_x + 70.0, 84.0)
    }

    /// Called when password privacy mode starts or ends. Starting it lets go of all keys.
    ///
    /// @param active True when privacy mode starts.
    pub fn on_privacy_mode_changed(&mut self, active: bool) {
        if active {
            self.left_hand.held_vks.clear();
            self.right_hand.held_vks.clear();
            self.tail.held_vks.clear();
            self.ripples.clear();
        }
    }

    /// Called on every key press while in privacy mode: a small shy bounce.
    ///
    /// @param _now Current time in ms (unused; kept for a uniform event API).
    pub fn on_privacy_typing(&mut self, _now: f64) {
        self.body_bob = (self.body_bob + 0.42).min(0.65);
    }

    /// Makes the left hand go straight back to the mouse instead of finishing a key press.
    pub fn reset_left_hand_for_mouse(&mut self) {
        self.left_hand.last_at = -10000.0;
    }

    /// Handles a key press: picks the tail (right-edge keys), the right hand (while the left
    /// hand is on the mouse) or the nearer hand.
    ///
    /// @param vk The virtual-key code.
    /// @param key_point Centre of the pressed key in pet coordinates.
    /// @param layout The scene layout.
    /// @param repeat True for auto-repeat presses (ignored).
    /// @param now Current time in ms.
    pub fn on_key_down(&mut self, vk: u32, key_point: Vec2, layout: &DaFeiYuLayout, repeat: bool, now: f64) {
        if layout.privacy_mode || repeat {
            return;
        }
        self.last_key_at = now;
        self.key_times.push(now);
        self.body_bob = (self.body_bob + 0.72).min(1.1);

        let char_x = character_x(layout);
        let norm_x = clamp((key_point.x - layout.kb_center_x) / (layout.kb_width * 0.46), -1.0, 1.0);
        let mouse_busy = is_mouse_busy(layout, now);
        let is_left_side = key_point.x < layout.kb_center_x - 2.0;
        let edge_share = if layout.is_108 { 0.16 } else { 0.215 };
        let right_edge_threshold = layout.kb_center_x + layout.kb_width * edge_share;

        if key_point.x >= right_edge_threshold {
            // Right-edge keys (actions, arrows, numpad) get a whale tail slap.
            self.tail.slap_at = now;
            self.tail.release_at = -10000.0;
            self.tail.held_vks.insert(vk);
            self.tail.slap_target_x = key_point.x;
            self.tail.slap_target_y = key_point.y;
            self.tail.squash = 1.0;
            if !mouse_busy {
                self.body_tilt_target = -0.075;
                self.body_shift_target = -7.0;
            }
            self.ripples.push(WaterRipple { x: key_point.x, y: key_point.y, radius: 6.0, life: 1.0 });
            self.ripples.push(WaterRipple { x: key_point.x, y: key_point.y, radius: 2.0, life: 0.85 });
            self.spawn_particles(key_point.x, key_point.y - 8.0, 3, true);
        } else if mouse_busy {
            // Mouse and keyboard at once: the left hand stays on the mouse, the right hand types.
            let shift_base = if layout.is_108 { -88.0 } else { -38.0 };
            let current_char_x = char_x + shift_base;
            let comfort_x = clamp(key_point.x, current_char_x - 10.0, current_char_x + 92.0);
            let comfort_y = (layout.kb_center_y + 6.0).min(key_point.y);
            press_hand(&mut self.right_hand, comfort_x, comfort_y, vk, now);
        } else {
            // Two-handed typing with a gentle sway.
            self.body_tilt_target = norm_x * 0.095;
            let shift_range = if layout.is_108 { 22.0 } else { 13.0 };
            self.body_shift_target = norm_x * shift_range;
            let comfort_x = char_x + self.body_shift_target + clamp((key_point.x - char_x) * 0.78, -76.0, 76.0);
            let comfort_y = (layout.kb_center_y + 10.0).min(key_point.y);
            if is_left_side {
                press_hand(&mut self.left_hand, comfort_x, comfort_y, vk, now);
            } else {
                press_hand(&mut self.right_hand, comfort_x, comfort_y, vk, now);
            }
        }
    }

    /// Handles a key release.
    ///
    /// @param vk The virtual-key code.
    pub fn on_key_up(&mut self, vk: u32) {
        let now = now_ms();
        self.left_hand.held_vks.remove(&vk);
        self.right_hand.held_vks.remove(&vk);
        if self.tail.held_vks.remove(&vk) && self.tail.held_vks.is_empty() {
            self.tail.release_at = now;
        }
    }

    /// Adds floating hearts or water drops.
    ///
    /// @param x Centre x.
    /// @param y Centre y.
    /// @param count How many.
    /// @param only_drop True for water drops only; otherwise a mix of hearts and drops.
    fn spawn_particles(&mut self, x: f32, y: f32, count: u32, only_drop: bool) {
        for _ in 0..count {
            let spread_x = if only_drop { 28.0 } else { 48.0 };
            let speed_x = if only_drop { 2.2 } else { 0.8 };
            let new_x = x + (random() - 0.5) * spread_x;
            let new_y = y + (random() - 0.5) * 16.0;
            let vx = (random() - 0.5) * speed_x;
            let vy = -1.1 - random() * 1.2;
            let kind = if only_drop {
                ParticleKind::Drop
            } else if random() > 0.45 {
                ParticleKind::Heart
            } else {
                ParticleKind::Drop
            };
            self.particles.push(FloatingParticle { x: new_x, y: new_y, vx, vy, life: 1.0, kind });
        }
    }

    /// Tells whether she is typing right now.
    ///
    /// @param now Current time in ms.
    /// @param layout The scene layout.
    /// @return True while keys are held or were pressed in the last 550 ms.
    fn is_typing(&self, now: f64, layout: &DaFeiYuLayout) -> bool {
        !layout.privacy_mode
            && (now - self.last_key_at < 550.0
                || !self.tail.held_vks.is_empty()
                || !self.left_hand.held_vks.is_empty()
                || !self.right_hand.held_vks.is_empty())
    }

    /// Advances the animation by one frame: mood, blush, blinking, body springs, ripples,
    /// particles, tail and hands.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in ms.
    /// @param layout The scene layout.
    pub fn update(&mut self, dt: f32, now: f64, layout: &DaFeiYuLayout) {
        self.body_bob *= (-12.0 * dt).exp();

        let is_happy = is_happy(layout);
        let is_typing = self.is_typing(now, layout);
        let mouse_mode = is_mouse_mode(layout, now);

        // Typing speed over a 2.8 s window drives the blush. The mood tier only changes after
        // a minimum dwell time so it never flickers.
        while let Some(first) = self.key_times.first() {
            if now - *first > 2800.0 {
                self.key_times.remove(0);
            } else {
                break;
            }
        }
        let typing_cps = self.key_times.len() as f32 / 2.8;
        let heat_target = if layout.privacy_mode {
            0.0
        } else {
            clamp((typing_cps - 2.4) / 5.4, 0.0, 1.0)
        };
        let heat_rate = if heat_target > self.typing_heat { 2.2 } else { 0.65 };
        self.typing_heat += (heat_target - self.typing_heat) * (1.0 - (-heat_rate * dt).exp());

        let dwell = now - self.mood_changed_at;
        match self.mood_tier {
            0 => {
                if self.typing_heat >= 0.40 && dwell >= 1400.0 {
                    self.mood_tier = 1;
                    self.mood_changed_at = now;
                }
            }
            1 => {
                if self.typing_heat >= 0.78 && dwell >= 1800.0 {
                    self.mood_tier = 2;
                    self.mood_changed_at = now;
                } else if self.typing_heat < 0.22 && dwell >= 2200.0 {
                    self.mood_tier = 0;
                    self.mood_changed_at = now;
                }
            }
            _ => {
                if self.typing_heat < 0.55 && dwell >= 2200.0 {
                    self.mood_tier = 1;
                    self.mood_changed_at = now;
                }
            }
        }

        let mood_blush = match self.mood_tier {
            1 => 0.55,
            2 => 0.85,
            _ => 0.0,
        };
        let happy_blush = if is_happy { 0.45 } else { 0.0 };
        let privacy_blush = if layout.privacy_mode { 0.46 } else { 0.0 };
        let target_blush = (self.typing_heat * 0.92).max(happy_blush).max(privacy_blush).max(mood_blush);
        self.blush_level += (target_blush - self.blush_level) * (1.0 - (-3.2 * dt).exp());

        // Calm blinking every 4.5 to 7.2 seconds.
        if now >= self.next_blink_at {
            self.blink_until = now + 135.0;
            self.next_blink_at = now + 4500.0 + random() as f64 * 2700.0;
        }

        let char_x = character_x(layout);
        if is_happy && random() < 0.06 {
            self.spawn_particles(char_x + self.body_shift_x, 120.0, 1, false);
        } else if !layout.privacy_mode && self.mood_tier == 2 && random() < 0.025 {
            self.spawn_particles(char_x + self.body_shift_x, 124.0, 1, false);
        }

        // Body spring physics.
        if layout.privacy_mode {
            self.body_shift_target = 0.0;
            let mut shy_sway = (now * 0.0042).sin() as f32 * 0.032;
            if now - layout.privacy_typing_at < 240.0 {
                shy_sway += (now * 0.04).sin() as f32 * 0.024;
            }
            self.body_tilt_target = shy_sway;
            self.body_dive_y *= (-9.0 * dt).exp();
        } else if mouse_mode {
            let base_shift = if layout.is_108 { -125.0 } else { -38.0 };
            self.body_shift_target = base_shift + (layout.mouse_nx - 0.5) * 12.0;
            self.body_tilt_target = -0.10;
            self.body_dive_y += (8.0 - self.body_dive_y) * (1.0 - (-10.0 * dt).exp());
        } else if !is_typing {
            self.body_shift_target = 0.0;
            self.body_tilt_target = 0.0;
            self.body_dive_y *= (-9.0 * dt).exp();
        } else {
            self.body_dive_y *= (-9.0 * dt).exp();
        }

        let shift_acceleration = (self.body_shift_target - self.body_shift_x) * 160.0 - self.body_shift_vel * 18.0;
        self.body_shift_vel += shift_acceleration * dt;
        self.body_shift_x += self.body_shift_vel * dt;

        let tilt_acceleration = (self.body_tilt_target - self.body_tilt) * 180.0 - self.body_tilt_vel * 17.0;
        self.body_tilt_vel += tilt_acceleration * dt;
        self.body_tilt += self.body_tilt_vel * dt;

        // Ripples and particles. Particle speeds were tuned per 60 fps frame.
        let frames = dt * 60.0;
        for ripple in self.ripples.iter_mut() {
            ripple.radius += 58.0 * dt;
            ripple.life -= 2.2 * dt;
        }
        self.ripples.retain(|ripple| ripple.life > 0.0);
        for particle in self.particles.iter_mut() {
            particle.x += particle.vx * frames;
            particle.y += particle.vy * frames;
            particle.life -= 0.022 * frames;
        }
        self.particles.retain(|particle| particle.life > 0.0);

        self.update_limbs(dt, now, layout, is_happy, is_typing, mouse_mode);
    }

    /// Moves the tail and both hands towards their targets.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in ms.
    /// @param layout The scene layout.
    /// @param is_happy She is being petted.
    /// @param is_typing She is typing.
    /// @param mouse_mode Her left hand is on the mouse.
    fn update_limbs(
        &mut self,
        dt: f32,
        now: f64,
        layout: &DaFeiYuLayout,
        is_happy: bool,
        is_typing: bool,
        mouse_mode: bool,
    ) {
        let geometry = self.geometry(now, layout);
        let shoulder_left = geometry.sprite_point_to_world(0.39, 0.653);
        let shoulder_right = geometry.sprite_point_to_world(0.61, 0.653);
        let tail_root = geometry.sprite_point_to_world(0.68, 0.822);

        // Tail: idle wag, or slapping down onto the held key.
        let wag_speed = self.wag_speed(is_happy, is_typing);
        let idle_tip_x = tail_root.x + 76.0 + (now * wag_speed).sin() as f32 * 10.0;
        let idle_tip_y = tail_root.y - 34.0 + (now * wag_speed * 1.15).cos() as f32 * 8.0;

        let tail_age = (now - self.tail.slap_at).max(0.0);
        let holding_slap = !layout.privacy_mode
            && (!self.tail.held_vks.is_empty() || (self.tail.slap_at > 0.0 && tail_age < 260.0));
        let mut target_phase: f32 = 0.0;
        if holding_slap {
            target_phase = (tail_age / 65.0).min(1.0) as f32;
        } else if !layout.privacy_mode && self.tail.slap_at > 0.0 {
            let hold_end = (self.tail.slap_at + 260.0).max(self.tail.release_at);
            let return_elapsed = (now - hold_end).max(0.0);
            target_phase = (1.0 - return_elapsed / 220.0).max(0.0) as f32;
        }
        self.tail.phase += (target_phase - self.tail.phase) * (1.0 - (-34.0 * dt).exp());
        self.tail.target_x = idle_tip_x + (self.tail.slap_target_x - idle_tip_x) * self.tail.phase;
        self.tail.target_y = idle_tip_y + (self.tail.slap_target_y - idle_tip_y) * self.tail.phase;
        let tail_lerp = 1.0 - (-36.0 * dt).exp();
        self.tail.x += (self.tail.target_x - self.tail.x) * tail_lerp;
        self.tail.y += (self.tail.target_y - self.tail.y) * tail_lerp;
        self.tail.squash *= (-11.0 * dt).exp();

        // Hands: over the eyes in privacy mode, cheering while petted, otherwise typing,
        // on the mouse or resting.
        let left_typing = !layout.privacy_mode
            && !mouse_mode
            && (now - self.left_hand.last_at < 380.0 || !self.left_hand.held_vks.is_empty());
        let right_typing = !layout.privacy_mode
            && (now - self.right_hand.last_at < 380.0 || !self.right_hand.held_vks.is_empty());

        let left_target: Vec2;
        let right_target: Vec2;
        if layout.privacy_mode {
            let eye_left = geometry.sprite_point_to_world(0.390, 0.476);
            let eye_right = geometry.sprite_point_to_world(0.598, 0.476);
            let shy_bob = (now * 0.008).sin() as f32 * 1.4;
            left_target = Vec2::new(eye_left.x, eye_left.y + shy_bob);
            right_target = Vec2::new(eye_right.x, eye_right.y - shy_bob);
        } else if is_happy {
            let cheer = (now * 0.016).sin() as f32 * 5.0;
            left_target = Vec2::new(shoulder_left.x - 14.0, shoulder_left.y + 4.0 + cheer);
            right_target = Vec2::new(shoulder_right.x + 14.0, shoulder_right.y + 4.0 - cheer);
        } else {
            if mouse_mode {
                let press_offset = if layout.mouse_left || layout.mouse_right { 2.5 } else { 0.0 };
                left_target = Vec2::new(layout.mouse_btn_x, layout.mouse_btn_y + press_offset);
            } else if left_typing {
                left_target = Vec2::new(self.left_hand.target_x, self.left_hand.target_y);
            } else {
                left_target = Vec2::new(shoulder_left.x - 15.0, shoulder_left.y + 34.0);
            }
            if right_typing {
                right_target = Vec2::new(self.right_hand.target_x, self.right_hand.target_y);
            } else {
                right_target = Vec2::new(shoulder_right.x + 15.0, shoulder_right.y + 34.0);
            }
        }

        let hand_lerp = 1.0 - (-26.0 * dt).exp();
        self.left_hand.x += (left_target.x - self.left_hand.x) * hand_lerp;
        self.left_hand.y += (left_target.y - self.left_hand.y) * hand_lerp;
        self.right_hand.x += (right_target.x - self.right_hand.x) * hand_lerp;
        self.right_hand.y += (right_target.y - self.right_hand.y) * hand_lerp;
        self.left_hand.squash *= (-14.0 * dt).exp();
        self.right_hand.squash *= (-14.0 * dt).exp();
    }

    /// Returns how fast the tail wags.
    ///
    /// @param is_happy She is being petted.
    /// @param is_typing She is typing.
    /// @return Wag speed in radians per ms.
    fn wag_speed(&self, is_happy: bool, is_typing: bool) -> f64 {
        if is_happy {
            0.015
        } else if self.typing_heat > 0.45 {
            0.014
        } else if is_typing {
            0.0082
        } else {
            0.0042
        }
    }

    /// Computes where the character is drawn this frame.
    ///
    /// @param now Current time in ms.
    /// @param layout The scene layout.
    /// @return The geometry.
    fn geometry(&self, now: f64, layout: &DaFeiYuLayout) -> Geometry {
        let char_x = character_x(layout);
        let base_char_y = if layout.is_108 { 366.0 } else { 364.0 };
        let breath = (now * 0.0034).sin() as f32 * 2.2 + self.body_bob * 3.8 + self.body_dive_y;
        let char_base_y = base_char_y + breath;
        let draw_w = if layout.is_108 { 268.0 } else { 262.0 };
        let draw_h = draw_w * (278.0 / 224.0);
        let pivot_off = 48.0;
        Geometry {
            char_center_x: char_x + self.body_shift_x,
            draw_w,
            draw_h,
            pivot_off,
            pivot_y: char_base_y - pivot_off,
            tilt: self.body_tilt,
        }
    }

    /// Draws layer 1 (before the keyboard and mouse): the sprite and the soft cheek blush.
    ///
    /// @param ctx The canvas.
    /// @param now Current time in ms.
    /// @param layout The scene layout.
    pub fn render_body(&self, ctx: &mut Canvas, now: f64, layout: &DaFeiYuLayout) {
        let geometry = self.geometry(now, layout);
        let is_happy = is_happy(layout);
        let is_blinking = now < self.blink_until;

        let expression = if layout.privacy_mode || layout.sleep > 0.55 {
            Expression::Blink
        } else if is_happy || self.mood_tier == 2 {
            Expression::Happy
        } else if self.mood_tier == 1 {
            Expression::Shy
        } else if is_blinking {
            Expression::Blink
        } else {
            Expression::Idle
        };
        let image = match expression {
            Expression::Idle => &self.sprite_idle,
            Expression::Blink => &self.sprite_blink,
            Expression::Shy => &self.sprite_shy,
            Expression::Happy => &self.sprite_happy,
        };

        ctx.save();
        ctx.begin_path();
        ctx.rect(0.0, 0.0, layout.logical_w, layout.kb_center_y - layout.kb_height * 0.36);
        ctx.clip();
        ctx.translate(geometry.char_center_x, geometry.pivot_y);
        ctx.rotate(self.body_tilt);
        ctx.scale(1.0 + self.body_bob * 0.022, 1.0 - self.body_bob * 0.018);
        ctx.draw_image(
            image,
            -geometry.draw_w * 0.5,
            -geometry.draw_h + geometry.pivot_off,
            geometry.draw_w,
            geometry.draw_h,
        );

        // Soft watercolour blush just below the eyes.
        if self.blush_level > 0.08 {
            let cheek_y = 0.562 * geometry.draw_h - geometry.draw_h + geometry.pivot_off;
            let cheeks = [
                Vec2::new((0.372 - 0.5) * geometry.draw_w, cheek_y),
                Vec2::new((0.616 - 0.5) * geometry.draw_w, cheek_y),
            ];
            for cheek in cheeks {
                self.draw_cheek_blush(ctx, cheek);
            }
        }
        ctx.restore();
    }

    /// Draws the blush on one cheek, plus little shy lines when the blush is strong.
    ///
    /// @param ctx The canvas (already in sprite space).
    /// @param cheek Centre of the cheek.
    fn draw_cheek_blush(&self, ctx: &mut Canvas, cheek: Vec2) {
        let radius_x = 11.5 + self.blush_level * 4.5;
        let radius_y = 6.2 + self.blush_level * 2.2;
        let mut gradient = Gradient::radial(cheek.x, cheek.y, 1.0, cheek.x, cheek.y, radius_x);
        gradient.add_color_stop(0.0, &format!("rgba(255, 110, 148, {:.3})", self.blush_level * 0.46));
        gradient.add_color_stop(0.55, &format!("rgba(255, 138, 170, {:.3})", self.blush_level * 0.22));
        gradient.add_color_stop(1.0, "rgba(255, 140, 170, 0)");
        ctx.set_fill_gradient(gradient);
        ctx.begin_path();
        ctx.ellipse(cheek.x, cheek.y, radius_x, radius_y, 0.0, 0.0, PI * 2.0, false);
        ctx.fill();

        if self.blush_level > 0.52 {
            let line_alpha = ((self.blush_level - 0.52) / 0.40).min(1.0) * 0.36;
            ctx.set_stroke_color(&format!("rgba(235, 88, 132, {:.3})", line_alpha));
            ctx.set_line_width(1.05);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            for offset_x in [-3.4, 0.0, 3.4] {
                ctx.move_to(cheek.x + offset_x - 0.9, cheek.y + 1.8);
                ctx.line_to(cheek.x + offset_x + 1.0, cheek.y - 1.5);
            }
            ctx.stroke();
        }
    }

    /// Draws layer 2 (after the keyboard and mouse): water ripples, the whale tail, the hands
    /// and the floating particles.
    ///
    /// @param ctx The canvas.
    /// @param _dt Frame time in seconds (unused; the motion is advanced in [`DaFeiYu::update`]).
    /// @param now Current time in ms.
    /// @param layout The scene layout.
    pub fn render_foreground(&self, ctx: &mut Canvas, _dt: f32, now: f64, layout: &DaFeiYuLayout) {
        let geometry = self.geometry(now, layout);
        let shoulder_left = geometry.sprite_point_to_world(0.39, 0.653);
        let shoulder_right = geometry.sprite_point_to_world(0.61, 0.653);
        let tail_root = geometry.sprite_point_to_world(0.68, 0.822);
        let is_happy = is_happy(layout);
        let is_typing = self.is_typing(now, layout);
        let mouse_mode = is_mouse_mode(layout, now);

        // Water rings where the tail hit the keyboard.
        for ripple in &self.ripples {
            ctx.save();
            ctx.set_global_alpha(ripple.life * 0.82);
            ctx.set_stroke_color("#67d8ff");
            ctx.set_line_width(2.4);
            ctx.begin_path();
            ctx.ellipse(ripple.x, ripple.y, ripple.radius, ripple.radius * 0.55, 0.0, 0.0, PI * 2.0, false);
            ctx.stroke();
            ctx.restore();
        }

        // Whale tail.
        let wag_speed = self.wag_speed(is_happy, is_typing);
        let wag_amount = if is_happy { 0.34 } else { 0.16 + self.typing_heat * 0.16 };
        let idle_wag = (now * wag_speed).sin() as f32 * wag_amount;
        let active_wag = idle_wag * (1.0 - self.tail.phase) + -0.08 * self.tail.phase;
        self.draw_whale_tail(
            ctx,
            tail_root,
            Vec2::new(self.tail.x, self.tail.y),
            self.tail.phase,
            active_wag,
            layout.logical_w,
        );

        // Round chibi hands.
        let max_mouse_reach = if layout.is_108 { 96.0 } else { 88.0 };
        let left_reach = if mouse_mode { max_mouse_reach } else { 72.0 };
        let right_reach = if mouse_mode { 86.0 } else { 72.0 };
        draw_chubby_arm(
            ctx,
            shoulder_left,
            Vec2::new(self.left_hand.x, self.left_hand.y),
            true,
            self.left_hand.squash,
            left_reach,
        );
        draw_chubby_arm(
            ctx,
            shoulder_right,
            Vec2::new(self.right_hand.x, self.right_hand.y),
            false,
            self.right_hand.squash,
            right_reach,
        );

        // Floating hearts and water drops.
        for particle in &self.particles {
            ctx.save();
            ctx.set_global_alpha(particle.life.max(0.0));
            // The original drew 14px emoji at a text baseline; their visual centre sits about
            // (8, -5) from the baseline origin.
            let center = Vec2::new(particle.x + 8.0, particle.y - 5.0);
            match particle.kind {
                ParticleKind::Heart => draw_heart(ctx, center),
                ParticleKind::Drop => draw_drop(ctx, center),
            }
            ctx.restore();
        }
    }

    /// Draws the whale tail: a tapered stem along a curve and the fluke at its tip.
    ///
    /// @param ctx The canvas.
    /// @param root Where the tail leaves the body.
    /// @param tip Where the fluke is.
    /// @param slap_phase 0 = idle, 1 = slapped down.
    /// @param wag_angle Extra fluke rotation in radians.
    /// @param logical_w Width of the pet area (keeps the curve on screen).
    fn draw_whale_tail(
        &self,
        ctx: &mut Canvas,
        root: Vec2,
        tip: Vec2,
        slap_phase: f32,
        wag_angle: f32,
        logical_w: f32,
    ) {
        let idle_control_x = root.x + 48.0;
        let idle_control_y = root.y - 2.0;
        let slap_control_x = (logical_w - 18.0).min((root.x + 94.0).max(tip.x + 42.0));
        let slap_control_y = (root.y - 28.0).min((root.y + tip.y) * 0.5 - 50.0);
        let control_x = idle_control_x + (slap_control_x - idle_control_x) * slap_phase;
        let control_y = idle_control_y + (slap_control_y - idle_control_y) * slap_phase;

        // Sample the stem: position, normal and half-width at each step.
        struct StemPoint {
            x: f32,
            y: f32,
            nx: f32,
            ny: f32,
            r: f32,
        }
        let steps = 14;
        let mut points: Vec<StemPoint> = Vec::with_capacity(steps + 1);
        for index in 0..=steps {
            let t = (index as f32 / steps as f32) * 0.90;
            let one_minus_t = 1.0 - t;
            let x = one_minus_t * one_minus_t * root.x + 2.0 * one_minus_t * t * control_x + t * t * tip.x;
            let y = one_minus_t * one_minus_t * root.y + 2.0 * one_minus_t * t * control_y + t * t * tip.y;
            let tangent_x = 2.0 * one_minus_t * (control_x - root.x) + 2.0 * t * (tip.x - control_x);
            let tangent_y = 2.0 * one_minus_t * (control_y - root.y) + 2.0 * t * (tip.y - control_y);
            let length = tangent_x.hypot(tangent_y).max(0.001);
            let r = if t < 0.14 {
                7.2 + (t / 0.14) * 2.8
            } else {
                10.0 * (1.0 - ((t - 0.14) / 0.86) * 0.56)
            };
            points.push(StemPoint { x, y, nx: -tangent_y / length, ny: tangent_x / length, r });
        }

        let tip_angle = (tip.y - control_y).atan2(tip.x - control_x) + wag_angle;

        ctx.save();
        ctx.set_line_cap(LineCap::Round);
        ctx.set_line_join(LineJoin::Round);

        if slap_phase > 0.25 {
            ctx.save();
            ctx.set_shadow_color("rgba(7, 16, 40, 0.45)");
            ctx.set_shadow_blur(10.0);
            ctx.set_shadow_offset_y(5.0);
            ctx.set_fill_color("rgba(20, 40, 88, 0.25)");
            ctx.begin_path();
            ctx.ellipse(tip.x, tip.y + 4.0, 20.0, 12.0, 0.0, 0.0, PI * 2.0, false);
            ctx.fill();
            ctx.restore();
        }

        let mut stem_gradient = Gradient::linear(root.x, root.y, tip.x, tip.y);
        stem_gradient.add_color_stop(0.0, "rgba(24, 34, 68, 0.92)");
        stem_gradient.add_color_stop(0.18, "#1d3269");
        stem_gradient.add_color_stop(0.55, "#2957a4");
        stem_gradient.add_color_stop(1.0, "#52b2eb");

        ctx.begin_path();
        ctx.move_to(points[0].x + points[0].nx * points[0].r, points[0].y + points[0].ny * points[0].r);
        for point in points.iter().skip(1) {
            ctx.line_to(point.x + point.nx * point.r, point.y + point.ny * point.r);
        }
        for point in points.iter().rev() {
            ctx.line_to(point.x - point.nx * point.r, point.y - point.ny * point.r);
        }
        ctx.close_path();
        ctx.set_fill_gradient(stem_gradient);
        ctx.fill();

        // Outline along both edges (the root end stays open).
        ctx.set_stroke_color("#16203d");
        ctx.set_line_width(2.4);
        for side in [1.0_f32, -1.0_f32] {
            ctx.begin_path();
            for (index, point) in points.iter().enumerate().skip(1) {
                let edge_x = point.x + side * point.nx * point.r;
                let edge_y = point.y + side * point.ny * point.r;
                if index == 1 {
                    ctx.move_to(edge_x, edge_y);
                } else {
                    ctx.line_to(edge_x, edge_y);
                }
            }
            ctx.stroke();
        }

        // Glossy highlight along the top of the stem.
        ctx.set_stroke_color("rgba(170, 232, 255, 0.48)");
        ctx.set_line_width(3.0);
        ctx.begin_path();
        for (index, point) in points.iter().enumerate().skip(2) {
            let highlight_x = point.x + point.nx * (point.r * 0.35);
            let highlight_y = point.y + point.ny * (point.r * 0.35);
            if index == 2 {
                ctx.move_to(highlight_x, highlight_y);
            } else {
                ctx.line_to(highlight_x, highlight_y);
            }
        }
        ctx.stroke();

        // Fluke.
        ctx.save();
        ctx.translate(tip.x, tip.y);
        ctx.rotate(tip_angle);
        let squash = self.tail.squash;
        let scale = 1.05 + slap_phase * 0.10;
        ctx.scale(scale * (1.0 + squash * 0.16), scale * (1.0 - squash * 0.10));

        let mut fluke_gradient = Gradient::radial(-6.0, 0.0, 2.0, 4.0, 0.0, 28.0);
        fluke_gradient.add_color_stop(0.0, "#96ecff");
        fluke_gradient.add_color_stop(0.38, "#4ba8e6");
        fluke_gradient.add_color_stop(0.78, "#26539e");
        fluke_gradient.add_color_stop(1.0, "#182f68");

        ctx.set_fill_gradient(fluke_gradient);
        ctx.set_stroke_color("#151f3b");
        ctx.set_line_width(2.4);
        ctx.begin_path();
        ctx.move_to(-13.0, -4.0);
        ctx.bezier_curve_to(-8.0, -19.0, 8.0, -30.0, 19.0, -23.0);
        ctx.bezier_curve_to(22.0, -14.0, 14.0, -4.0, 3.0, 0.0);
        ctx.bezier_curve_to(14.0, 4.0, 22.0, 14.0, 19.0, 23.0);
        ctx.bezier_curve_to(8.0, 30.0, -8.0, 19.0, -13.0, 4.0);
        ctx.close_path();
        ctx.fill();
        ctx.stroke();

        ctx.set_stroke_color("rgba(215, 248, 255, 0.65)");
        ctx.set_line_width(1.6);
        ctx.begin_path();
        ctx.move_to(-8.0, -2.0);
        ctx.quadratic_curve_to(2.0, -11.0, 13.0, -17.0);
        ctx.move_to(-8.0, 2.0);
        ctx.quadratic_curve_to(2.0, 11.0, 13.0, 17.0);
        ctx.move_to(-6.0, -1.0);
        ctx.quadratic_curve_to(1.0, -6.0, 8.0, -9.0);
        ctx.move_to(-6.0, 1.0);
        ctx.quadratic_curve_to(1.0, 6.0, 8.0, 9.0);
        ctx.stroke();

        ctx.set_fill_color("rgba(230, 252, 255, 0.55)");
        ctx.begin_path();
        ctx.ellipse(-4.0, -7.0, 4.2, 2.2, -0.45, 0.0, PI * 2.0, false);
        ctx.fill();

        ctx.restore();
        ctx.restore();
    }
}

/// Moves a hand onto a key.
///
/// @param hand The hand.
/// @param x Target x.
/// @param y Target y.
/// @param vk The pressed key.
/// @param now Current time in ms.
fn press_hand(hand: &mut HandState, x: f32, y: f32, vk: u32, now: f64) {
    hand.target_x = x;
    hand.target_y = y;
    hand.last_at = now;
    hand.squash = 1.0;
    hand.held_vks.insert(vk);
}

/// Horizontal resting position of the character.
///
/// @param layout The scene layout.
/// @return The x position.
fn character_x(layout: &DaFeiYuLayout) -> f32 {
    if layout.is_108 {
        268.0
    } else {
        192.0
    }
}

/// Tells whether she is being petted.
///
/// @param layout The scene layout.
/// @return True when petted and not in privacy mode.
fn is_happy(layout: &DaFeiYuLayout) -> bool {
    !layout.privacy_mode && layout.pet_energy > 0.25
}

/// Tells whether the mouse is in use (a button held or moved in the last 900 ms).
///
/// @param layout The scene layout.
/// @param now Current time in ms.
/// @return True while the mouse is busy.
fn is_mouse_busy(layout: &DaFeiYuLayout, now: f64) -> bool {
    layout.mouse_left || layout.mouse_right || now - layout.mouse_active_at < 900.0
}

/// Tells whether her left hand should be on the mouse.
///
/// @param layout The scene layout.
/// @param now Current time in ms.
/// @return True in mouse mode.
fn is_mouse_mode(layout: &DaFeiYuLayout, now: f64) -> bool {
    !layout.privacy_mode && !is_happy(layout) && is_mouse_busy(layout, now)
}

/// Draws one arm: a puffy maid sleeve with a lace cuff and a round hand.
///
/// @param ctx The canvas.
/// @param shoulder Where the arm starts.
/// @param target Where the hand wants to be.
/// @param is_left True for the left arm (the sleeve bulges the other way).
/// @param squash Press squash of the hand, 0..1.
/// @param max_length Longest the arm can reach.
fn draw_chubby_arm(ctx: &mut Canvas, shoulder: Vec2, target: Vec2, is_left: bool, squash: f32, max_length: f32) {
    let sx = shoulder.x;
    let sy = shoulder.y;
    let dx = target.x - sx;
    let dy = target.y - sy;
    let raw_length = dx.hypot(dy).max(1.0);
    let length = raw_length.min(max_length);
    let ux = dx / raw_length;
    let uy = dy / raw_length;
    let hand_x = sx + ux * length;
    let hand_y = sy + uy * length;
    let nx = -uy;
    let ny = ux;
    let angle = dy.atan2(dx);
    let bulge_sign = if is_left { -1.0 } else { 1.0 };

    let wrist_distance = (length - 9.0).max(14.0);
    let wx = sx + ux * wrist_distance;
    let wy = sy + uy * wrist_distance;
    let shoulder_radius = 9.2;
    let cuff_radius = 12.0;
    let mid_x = (sx + wx) * 0.5 + nx * (bulge_sign * 2.5);
    let mid_y = (sy + wy) * 0.5 + ny * (bulge_sign * 2.5);

    ctx.save();

    // Sleeve.
    let mut gradient = Gradient::linear(sx, sy, wx, wy);
    gradient.add_color_stop(0.0, "#1f223b");
    gradient.add_color_stop(0.65, "#272b4b");
    gradient.add_color_stop(1.0, "#31365e");

    ctx.begin_path();
    ctx.move_to(sx + nx * shoulder_radius, sy + ny * shoulder_radius);
    ctx.quadratic_curve_to(mid_x + nx * 11.0, mid_y + ny * 11.0, wx + nx * cuff_radius, wy + ny * cuff_radius);
    ctx.line_to(wx - nx * cuff_radius, wy - ny * cuff_radius);
    ctx.quadratic_curve_to(mid_x - nx * 11.0, mid_y - ny * 11.0, sx - nx * shoulder_radius, sy - ny * shoulder_radius);
    ctx.close_path();
    ctx.set_fill_gradient(gradient);
    ctx.fill();

    ctx.begin_path();
    ctx.arc(sx, sy, shoulder_radius, 0.0, PI * 2.0, false);
    ctx.fill();

    ctx.set_stroke_color("#181a2e");
    ctx.set_line_width(2.4);
    ctx.set_line_cap(LineCap::Round);
    ctx.begin_path();
    ctx.move_to(sx + nx * shoulder_radius, sy + ny * shoulder_radius);
    ctx.quadratic_curve_to(mid_x + nx * 11.0, mid_y + ny * 11.0, wx + nx * cuff_radius, wy + ny * cuff_radius);
    ctx.move_to(sx - nx * shoulder_radius, sy - ny * shoulder_radius);
    ctx.quadratic_curve_to(mid_x - nx * 11.0, mid_y - ny * 11.0, wx - nx * cuff_radius, wy - ny * cuff_radius);
    ctx.stroke();

    // Trim line with a small gold button.
    let trim_distance = (wrist_distance - 5.5).max(8.0);
    ctx.save();
    ctx.translate(sx + ux * trim_distance, sy + uy * trim_distance);
    ctx.rotate(angle + PI / 2.0);
    ctx.set_stroke_color("rgba(205, 212, 232, 0.85)");
    ctx.set_line_width(1.6);
    ctx.begin_path();
    ctx.move_to(-cuff_radius + 2.0, 0.0);
    ctx.line_to(cuff_radius - 2.0, 0.0);
    ctx.stroke();
    ctx.set_fill_color("#e7c67b");
    ctx.begin_path();
    ctx.arc(0.0, 0.0, 1.5, 0.0, PI * 2.0, false);
    ctx.fill();
    ctx.restore();

    // Lace cuff.
    ctx.save();
    ctx.translate(wx, wy);
    ctx.rotate(angle + PI / 2.0);
    ctx.set_fill_color("#f8f9fe");
    ctx.set_stroke_color("#252844");
    ctx.set_line_width(1.8);
    for index in -2..=2 {
        ctx.begin_path();
        ctx.arc(index as f32 * 4.4, 0.0, 4.7, 0.0, PI * 2.0, false);
        ctx.fill();
        ctx.stroke();
    }
    ctx.begin_path();
    ctx.ellipse(0.0, -0.5, 10.2, 3.6, 0.0, 0.0, PI * 2.0, false);
    ctx.fill();
    ctx.restore();

    // Round hand (no thumb).
    ctx.save();
    ctx.translate(hand_x, hand_y);
    ctx.rotate(angle);
    ctx.set_fill_color("#fde7df");
    ctx.set_stroke_color("#3d2c35");
    ctx.set_line_width(2.0);
    ctx.begin_path();
    ctx.ellipse(0.0, 0.0, 8.4 * (1.0 + squash * 0.14), 8.2 * (1.0 - squash * 0.10), 0.0, 0.0, PI * 2.0, false);
    ctx.fill();
    ctx.stroke();
    ctx.set_fill_color("rgba(255,255,255,0.72)");
    ctx.begin_path();
    ctx.arc(-2.2, -2.2, 2.3, 0.0, PI * 2.0, false);
    ctx.fill();
    ctx.restore();

    ctx.restore();
}

/// Draws a small blue heart (replaces the 💙 emoji).
///
/// @param ctx The canvas.
/// @param center Centre of the heart.
fn draw_heart(ctx: &mut Canvas, center: Vec2) {
    let x = center.x;
    let y = center.y;
    ctx.set_fill_color("#4aa3f0");
    ctx.set_stroke_color("#2f78c8");
    ctx.set_line_width(0.8);
    ctx.begin_path();
    ctx.move_to(x, y + 5.8);
    ctx.bezier_curve_to(x - 8.0, y + 0.5, x - 6.8, y - 7.2, x, y - 3.4);
    ctx.bezier_curve_to(x + 6.8, y - 7.2, x + 8.0, y + 0.5, x, y + 5.8);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
    ctx.set_fill_color("rgba(255, 255, 255, 0.55)");
    ctx.begin_path();
    ctx.ellipse(x - 3.2, y - 2.4, 1.8, 1.1, -0.6, 0.0, PI * 2.0, false);
    ctx.fill();
}

/// Draws a small water drop (replaces the 💧 emoji).
///
/// @param ctx The canvas.
/// @param center Centre of the drop.
fn draw_drop(ctx: &mut Canvas, center: Vec2) {
    let x = center.x;
    let y = center.y;
    ctx.set_fill_color("#5db8f2");
    ctx.set_stroke_color("#3a8fd0");
    ctx.set_line_width(0.8);
    ctx.begin_path();
    ctx.move_to(x, y - 7.0);
    ctx.bezier_curve_to(x + 1.6, y - 3.6, x + 5.4, y - 0.6, x + 5.4, y + 2.4);
    ctx.bezier_curve_to(x + 5.4, y + 5.6, x + 3.0, y + 7.4, x, y + 7.4);
    ctx.bezier_curve_to(x - 3.0, y + 7.4, x - 5.4, y + 5.6, x - 5.4, y + 2.4);
    ctx.bezier_curve_to(x - 5.4, y - 0.6, x - 1.6, y - 3.6, x, y - 7.0);
    ctx.close_path();
    ctx.fill();
    ctx.stroke();
    ctx.set_fill_color("rgba(255, 255, 255, 0.6)");
    ctx.begin_path();
    ctx.ellipse(x - 2.2, y + 2.6, 1.3, 2.1, 0.3, 0.0, PI * 2.0, false);
    ctx.fill();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @return A 65% layout with the mouse idle.
    fn test_layout() -> DaFeiYuLayout {
        DaFeiYuLayout {
            logical_w: 420.0,
            kb_center_x: 210.0,
            kb_center_y: 370.0,
            kb_width: 300.0,
            kb_height: 100.0,
            mouse_active_at: -100000.0,
            privacy_typing_at: -100000.0,
            ..DaFeiYuLayout::default()
        }
    }

    #[test]
    fn sprites_decode() {
        let character = DaFeiYu::new();
        assert_eq!(character.sprite_idle.width(), 224);
        assert_eq!(character.sprite_idle.height(), 278);
    }

    #[test]
    fn right_edge_key_slaps_the_tail() {
        let mut character = DaFeiYu::new();
        let layout = test_layout();
        character.on_key_down(0x27, Vec2::new(350.0, 380.0), &layout, false, 1000.0);
        assert!(character.tail.held_vks.contains(&0x27));
        assert_eq!(character.ripples.len(), 2);
        character.on_key_up(0x27);
        assert!(character.tail.held_vks.is_empty());
    }

    #[test]
    fn left_key_uses_left_hand() {
        let mut character = DaFeiYu::new();
        let layout = test_layout();
        character.on_key_down(0x41, Vec2::new(120.0, 380.0), &layout, false, 1000.0);
        assert!(character.left_hand.held_vks.contains(&0x41));
        assert!(character.right_hand.held_vks.is_empty());
    }

    #[test]
    fn privacy_mode_ignores_keys_and_renders() {
        let mut character = DaFeiYu::new();
        let mut layout = test_layout();
        layout.privacy_mode = true;
        character.on_privacy_mode_changed(true);
        character.on_key_down(0x41, Vec2::new(120.0, 380.0), &layout, false, 1000.0);
        assert!(character.left_hand.held_vks.is_empty());
        for frame in 0..30 {
            character.update(1.0 / 60.0, 1000.0 + frame as f64 * 16.7, &layout);
        }
        let mut canvas = Canvas::new(420, 440);
        character.render_body(&mut canvas, 1500.0, &layout);
        character.render_foreground(&mut canvas, 1.0 / 60.0, 1500.0, &layout);
        assert!(canvas.pixels().iter().any(|value| *value != 0));
    }
}
