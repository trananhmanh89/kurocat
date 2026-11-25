//! Anger, in two strengths.
//!
//! - **Grumpy wake-up**: a cat that slept for at least a minute and is woken up gets cross for a
//!   few seconds: a slanted eyelid and slit pupils, flattened "airplane" ears, an anger vein next
//!   to the head, a huffy side glance and the tail thumping the desk.
//! - **Typing glare**: typing and clicking fast for 2 seconds without slowing down (key presses
//!   and mouse clicks count together) gives the eyes the same grumpy look, without the vein or the other effects. It fades as soon as the typing slows down.
//!
//! Neither happens for DaFeiYu or while a password field is focused. Petting ends a grumpy wake-up.

use super::curious::step_spring;
use super::sleep::SLEEP_AFTER_MS;
use super::{CatBreed, Engine};
use crate::render::{Canvas, LineCap};
use std::f32::consts::PI;

/// How long the cat must have slept before being woken up makes it cross (ms).
const WAKE_ANGER_AFTER_SLEEP_MS: f64 = 60000.0;
/// How long a grumpy wake-up lasts before the cat calms down (ms).
const CROSS_HOLD_MS: f64 = 3200.0;
/// Spring that eases the grumpy wake-up in and out.
const CROSS_SPRING_HZ: f32 = 1.9;
const CROSS_SPRING_DAMPING: f32 = 0.86;
/// Key presses plus mouse clicks within the last 0.95 s that count as fast input (about 7 per
/// second, roughly 85 words per minute when only typing).
const FAST_TYPING_KEYS: usize = 7;
/// Once input counts as fast, it stays fast down to this many presses, so the look does not
/// flicker on a single slower key.
const KEEP_FAST_TYPING_KEYS: usize = 6;
/// Window in which key presses and clicks are counted, the same as `recent_key_times` (ms).
const FAST_INPUT_WINDOW_MS: f64 = 950.0;
/// How long the typing must stay fast before the eyes glare (ms).
const GLARE_AFTER_MS: f64 = 2000.0;
/// Tail thumps per second while cross (three thumps, then a rest beat).
const THUMP_BEATS_PER_SECOND: f32 = 2.55;
/// How far the ears turn back while cross (radians at full anger).
const EAR_FLATTEN: f32 = 0.58;

/// Where the slanted grumpy eyelid cuts the eye, in eye space.
pub(crate) struct GrumpyLid {
    /// Height of the lid line at the left edge of the eye.
    left_y: f32,
    /// Height of the lid line at the right edge of the eye.
    right_y: f32,
    /// True when the lid is drawn at all.
    active: bool,
}

/// Starts drawing an eye under a slanted grumpy eyelid: the inner corner drops lower than the
/// outer one. Saves the canvas state and clips the eye below the lid; call [`end_grumpy_lid`]
/// after drawing the iris and pupil.
///
/// @param ctx The canvas, in eye space.
/// @param eye_index 0 for the left eye, 1 for the right eye.
/// @param radius_x Horizontal radius of the eye.
/// @param open_radius_y Vertical radius of the open eye.
/// @param grumpy How grumpy the eyes look, 0..1.
/// @return Where the lid line is, for `end_grumpy_lid`.
pub(crate) fn begin_grumpy_lid(ctx: &mut Canvas, eye_index: usize, radius_x: f32, open_radius_y: f32, grumpy: f32) -> GrumpyLid {
    let inner_y = -open_radius_y + open_radius_y * 1.02 * grumpy;
    let outer_y = -open_radius_y + open_radius_y * 0.36 * grumpy;
    let (left_y, right_y) = if eye_index == 0 { (outer_y, inner_y) } else { (inner_y, outer_y) };
    let lid = GrumpyLid {
        left_y,
        right_y,
        active: grumpy > 0.04,
    };

    ctx.save();
    if lid.active {
        ctx.begin_path();
        ctx.move_to(-radius_x - 4.0, lid.left_y);
        ctx.line_to(radius_x + 4.0, lid.right_y);
        ctx.line_to(radius_x + 4.0, open_radius_y + 4.0);
        ctx.line_to(-radius_x - 4.0, open_radius_y + 4.0);
        ctx.close_path();
        ctx.clip();
    }
    lid
}

/// Ends the eyelid clip started by [`begin_grumpy_lid`] and, for eyes with a rim, draws the lid edge.
///
/// @param ctx The canvas, in eye space.
/// @param lid The lid returned by `begin_grumpy_lid`.
/// @param radius_x Horizontal radius of the eye.
/// @param open_radius_y Vertical radius of the open eye.
/// @param rim_color Colour of the lid edge, or None for eyes without a rim (the fur shows the lid).
pub(crate) fn end_grumpy_lid(ctx: &mut Canvas, lid: &GrumpyLid, radius_x: f32, open_radius_y: f32, rim_color: Option<&str>) {
    ctx.restore();
    let Some(color) = rim_color else {
        return;
    };
    if !lid.active || open_radius_y <= 2.2 {
        return;
    }
    ctx.save();
    ctx.begin_path();
    ctx.ellipse(0.0, 0.0, radius_x + 0.9, open_radius_y + 0.9, 0.0, 0.0, PI * 2.0, false);
    ctx.clip();
    ctx.set_stroke_color(color);
    ctx.set_line_width(1.9);
    ctx.set_line_cap(LineCap::Round);
    ctx.begin_path();
    ctx.move_to(-radius_x - 2.0, lid.left_y);
    ctx.line_to(radius_x + 2.0, lid.right_y);
    ctx.stroke();
    ctx.restore();
}

/// Draws one arc of the anger vein symbol.
///
/// @param ctx The canvas.
/// @param x Centre x of the arc.
/// @param y Centre y of the arc.
/// @param start Start angle, as a fraction of PI.
/// @param end End angle, as a fraction of PI.
fn vein_arc(ctx: &mut Canvas, x: f32, y: f32, start: f32, end: f32) {
    ctx.begin_path();
    ctx.arc(x, y, 5.2, start * PI, end * PI, false);
    ctx.stroke();
}

/// Draws the four arcs of the anger vein symbol in one colour.
///
/// @param ctx The canvas, centred on the symbol.
/// @param color Stroke colour.
/// @param width Stroke width.
fn draw_vein_strokes(ctx: &mut Canvas, color: &str, width: f32) {
    let offset = 6.0;
    ctx.set_stroke_color(color);
    ctx.set_line_width(width);
    vein_arc(ctx, -offset, -offset, 0.05, 0.45);
    vein_arc(ctx, offset, -offset, 0.55, 0.95);
    vein_arc(ctx, offset, offset, 1.05, 1.45);
    vein_arc(ctx, -offset, offset, 1.55, 1.95);
}

impl Engine {
    /// Called on every input before the activity time is updated: a cat woken up after at
    /// least a minute of sleep gets cross.
    ///
    /// @param now Current time in milliseconds.
    pub(crate) fn check_grumpy_wake_up(&mut self, now: f64) {
        if self.cat_breed == CatBreed::DaFeiYu || self.privacy_mode {
            return;
        }
        let slept_for = now - self.last_activity_at - SLEEP_AFTER_MS;
        if self.sleep_progress > 0.5 && slept_for >= WAKE_ANGER_AFTER_SLEEP_MS {
            self.start_cross(now);
        }
    }

    /// Makes the cat cross for a few seconds.
    ///
    /// @param now Current time in milliseconds.
    pub(crate) fn start_cross(&mut self, now: f64) {
        if !self.is_cross(now) {
            self.thump_phase = 0.0;
            // Look away from the cursor, and keep that side for the whole huff.
            self.cross_huff_dir = if self.cursor_rel_x >= 0.5 { -1.0 } else { 1.0 };
        }
        self.cross_until = now + CROSS_HOLD_MS;
        self.curious_until = self.curious_until.min(now);
    }

    /// @param now Current time in milliseconds.
    /// @return True while a grumpy wake-up is going on.
    pub(crate) fn is_cross(&self, now: f64) -> bool {
        now < self.cross_until
    }

    /// Eases the grumpy wake-up and the typing glare, and runs the tail thumps.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in milliseconds.
    pub(crate) fn update_anger(&mut self, dt: f32, now: f64) {
        let allowed = self.cat_breed != CatBreed::DaFeiYu && !self.privacy_mode;
        if !allowed || self.pet_energy > 0.3 {
            // Petting calms the cat down at once.
            self.cross_until = self.cross_until.min(now);
        }

        let cross_target = if self.is_cross(now) { 1.0 } else { 0.0 };
        let next = step_spring(
            self.cross_amount,
            &mut self.cross_velocity,
            cross_target,
            CROSS_SPRING_HZ,
            CROSS_SPRING_DAMPING,
            dt,
        );
        self.cross_amount = next.clamp(0.0, 1.0);
        if cross_target == 0.0 && self.cross_amount < 0.002 && self.cross_velocity.abs() < 0.01 {
            self.cross_amount = 0.0;
            self.cross_velocity = 0.0;
        }

        // Typing glare: fast key presses and mouse clicks that last 2 s.
        let mut stale_clicks = 0;
        for clicked_at in &self.recent_click_times {
            if now - *clicked_at > FAST_INPUT_WINDOW_MS {
                stale_clicks += 1;
            } else {
                break;
            }
        }
        self.recent_click_times.drain(..stale_clicks);
        let keys = self.recent_key_times.len() + self.recent_click_times.len();
        let needed = if self.fast_typing_since.is_some() { KEEP_FAST_TYPING_KEYS } else { FAST_TYPING_KEYS };
        if allowed && keys >= needed {
            if self.fast_typing_since.is_none() {
                self.fast_typing_since = Some(now);
            }
        } else {
            self.fast_typing_since = None;
        }
        let glaring = match self.fast_typing_since {
            Some(since) => now - since >= GLARE_AFTER_MS,
            None => false,
        };
        let glare_target = if glaring { 1.0 } else { 0.0 };
        let glare_rate = if glaring { 5.0 } else { 4.0 };
        self.typing_glare += (glare_target - self.typing_glare) * (1.0 - (-glare_rate * dt).exp());
        if !glaring && self.typing_glare < 0.002 {
            self.typing_glare = 0.0;
        }

        self.update_tail_thump(dt, now);
    }

    /// Three slow lifts and hard slams of the tail on the desk, then a rest beat, while cross.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in milliseconds.
    fn update_tail_thump(&mut self, dt: f32, now: f64) {
        let mut target_lift = 0.0;
        let mut target_lash = 0.0;
        if self.cross_amount > 0.08 {
            let previous_phase = self.thump_phase;
            self.thump_phase += dt * THUMP_BEATS_PER_SECOND;
            let beat = self.thump_phase.floor();
            let beat_time = self.thump_phase - beat;
            let resting_beat = (beat as i64) % 4 == 3;
            if !resting_beat {
                target_lift = if beat_time < 0.58 {
                    let u = beat_time / 0.58;
                    u * u * (3.0 - 2.0 * u)
                } else if beat_time < 0.72 {
                    let fall = (beat_time - 0.58) / 0.14;
                    1.0 - fall * fall
                } else {
                    0.0
                };
            }
            target_lash = (self.thump_phase * PI * 0.5).sin();

            let slam_at = beat + 0.72;
            if !resting_beat && self.is_cross(now) && previous_phase < slam_at && self.thump_phase >= slam_at {
                // The slam shakes the desk.
                self.last_slam_at = now;
                self.kb_bounce = (self.kb_bounce + 0.8).min(1.0);
                self.body_bob = (self.body_bob + 0.35).min(1.15);
            }
        }
        self.thump_lift += (target_lift - self.thump_lift) * (1.0 - (-38.0 * dt).exp());
        self.thump_lash += (target_lash - self.thump_lash) * (1.0 - (-18.0 * dt).exp());
    }

    /// @return How grumpy the eyes look, 0..1 (grumpy wake-up or typing glare).
    pub(crate) fn grumpy_eye_amount(&self) -> f32 {
        self.cross_amount.max(self.typing_glare).min(1.0)
    }

    /// @return Extra backwards rotation of the ears, in radians (flattened while cross).
    pub(crate) fn ear_flatten(&self) -> f32 {
        self.cross_amount * EAR_FLATTEN
    }

    /// @param now Current time in milliseconds.
    /// @return True while anything anger-related still moves on screen.
    pub(crate) fn is_anger_active(&self, now: f64) -> bool {
        self.is_cross(now) || self.cross_amount > 0.0 || self.typing_glare > 0.0 || self.fast_typing_since.is_some()
    }

    /// Draws the pulsing anger vein next to the head while cross.
    ///
    /// @param ctx The canvas, in logical pixels.
    /// @param breath Current breathing offset.
    pub(crate) fn draw_anger_vein(&self, ctx: &mut Canvas, breath: f32) {
        if self.cross_amount <= 0.12 || self.privacy_mode {
            return;
        }
        let head = self.spine_point(0.18, breath);
        let since_slam = (super::now_ms() - self.last_slam_at) as f32;
        let slam_pulse = 1.0 + 0.24 * (1.0 - since_slam / 250.0).max(0.0);
        let scale = self.cross_amount * slam_pulse;

        ctx.save();
        ctx.translate(head.x - 68.0 + self.face_turn_x * 0.4, head.y - 30.0);
        ctx.rotate(-0.14);
        ctx.scale(scale, scale);
        ctx.set_global_alpha((self.cross_amount * 1.1).min(1.0));
        ctx.set_line_cap(LineCap::Round);
        draw_vein_strokes(ctx, "#fbfaf3", 5.2);
        draw_vein_strokes(ctx, "#e2483d", 2.6);
        ctx.restore();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs frames of the whole engine.
    ///
    /// @param engine The engine.
    /// @param start Start time in milliseconds.
    /// @param seconds How long to run.
    /// @return The time after the last frame.
    fn run(engine: &mut Engine, start: f64, seconds: f64) -> f64 {
        let mut now = start;
        while now < start + seconds * 1000.0 {
            now += 33.0;
            engine.update(0.033, now);
        }
        now
    }

    /// Types one key press per interval for a while, running frames in between.
    ///
    /// @param engine The engine.
    /// @param start Start time in milliseconds.
    /// @param keys_per_second Typing speed.
    /// @param seconds How long to type.
    /// @return The time after the last frame.
    fn type_for(engine: &mut Engine, start: f64, keys_per_second: f64, seconds: f64) -> f64 {
        let interval = 1000.0 / keys_per_second;
        let mut now = start;
        let mut next_key = start;
        while now < start + seconds * 1000.0 {
            if now >= next_key {
                engine.recent_key_times.push(now);
                engine.last_any_key_at = now;
                engine.last_activity_at = now;
                next_key += interval;
            }
            now += 16.0;
            engine.update(0.016, now);
        }
        now
    }

    #[test]
    fn short_nap_does_not_make_it_cross() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        let now = run(&mut engine, start, 30.0);
        engine.check_grumpy_wake_up(now);
        assert!(!engine.is_cross(now));
    }

    #[test]
    fn waking_after_a_minute_of_sleep_makes_it_cross() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        let now = run(&mut engine, start, SLEEP_AFTER_MS / 1000.0 + 61.0);
        engine.check_grumpy_wake_up(now);
        engine.mark_activity(now);
        assert!(engine.is_cross(now));
        let later = run(&mut engine, now, 1.5);
        assert!(engine.cross_amount > 0.5);
        run(&mut engine, later, 6.0);
        assert_eq!(engine.cross_amount, 0.0);
    }

    #[test]
    fn fast_typing_for_two_seconds_glares() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        let now = type_for(&mut engine, start, 9.0, 1.5);
        assert_eq!(engine.typing_glare, 0.0);
        let now = type_for(&mut engine, now, 9.0, 1.5);
        assert!(engine.typing_glare > 0.5);
        assert_eq!(engine.cross_amount, 0.0);
        type_for(&mut engine, now, 3.0, 2.0);
        assert!(engine.typing_glare < 0.01);
    }

    #[test]
    fn clicks_count_with_key_presses() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        let mut now = start;
        let mut next_input = start;
        let mut use_click = false;
        while now < start + 3000.0 {
            if now >= next_input {
                if use_click {
                    engine.recent_click_times.push(now);
                } else {
                    engine.recent_key_times.push(now);
                    engine.last_any_key_at = now;
                }
                engine.last_activity_at = now;
                use_click = !use_click;
                next_input += 1000.0 / 9.0;
            }
            now += 16.0;
            engine.update(0.016, now);
        }
        assert!(engine.typing_glare > 0.5);
    }

    #[test]
    fn slow_typing_never_glares() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        type_for(&mut engine, start, 5.0, 6.0);
        assert_eq!(engine.typing_glare, 0.0);
    }
}
