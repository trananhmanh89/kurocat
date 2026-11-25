//! Falling asleep after a few seconds without keyboard or mouse input: the eyes close, the head
//! droops, breathing slows down and "Z" bubbles float up from the head. Any input wakes the pet.

use super::{random, CatBreed, Engine, Vec2};
use crate::render::{Canvas, FontSpec, Gradient, LineCap, TextAlign, TextBaseline};
use std::f32::consts::PI;

/// How long without keyboard or mouse input before the pet falls asleep (ms).
pub(crate) const SLEEP_AFTER_MS: f64 = 5000.0;
/// How long falling asleep takes (seconds).
const FALL_ASLEEP_SECONDS: f32 = 2.4;
/// How long waking up takes (seconds).
const WAKE_UP_SECONDS: f32 = 0.35;
/// Time between two new Z bubbles (ms).
const Z_INTERVAL_MS: f64 = 1150.0;
/// How long one Z bubble lives (ms).
const Z_LIFETIME_MS: f64 = 3400.0;
/// How far a Z bubble rises during its life (logical pixels).
const Z_RISE: f32 = 62.0;
/// How far a Z bubble drifts to the right during its life (logical pixels).
const Z_DRIFT_X: f32 = 24.0;
/// How long the head takes to sink down after falling asleep or after a startle (seconds).
const NOD_SINK_SECONDS: f32 = 4.0;
/// Shortest time between two startles (seconds).
const STARTLE_MIN_SECONDS: f32 = 60.0;
/// Longest time between two startles (seconds).
const STARTLE_MAX_SECONDS: f32 = 120.0;
/// How long the quick jerk back up at the end of a nod takes (seconds).
const NOD_JERK_SECONDS: f32 = 0.22;
/// How long the head settles after the jerk before it starts dropping again (seconds).
const NOD_SETTLE_SECONDS: f32 = 0.7;
/// How long one slow sway from one side to the other and back takes while asleep (seconds).
const SWAY_SECONDS: f32 = 6.5;

/// One "Z" bubble floating up from the sleeping pet's head.
#[derive(Clone, Debug)]
pub struct SleepZ {
    /// Start position x, in logical pixels.
    pub start_x: f32,
    /// Start position y, in logical pixels.
    pub start_y: f32,
    /// How long it has been floating (ms).
    pub age_ms: f64,
    /// Size multiplier, so the bubbles grow from a small "z" to a big "Z".
    pub size: f32,
    /// Phase of the side-to-side wobble, so the bubbles do not move in lockstep.
    pub sway_phase: f32,
}

/// Eases a value from 0 to 1 with a soft start and a soft end.
///
/// @param value The input; clamped to 0..1.
/// @return The eased value, 0..1.
fn smoothstep(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Eases a value from 0 to 1 with a small overshoot at the end, for a soft "pop".
///
/// @param value The input; clamped to 0..1.
/// @return The eased value; slightly above 1 near the end.
fn ease_out_back(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    let overshoot = 1.7;
    let shifted = t - 1.0;
    1.0 + (overshoot + 1.0) * shifted * shifted * shifted + overshoot * shifted * shifted
}

/// Picks how long one nod lasts, from sinking down to the next startle.
///
/// @return The length in seconds, between 1 and 2 minutes.
pub(crate) fn random_nod_length() -> f32 {
    STARTLE_MIN_SECONDS + random() * (STARTLE_MAX_SECONDS - STARTLE_MIN_SECONDS)
}

/// Returns how far the head has nodded down at a moment of one nod.
///
/// @param time Seconds into the nod.
/// @param droop_seconds When the startle starts; until then the head sinks and stays down.
/// @return 0 = upright, 1 = lowest point; slightly below 0 just after the startle.
fn nod_curve(time: f32, droop_seconds: f32) -> f32 {
    if time < droop_seconds {
        // Sinks slowly at first, then faster as the cat drifts off, then stays down.
        return smoothstep(time / NOD_SINK_SECONDS);
    }
    let jerk_time = time - droop_seconds;
    if jerk_time < NOD_JERK_SECONDS {
        // Snaps back up and a little past upright.
        let progress = jerk_time / NOD_JERK_SECONDS;
        let eased = 1.0 - (1.0 - progress) * (1.0 - progress);
        return 1.0 - eased * 1.1;
    }
    // Settles from just above upright back to upright.
    let settle = ((jerk_time - NOD_JERK_SECONDS) / NOD_SETTLE_SECONDS).min(1.0);
    -0.1 * (1.0 - smoothstep(settle))
}

/// Draws a closed, sleeping eye: a soft downward curve. Expects eye-space coordinates.
///
/// @param ctx The canvas, translated to the eye centre.
/// @param color Stroke colour of the line.
/// @param alpha Opacity of the line, 0..1.
pub(crate) fn draw_sleeping_eye_line(ctx: &mut Canvas, color: &str, alpha: f32) {
    ctx.save();
    let base_alpha = ctx.global_alpha();
    ctx.set_global_alpha(base_alpha * alpha.clamp(0.0, 1.0));
    ctx.set_stroke_color(color);
    ctx.set_line_width(2.9);
    ctx.set_line_cap(LineCap::Round);
    ctx.begin_path();
    ctx.move_to(-13.5, -0.5);
    ctx.quadratic_curve_to(0.0, 5.2, 13.5, -0.5);
    ctx.stroke();
    ctx.restore();
}

impl Engine {
    /// Records keyboard or mouse activity, which keeps the pet awake (or wakes it up).
    ///
    /// @param now Current time in milliseconds.
    pub(crate) fn mark_activity(&mut self, now: f64) {
        self.last_activity_at = now;
    }

    /// Returns how asleep the pet is, eased for drawing.
    ///
    /// @return 0 = awake, 1 = fast asleep.
    pub(crate) fn sleep_amount(&self) -> f32 {
        smoothstep(self.sleep_progress)
    }

    /// Returns how far the tail has lowered and wrapped around the front paws. It follows a
    /// little after the eyes close.
    ///
    /// @return 0 = normal tail, 1 = fully wrapped.
    pub(crate) fn tail_wrap_amount(&self) -> f32 {
        smoothstep((self.sleep_progress - 0.25) / 0.75)
    }

    /// Returns true while the pet is falling asleep or waking up.
    ///
    /// @return True during the transition.
    pub(crate) fn is_sleep_changing(&self) -> bool {
        self.sleep_progress > 0.0 && self.sleep_progress < 1.0
    }

    /// Returns how much the eyes are drawn as closed sleeping lines instead of thin open eyes.
    ///
    /// @return 0 = normal eyes, 1 = sleeping lines only.
    pub(crate) fn sleep_eye_line_amount(&self) -> f32 {
        smoothstep((self.sleep_amount() - 0.7) / 0.22)
    }

    /// Advances falling asleep, waking up, the breathing rhythm and the Z bubbles by one frame.
    ///
    /// @param dt Frame time in seconds.
    /// @param now Current time in milliseconds.
    pub(crate) fn update_sleep(&mut self, dt: f32, now: f64) {
        let wants_sleep = now - self.last_activity_at >= SLEEP_AFTER_MS;
        if wants_sleep {
            self.sleep_progress = (self.sleep_progress + dt / FALL_ASLEEP_SECONDS).min(1.0);
        } else {
            self.sleep_progress = (self.sleep_progress - dt / WAKE_UP_SECONDS).max(0.0);
        }
        let sleep = self.sleep_amount();

        // Breathing slows down while asleep. The phase is integrated so the speed change never jumps.
        let breaths_per_second = 2.1 - 0.95 * sleep;
        self.breath_phase = (self.breath_phase + dt * breaths_per_second) % (PI * 2.0);

        self.update_nod(dt);

        self.sleep_sway_phase = (self.sleep_sway_phase + dt * PI * 2.0 / SWAY_SECONDS) % (PI * 2.0);
        self.sleep_sway = self.sleep_sway_phase.sin() * sleep;

        if self.sleep_progress <= 0.0 {
            self.sleep_zs.clear();
            self.next_sleep_z_at = 0.0;
            self.sleep_z_count = 0;
            return;
        }

        for bubble in self.sleep_zs.iter_mut() {
            bubble.age_ms += dt as f64 * 1000.0;
        }
        self.sleep_zs.retain(|bubble| bubble.age_ms < Z_LIFETIME_MS);
        if wants_sleep && sleep > 0.75 && now >= self.next_sleep_z_at {
            let anchor = self.sleep_z_anchor();
            let sizes = [0.74, 0.9, 1.06];
            let size = sizes[(self.sleep_z_count % 3) as usize];
            self.sleep_zs.push(SleepZ {
                start_x: anchor.x,
                start_y: anchor.y,
                age_ms: 0.0,
                size,
                sway_phase: self.sleep_z_count as f32 * 2.1,
            });
            self.sleep_z_count += 1;
            self.next_sleep_z_at = now + Z_INTERVAL_MS;
        }
    }

    /// Advances the dozing nod: the head sinks and stays down, and every 1 to 2 minutes it jerks
    /// back up with an ear twitch (a startle) and sinks again. It only runs once the pet is fully
    /// asleep.
    ///
    /// @param dt Frame time in seconds.
    fn update_nod(&mut self, dt: f32) {
        if self.sleep_progress < 1.0 {
            if self.nod_time > 0.0 {
                // Each new nap waits a fresh random time before the first startle.
                self.nod_time = 0.0;
                self.nod_length = random_nod_length();
            }
            self.nod_amount = (self.nod_amount - dt * 4.0).max(0.0);
            return;
        }

        let droop_seconds = self.nod_length - NOD_JERK_SECONDS - NOD_SETTLE_SECONDS;
        let previous_time = self.nod_time;
        self.nod_time += dt;

        // The moment the head jerks up, the ears twitch as if the cat almost woke up.
        if previous_time < droop_seconds && self.nod_time >= droop_seconds {
            self.ear_twitch_left = 1.0;
            self.ear_twitch_right = 0.7;
        }

        if self.nod_time >= self.nod_length {
            self.nod_time = 0.0;
            self.nod_length = random_nod_length();
        }

        self.nod_amount = nod_curve(self.nod_time, droop_seconds);
    }

    /// Returns where new Z bubbles appear: just above and to the right of the head.
    ///
    /// @return The point in logical pixels.
    fn sleep_z_anchor(&self) -> Vec2 {
        if self.cat_breed == CatBreed::DaFeiYu {
            let layout = self.build_dafeiyu_layout();
            return self.dafeiyu.sleep_z_anchor(&layout);
        }
        let head = self.spine_point(0.24, 0.0);
        Vec2::new(head.x + 50.0, head.y - 48.0)
    }

    /// Draws the Z bubbles floating up from the sleeping pet.
    ///
    /// @param ctx The canvas, in logical pet coordinates.
    pub(crate) fn draw_sleep_zs(&self, ctx: &mut Canvas) {
        if self.sleep_zs.is_empty() {
            return;
        }
        // While waking up, all bubbles fade out together.
        let wake_fade = self.sleep_amount();

        for bubble in self.sleep_zs.iter() {
            let t = (bubble.age_ms / Z_LIFETIME_MS).clamp(0.0, 1.0) as f32;

            let fade_in = smoothstep(t / 0.14);
            let fade_out = 1.0 - smoothstep((t - 0.62) / 0.38);
            let alpha = fade_in * fade_out * wake_fade;
            if alpha <= 0.01 {
                continue;
            }

            let wobble = (t * PI * 2.0 * 0.9 + bubble.sway_phase).sin();
            let x = bubble.start_x + Z_DRIFT_X * smoothstep(t) + wobble * 5.0 * t.min(0.5) * 2.0;
            let y = bubble.start_y - Z_RISE * t;
            let pop = if t < 0.16 {
                0.35 + 0.65 * ease_out_back(t / 0.16)
            } else {
                1.0
            };
            let scale = bubble.size * pop * (0.9 + 0.22 * t);
            let rotation = wobble * 0.16 - 0.08;

            ctx.save();
            ctx.set_global_alpha(alpha);
            ctx.translate(x, y);
            ctx.rotate(rotation);
            ctx.scale(scale, scale);

            let radius = 12.5;

            // Soft round bubble.
            ctx.set_shadow_color("rgba(20, 24, 40, 0.20)");
            ctx.set_shadow_blur(4.0);
            ctx.set_shadow_offset_y(1.5);
            let mut fill = Gradient::radial(-4.0, -5.0, 1.0, 0.0, 0.0, radius);
            fill.add_color_stop(0.0, "#ffffff");
            fill.add_color_stop(1.0, "#e6ecfb");
            ctx.set_fill_gradient(fill);
            ctx.begin_path();
            ctx.arc(0.0, 0.0, radius, 0.0, PI * 2.0, false);
            ctx.fill();

            ctx.set_shadow_blur(0.0);
            ctx.set_shadow_offset_y(0.0);
            ctx.set_stroke_color("rgba(58, 70, 112, 0.40)");
            ctx.set_line_width(1.1);
            ctx.stroke();

            // Small shine in the top-left corner.
            ctx.set_stroke_color("rgba(255, 255, 255, 0.95)");
            ctx.set_line_width(1.6);
            ctx.set_line_cap(LineCap::Round);
            ctx.begin_path();
            ctx.arc(0.0, 0.0, radius - 3.6, PI * 1.08, PI * 1.42, false);
            ctx.stroke();

            // The letter: a small "z" for the first bubble of each group, then big "Z"s.
            let label = if bubble.size < 0.8 { "z" } else { "Z" };
            ctx.set_font(FontSpec::new(800, 14.0));
            ctx.set_fill_color("#4a5a8c");
            ctx.set_text_align(TextAlign::Center);
            ctx.set_text_baseline(TextBaseline::Middle);
            ctx.fill_text(label, 0.3, 0.2);

            ctx.restore();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs the engine at 60 fps from `start` for `seconds`, without any input.
    ///
    /// @param engine The engine.
    /// @param start Start time in ms.
    /// @param seconds How long to run.
    /// @return The time after the last frame (ms).
    fn run_frames(engine: &mut Engine, start: f64, seconds: f64) -> f64 {
        let dt = 1.0 / 60.0;
        let frames = (seconds / dt).round() as u32;
        let mut now = start;
        for _ in 0..frames {
            now += dt * 1000.0;
            engine.update(dt as f32, now);
        }
        now
    }

    #[test]
    fn stays_awake_before_the_timeout() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        run_frames(&mut engine, start, 4.5);
        assert_eq!(engine.sleep_amount(), 0.0);
        assert!(engine.sleep_zs.is_empty());
    }

    #[test]
    fn falls_asleep_and_shows_z_bubbles() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        run_frames(&mut engine, start, 9.0);
        assert!(engine.sleep_amount() > 0.99);
        assert!(!engine.sleep_zs.is_empty());
        assert!(!engine.is_sleep_changing());
    }

    #[test]
    fn nod_sinks_stays_down_then_startles() {
        let startle_at = 70.0;
        assert_eq!(nod_curve(0.0, startle_at), 0.0);
        let halfway = nod_curve(NOD_SINK_SECONDS / 2.0, startle_at);
        assert!(halfway > 0.4 && halfway < 0.6);
        assert_eq!(nod_curve(NOD_SINK_SECONDS, startle_at), 1.0);
        assert_eq!(nod_curve(startle_at - 0.01, startle_at), 1.0);
        assert!(nod_curve(startle_at + NOD_JERK_SECONDS, startle_at) < 0.0);
        assert_eq!(nod_curve(startle_at + NOD_JERK_SECONDS + NOD_SETTLE_SECONDS, startle_at), 0.0);
    }

    #[test]
    fn startles_every_one_to_two_minutes() {
        for _ in 0..50 {
            let length = random_nod_length();
            assert!((STARTLE_MIN_SECONDS..=STARTLE_MAX_SECONDS).contains(&length));
        }
    }

    #[test]
    fn input_wakes_it_up() {
        let mut engine = Engine::new();
        let start = engine.last_activity_at;
        let now = run_frames(&mut engine, start, 9.0);
        engine.mark_activity(now);
        run_frames(&mut engine, now, 0.5);
        assert_eq!(engine.sleep_amount(), 0.0);
        assert!(engine.sleep_zs.is_empty());
    }



}
