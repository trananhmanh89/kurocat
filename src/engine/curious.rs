//! Curious mode: when the cursor darts across the screen (like a laser pointer), the cat stares
//! at it with wide eyes, big round pupils and sparkling catchlights, cocks its head and flicks
//! the tip of its tail. Ported from the "Curious" state of the original TypeScript engine.
//!
//! It never starts while the cat sleeps, while a password field is focused, for DaFeiYu, or while
//! the user types or holds a key (so WASD in a game does not trigger it).

use super::{CatBreed, Engine};
use crate::render::Canvas;
use std::f32::consts::PI;

/// How far back pointer movement is summed up, in milliseconds.
const SPEED_WINDOW_MS: f64 = 220.0;
/// Pointer speed (screen widths per second) that makes the cat curious.
const CURIOUS_SPEED: f32 = 0.92;
/// How many movement samples the window must hold, so a single jump never counts.
const MIN_SAMPLES: usize = 4;
/// How long the cat stays curious after the last fast movement, in milliseconds.
const CURIOUS_HOLD_MS: f64 = 2200.0;
/// Two samples further apart than this are not one continuous movement, in milliseconds.
const MAX_SAMPLE_GAP_MS: f64 = 140.0;
/// Steps shorter than this are sensor jitter (share of the screen).
const MIN_STEP: f32 = 0.0015;
/// Steps longer than this are jumps, for example the cursor being recentred (share of the screen).
const MAX_STEP: f32 = 0.45;
/// No curiosity until this long after the last key press, in milliseconds.
const QUIET_AFTER_TYPING_MS: f64 = 900.0;
/// Spring that eases the curious amount in and out (slight overshoot when the eyes open).
const SPRING_HZ: f32 = 3.8;
const SPRING_DAMPING: f32 = 0.72;

/// One step of cursor movement.
pub(crate) struct PointerSample {
    /// When the step happened, in milliseconds.
    at_ms: f64,
    /// Length of the step as a share of the screen.
    distance: f32,
}

/// Advances a damped spring towards a target, in small sub-steps so it stays stable at any frame rate.
///
/// @param value Current value.
/// @param velocity Current velocity (updated in place).
/// @param target Value the spring pulls towards.
/// @param hz Spring frequency.
/// @param damping Damping ratio (1 = no overshoot).
/// @param dt Frame time in seconds.
/// @return The new value.
pub(crate) fn step_spring(value: f32, velocity: &mut f32, target: f32, hz: f32, damping: f32, dt: f32) -> f32 {
    let omega = 2.0 * PI * hz;
    let steps = ((dt * 240.0).ceil() as i32).max(1);
    let step_dt = dt / steps as f32;
    let mut position = value;
    for _ in 0..steps {
        *velocity += (omega * omega * (target - position) - 2.0 * damping * omega * *velocity) * step_dt;
        position += *velocity * step_dt;
    }
    position
}

/// Draws a pupil and, when wanted, its two catchlights. Must be called inside the iris clip,
/// in eye space (origin at the eye centre).
///
/// @param ctx The canvas.
/// @param look_x Where the pupil looks, in pixels from the eye centre.
/// @param look_y Where the pupil looks, in pixels from the eye centre.
/// @param radius_x Horizontal radius of the open eye.
/// @param open_radius_y Vertical radius of the open eye (smaller while blinking).
/// @param curious Curious amount, 0..1.
/// @param pupil_color Pupil colour.
/// @param always_catchlight True for cats that always have catchlights (ragdoll and most skins).
/// @param grumpy How grumpy the eyes look, 0..1; grumpy pupils narrow into slits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_pupil(
    ctx: &mut Canvas,
    look_x: f32,
    look_y: f32,
    radius_x: f32,
    open_radius_y: f32,
    curious: f32,
    pupil_color: &str,
    always_catchlight: bool,
    grumpy: f32,
) {
    // Curious pupils dilate into big round discs; grumpy ones narrow into slits.
    let pupil_radius_x = ((6.2 + curious * 4.2) * (1.0 - grumpy * 0.34)).min(radius_x - 2.4);
    let pupil_radius_y = ((8.8 + curious * 1.9) * (1.0 - grumpy * 0.08)).min(open_radius_y - 1.4).max(1.0);

    // Bigger pupils have less room to move; keep them inside the iris while curious.
    let room_x = (radius_x - pupil_radius_x - 1.0).max(2.6);
    let room_y = (open_radius_y - pupil_radius_y - 0.6).max(2.2);
    let kept_x = look_x.clamp(-room_x, room_x);
    let kept_y = look_y.clamp(-room_y, room_y);
    let pupil_x = look_x + (kept_x - look_x) * curious;
    let pupil_y = look_y + (kept_y - look_y) * curious;

    ctx.set_fill_color(pupil_color);
    ctx.begin_path();
    ctx.ellipse(pupil_x, pupil_y, pupil_radius_x, pupil_radius_y, 0.0, 0.0, PI * 2.0, false);
    ctx.fill();

    if !always_catchlight && curious <= 0.25 {
        return;
    }
    let alpha = if always_catchlight { 0.94 } else { (curious * 0.95).min(0.92) };
    ctx.set_fill_color(&format!("rgba(255, 255, 255, {:.3})", alpha));
    ctx.begin_path();
    ctx.arc(pupil_x - 3.6 - curious * 0.8, pupil_y - 4.0 - curious * 0.5, 2.5 + curious * 0.7, 0.0, PI * 2.0, false);
    ctx.fill();

    ctx.set_fill_color(&format!("rgba(255, 255, 255, {:.3})", alpha * 0.76));
    ctx.begin_path();
    ctx.arc(pupil_x + 3.2, pupil_y + 3.2, 1.2 + curious * 0.4, 0.0, PI * 2.0, false);
    ctx.fill();
}

impl Engine {
    /// Records one cursor position for the speed measurement.
    ///
    /// @param nx Cursor x across the whole virtual screen (0..1).
    /// @param ny Cursor y across the whole virtual screen (0..1).
    /// @param now Current time in milliseconds.
    pub(crate) fn record_pointer_speed(&mut self, nx: f32, ny: f32, now: f64) {
        if self.cat_breed == CatBreed::DaFeiYu || self.privacy_mode {
            self.last_pointer_sample = None;
            return;
        }
        if let Some((last_x, last_y)) = self.last_pointer_sample {
            let gap = now - self.last_pointer_sample_at;
            if gap > 0.0 && gap <= MAX_SAMPLE_GAP_MS {
                let distance = (nx - last_x).hypot(ny - last_y);
                if distance > MIN_STEP && distance < MAX_STEP {
                    self.pointer_samples.push(PointerSample { at_ms: now, distance });
                }
            }
        }
        self.last_pointer_sample = Some((nx, ny));
        self.last_pointer_sample_at = now;
    }

    /// @return True while any key on the keyboard is held down.
    fn any_key_held(&self) -> bool {
        for anim in self.key_anims.values() {
            if anim.pressed {
                return true;
            }
        }
        false
    }

    /// @param now Current time in milliseconds.
    /// @return True when the cat may become curious right now (never while cross).
    fn can_be_curious(&self, now: f64) -> bool {
        self.cat_breed != CatBreed::DaFeiYu
            && !self.privacy_mode
            && self.sleep_progress <= 0.0
            && !self.is_cross(now)
            && self.cross_amount < 0.05
    }

    /// Measures the cursor speed, starts or ends curious mode and eases the curious amount.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in milliseconds.
    pub(crate) fn update_curious(&mut self, dt: f32, now: f64) {
        let mut stale = 0;
        for sample in &self.pointer_samples {
            if now - sample.at_ms > SPEED_WINDOW_MS {
                stale += 1;
            } else {
                break;
            }
        }
        self.pointer_samples.drain(..stale);

        let mut travelled = 0.0;
        for sample in &self.pointer_samples {
            travelled += sample.distance;
        }
        let speed = travelled / (SPEED_WINDOW_MS as f32 / 1000.0);

        let typing = now - self.last_any_key_at < QUIET_AFTER_TYPING_MS || self.any_key_held();
        let may_trigger = self.can_be_curious(now) && !typing;
        if may_trigger && self.pointer_samples.len() >= MIN_SAMPLES && speed > CURIOUS_SPEED {
            self.curious_until = self.curious_until.max(now + CURIOUS_HOLD_MS);
        }
        if typing {
            // Typing ends the stare right away.
            self.curious_until = self.curious_until.min(now);
        }

        let curious_now = self.can_be_curious(now) && now < self.curious_until;
        let target = if curious_now { 1.0 } else { 0.0 };
        let next = step_spring(
            self.curious_amount,
            &mut self.curious_velocity,
            target,
            SPRING_HZ,
            SPRING_DAMPING,
            dt,
        );
        self.curious_amount = next.clamp(0.0, 1.2);
        if !curious_now && self.curious_amount < 0.002 && self.curious_velocity.abs() < 0.01 {
            self.curious_amount = 0.0;
            self.curious_velocity = 0.0;
        }

        // A cat staring at a laser pointer does not blink.
        if curious_now && self.elapsed >= self.next_blink_at - 0.2 {
            self.next_blink_at = self.elapsed + 0.6;
        }
    }

    /// @return How curious the cat looks, 0..1 (eased, without the spring overshoot above 1).
    pub(crate) fn curious_level(&self) -> f32 {
        self.curious_amount.min(1.0)
    }

    /// @return True while curious mode is easing in, active or easing out.
    pub(crate) fn is_curious_active(&self, now: f64) -> bool {
        now < self.curious_until || self.curious_amount > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// @return A fresh engine.
    fn engine() -> Engine {
        Engine::new()
    }

    /// Moves the cursor across the screen at a given speed, running a frame after each step.
    ///
    /// @param engine The engine.
    /// @param start Start time in milliseconds.
    /// @param screens_per_second Cursor speed.
    /// @param duration_ms How long to move.
    /// @return The time after the last step.
    fn sweep(engine: &mut Engine, start: f64, screens_per_second: f32, duration_ms: f64) -> f64 {
        let mut now = start;
        let mut x = 0.1;
        while now < start + duration_ms {
            engine.record_pointer_speed(x, 0.5, now);
            engine.update_curious(0.016, now);
            x += screens_per_second * 0.016;
            if x > 0.9 {
                x = 0.1;
            }
            now += 16.0;
        }
        now
    }

    #[test]
    fn fast_cursor_makes_the_cat_curious() {
        let mut engine = engine();
        let now = sweep(&mut engine, 100000.0, 1.5, 400.0);
        assert!(engine.curious_until > now);
        let mut time = now;
        for _ in 0..40 {
            engine.update_curious(0.016, time);
            time += 16.0;
        }
        assert!(engine.curious_level() > 0.8);
    }

    #[test]
    fn slow_cursor_does_not() {
        let mut engine = engine();
        sweep(&mut engine, 100000.0, 0.4, 600.0);
        assert_eq!(engine.curious_amount, 0.0);
    }

    #[test]
    fn held_key_blocks_it() {
        let mut engine = engine();
        for anim in engine.key_anims.values_mut() {
            anim.pressed = true;
            break;
        }
        sweep(&mut engine, 100000.0, 1.5, 400.0);
        assert_eq!(engine.curious_amount, 0.0);
    }

    #[test]
    fn it_fades_after_the_hold_time() {
        let mut engine = engine();
        let mut now = sweep(&mut engine, 100000.0, 1.5, 400.0);
        for _ in 0..250 {
            engine.update_curious(0.016, now);
            now += 16.0;
        }
        assert_eq!(engine.curious_amount, 0.0);
    }
}
