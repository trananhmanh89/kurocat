//! The animated pet: state, animation and drawing.
//!
//! [`Engine`] receives input events, advances the animation in [`Engine::update`] and draws a
//! frame into a [`Canvas`] in [`Engine::render`]. The drawing code is split by body part:
//!
//! - `animation.rs`: input handling, the per-frame update and the render order;
//! - `body.rs`: tail, body, head, ears, eyes and paws of the built-in black cat and ragdoll;
//! - `custom_skin.rs`: the same parts for custom (MCP) cats, driven by a [`ResolvedSkin`];
//! - `desk.rs`: keyboard, mouse, key bubbles and click badges;
//! - `dafeiyu.rs`: the sprite-based whale-girl character.
//!
//! Geometry uses `f32` in logical pixels (the 420×440 or 540×456 pet space). Timestamps use
//! `f64` milliseconds from [`now_ms`]. `elapsed` is `f64` seconds.

mod body;
mod anger;
mod animation;
mod curious;
mod custom_skin;
mod dafeiyu;
mod desk;
pub(crate) mod sleep;

pub use dafeiyu::{DaFeiYu, DaFeiYuLayout};

use crate::keyboard_layout::KeyboardLayoutId;
use crate::skins::ResolvedSkin;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

static CLOCK_START: OnceLock<Instant> = OnceLock::new();
static RANDOM_STATE: AtomicU64 = AtomicU64::new(0x9E37_79B9_7F4A_7C15);

/// Milliseconds since the first call (the equivalent of `performance.now()`).
///
/// @return Elapsed milliseconds.
pub fn now_ms() -> f64 {
    let start = CLOCK_START.get_or_init(Instant::now);
    start.elapsed().as_secs_f64() * 1000.0
}

/// A pseudo-random number in 0..1 (the equivalent of `Math.random()`).
///
/// @return A number from 0 (inclusive) to 1 (exclusive).
pub fn random() -> f32 {
    let mut value = RANDOM_STATE.load(Ordering::Relaxed);
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    RANDOM_STATE.store(value, Ordering::Relaxed);
    ((value >> 40) as f32) / ((1u64 << 24) as f32)
}

/// A 2D point or vector.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    /// @param x Horizontal value.
    /// @param y Vertical value.
    /// @return The vector.
    pub fn new(x: f32, y: f32) -> Vec2 {
        Vec2 { x, y }
    }
}

/// Which character is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatBreed {
    /// The classic black cat (also the base for custom skins).
    Black,
    /// The built-in ragdoll.
    Ragdoll,
    /// The sprite-based whale girl.
    DaFeiYu,
}

/// Which way the keyboard faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyboardOrientation {
    /// Seen from the cat's side (legends upside down to the viewer).
    Cat,
    /// Seen from the viewer's side (legends readable).
    Viewer,
}

impl KeyboardOrientation {
    /// Reads the orientation from its settings value.
    ///
    /// @param value "cat" or "viewer".
    /// @return The orientation ("cat" for anything unknown).
    pub fn from_setting(value: &str) -> KeyboardOrientation {
        if value == "viewer" {
            KeyboardOrientation::Viewer
        } else {
            KeyboardOrientation::Cat
        }
    }
}

/// Left or right paw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PawSide {
    Left,
    Right,
}

/// What a paw is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PawMode {
    /// Resting on the desk.
    Idle,
    /// Curled up against the belly.
    Curled,
    /// Poking a key.
    Poke,
    /// Holding the mouse.
    Mouse,
}

/// One motion-blur ghost of a fast-moving paw.
#[derive(Clone, Copy, Debug)]
pub struct SmearFrame {
    pub sx: f32,
    pub sy: f32,
    pub px: f32,
    pub py: f32,
    pub alpha: f32,
}

/// Animation state of one front paw.
#[derive(Clone, Debug)]
pub struct PawState {
    pub side: PawSide,
    /// Vertical position of the shoulder along the spine (0 = ear tips, 1 = base).
    pub shoulder_norm_y: f32,
    /// Horizontal offset of the shoulder from the spine.
    pub shoulder_half_x: f32,
    pub pos: Vec2,
    pub vel: Vec2,
    pub target: Vec2,
    pub mode: PawMode,
    pub idle_weight: f32,
    pub curl_weight: f32,
    pub poke_time: f32,
    pub impact_squash: f32,
    pub last_active_at: f64,
    pub smears: Vec<SmearFrame>,
}

/// A keycap bubble that pops up above the keyboard.
#[derive(Clone, Debug)]
pub struct KeyBubble {
    pub label: String,
    pub is_space: bool,
    pub x: f32,
    pub target_x: f32,
    pub y: f32,
    pub target_y: f32,
    pub rot: f32,
    pub created_at: f64,
    pub lifetime: f64,
}

/// Press animation of one key.
#[derive(Clone, Copy, Debug)]
pub struct KeyAnim {
    pub pressed: bool,
    pub press_amount: f32,
    pub last_hit_at: f64,
}

/// A small "LMB" / "RMB" / "MMB" pill shown on mouse clicks.
#[derive(Clone, Debug)]
pub struct MouseBadge {
    pub label: String,
    pub color: String,
    pub x: f32,
    pub y: f32,
    pub created_at: f64,
    pub lifetime: f64,
}

/// Half-widths of the cat silhouette from the ear tips (t = 0) to the seated base (t = 1).
pub const BODY_SLICES: [(f32, f32); 11] = [
    (0.12, 49.0),
    (0.21, 56.0),
    (0.30, 60.0),
    (0.38, 52.0),
    (0.47, 54.0),
    (0.57, 62.0),
    (0.68, 70.0),
    (0.79, 73.0),
    (0.90, 69.0),
    (0.97, 58.0),
    (1.00, 46.0),
];

/// The whole pet: settings that affect drawing plus all animation state.
///
/// Field names follow the original Canvas implementation (camelCase → snake_case).
pub struct Engine {
    pub(crate) last_time: f64,
    pub(crate) elapsed: f64,

    pub show_key_bubbles: bool,
    pub privacy_mode: bool,
    pub cat_breed: CatBreed,
    pub custom_skin: Option<ResolvedSkin>,
    pub keyboard_layout: KeyboardLayoutId,
    pub keyboard_orientation: KeyboardOrientation,
    pub(crate) render_scale: f32,
    pub(crate) dafeiyu: DaFeiYu,
    pub(crate) privacy_close: f32,
    pub(crate) privacy_typing_at: f64,
    pub(crate) last_mouse_nx: f32,
    pub(crate) last_dt: f32,

    // Layout (65%: 420×440, 108-key: 540×456).
    pub(crate) logical_w: f32,
    pub(crate) logical_h: f32,
    pub(crate) cat_center_x: f32,
    pub(crate) cat_top_y: f32,
    pub(crate) cat_base_y: f32,
    pub(crate) cat_height: f32,

    // Keyboard deck below the cat.
    pub(crate) kb_center_x: f32,
    pub(crate) kb_center_y: f32,
    pub(crate) kb_width: f32,
    pub(crate) kb_height: f32,
    pub(crate) kb_tilt_rad: f32,

    // Mouse to the left of the keyboard.
    pub(crate) mouse_base_x: f32,
    pub(crate) mouse_base_y: f32,
    pub(crate) mouse_offset_x: f32,
    pub(crate) mouse_offset_y: f32,
    pub(crate) mouse_target_offset_x: f32,
    pub(crate) mouse_target_offset_y: f32,
    pub(crate) mouse_left_down: bool,
    pub(crate) mouse_right_down: bool,
    pub(crate) mouse_middle_down: bool,
    pub(crate) mouse_left_press: f32,
    pub(crate) mouse_right_press: f32,
    pub(crate) mouse_middle_press: f32,
    pub(crate) mouse_scroll_flash: f32,
    pub(crate) last_mouse_move_at: f64,
    pub(crate) mouse_badges: Vec<MouseBadge>,

    // Cursor tracking, eyes and head turn.
    pub(crate) cursor_rel_x: f32,
    pub(crate) cursor_rel_y: f32,
    pub(crate) cursor_over_cat: bool,
    pub(crate) eye_look_x: f32,
    pub(crate) eye_look_y: f32,
    pub(crate) face_turn_x: f32,

    // Spine bending and typing sway.
    pub(crate) spine_lean: f32,
    pub(crate) head_tilt: f32,
    pub(crate) typing_hunch: f32,
    pub(crate) typing_sway: f32,
    pub(crate) typing_sway_vel: f32,
    pub(crate) typing_groove_phase: f32,
    pub(crate) last_key_bias: f32,
    pub(crate) strike_rock: f32,
    pub(crate) strike_rock_vel: f32,
    pub(crate) body_bob: f32,
    pub(crate) kb_bounce: f32,

    // Blinking, petting, ears and typing rhythm.
    pub(crate) next_blink_at: f64,
    pub(crate) blink_progress: f32,
    pub(crate) is_blinking: bool,
    pub(crate) last_pet_cursor_x: Option<f32>,
    pub(crate) last_pet_cursor_y: Option<f32>,
    pub(crate) pet_energy: f32,
    pub(crate) squint_amount: f32,
    pub(crate) ear_twitch_left: f32,
    pub(crate) ear_twitch_right: f32,
    pub(crate) recent_key_times: Vec<f64>,
    pub(crate) typing_energy: f32,
    pub(crate) last_any_key_at: f64,

    // Falling asleep without input (see `sleep.rs`).
    /// Time of the last keyboard or mouse input (ms).
    pub(crate) last_activity_at: f64,
    /// 0 = awake, 1 = asleep; moves linearly, `sleep_amount` eases it for drawing.
    pub(crate) sleep_progress: f32,
    /// Breathing phase in radians; slows down while asleep.
    pub(crate) breath_phase: f32,
    /// Z bubbles floating up while asleep.
    pub(crate) sleep_zs: Vec<sleep::SleepZ>,
    /// When the next Z bubble appears (ms).
    pub(crate) next_sleep_z_at: f64,
    /// How many Z bubbles appeared in this nap; picks their size.
    pub(crate) sleep_z_count: u32,
    /// Seconds into the current nod (the head slowly drops, then jerks back up).
    pub(crate) nod_time: f32,
    /// Length of the current nod in seconds; varies a little from nod to nod.
    pub(crate) nod_length: f32,
    /// How far the head has nodded down: 0 = upright, 1 = lowest point.
    pub(crate) nod_amount: f32,
    /// Phase of the slow side-to-side sway while asleep, in radians.
    pub(crate) sleep_sway_phase: f32,
    /// Current sway while asleep: -1 = leaning left, 1 = leaning right.
    pub(crate) sleep_sway: f32,

    // Curious mode (see `curious.rs`).
    /// Recent cursor steps, for the speed measurement.
    pub(crate) pointer_samples: Vec<curious::PointerSample>,
    /// Last cursor position seen (0..1 across the virtual screen).
    pub(crate) last_pointer_sample: Option<(f32, f32)>,
    /// When the last cursor position was seen (ms).
    pub(crate) last_pointer_sample_at: f64,
    /// The cat stays curious until this time (ms).
    pub(crate) curious_until: f64,
    /// How curious the cat looks: 0 = not at all, 1 = fully (may overshoot a little).
    pub(crate) curious_amount: f32,
    /// Velocity of the spring that eases `curious_amount`.
    pub(crate) curious_velocity: f32,

    // Anger (see `anger.rs`).
    /// A grumpy wake-up lasts until this time (ms).
    pub(crate) cross_until: f64,
    /// How cross the cat looks: 0 = calm, 1 = fully cross.
    pub(crate) cross_amount: f32,
    /// Velocity of the spring that eases `cross_amount`.
    pub(crate) cross_velocity: f32,
    /// Which way the cross cat looks away: -1 = left, 1 = right.
    pub(crate) cross_huff_dir: f32,
    /// Beat counter of the tail thumps.
    pub(crate) thump_phase: f32,
    /// How high the tail is lifted for the next thump, 0..1.
    pub(crate) thump_lift: f32,
    /// Sideways lash of the thumping tail, -1..1.
    pub(crate) thump_lash: f32,
    /// When the tail last slammed the desk (ms).
    pub(crate) last_slam_at: f64,
    /// When the current run of fast typing started (ms), if typing is fast right now.
    pub(crate) fast_typing_since: Option<f64>,
    /// How strongly the eyes glare from fast typing, 0..1.
    pub(crate) typing_glare: f32,
    /// Recent mouse button presses (ms), counted with key presses for the typing glare.
    pub(crate) recent_click_times: Vec<f64>,

    // Paws.
    pub(crate) left_paw: PawState,
    pub(crate) right_paw: PawState,
    pub(crate) last_space_paw: PawSide,

    // Keys and bubbles.
    pub(crate) key_anims: HashMap<&'static str, KeyAnim>,
    pub(crate) bubbles: Vec<KeyBubble>,
    /// Counts bubbles and click badges shown so far; used to stagger bubble heights.
    pub(crate) popup_counter: u32,

    /// The keyboard drawn once while no key moves, reused every frame (see `desk.rs`).
    pub(crate) keyboard_cache: std::cell::RefCell<Option<desk::KeyboardCache>>,
}
