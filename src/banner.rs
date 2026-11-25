//! The pill-shaped notice at the top of the pet window: "copied" confirmations, click-through
//! lock messages and so on. It is drawn on the same canvas as the pet and has a close cross.

use crate::app;
use crate::engine::now_ms;
use crate::render::{Canvas, FontSpec, TextAlign, TextBaseline};
use std::sync::atomic::Ordering;

/// Widest the message text may get before it is shortened with "…".
const MAX_TEXT_WIDTH: f32 = 268.0;
/// Distance of the pill from the top of the window.
const TOP_MARGIN: f32 = 10.0;
/// Height of the pill.
const PILL_HEIGHT: f32 = 28.0;
/// Length of the pop-in animation in milliseconds.
const POP_DURATION_MS: f64 = 200.0;


/// A rectangle in logical window pixels.
#[derive(Clone, Copy, Debug, Default)]
struct Area {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Area {
    /// @param x Point x.
    /// @param y Point y.
    /// @return True when the point lies inside the rectangle.
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }
}

/// The notice.
pub struct Banner {
    visible: bool,
    text: String,
    shown_at: f64,
    hide_at: Option<f64>,
    close_hovered: bool,
    pill_area: Area,
    close_area: Area,
}

impl Banner {
    /// @return A hidden notice.
    pub fn new() -> Banner {
        Banner {
            visible: false,
            text: String::new(),
            shown_at: 0.0,
            hide_at: None,
            close_hovered: false,
            pill_area: Area::default(),
            close_area: Area::default(),
        }
    }

    /// @return True while the notice is on screen.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// @return True while the pop-in animation is still running (the window must keep drawing).
    pub fn is_animating(&self) -> bool {
        self.visible && now_ms() - self.shown_at < POP_DURATION_MS
    }

    /// Hides the notice.
    pub fn hide(&mut self) {
        self.visible = false;
        self.close_hovered = false;
        self.hide_at = None;
        app::state().banner_open.store(false, Ordering::Relaxed);
    }

    /// Shows a short message. It hides itself after a delay that grows with the length of the
    /// text, or when the close cross is clicked.
    ///
    /// @param text The message.
    pub fn show_message(&mut self, text: &str) {
        let delay = (2200.0 + text.chars().count() as f64 * 45.0).min(5000.0);
        if !self.visible {
            self.shown_at = now_ms();
        }
        self.visible = true;
        self.text = text.to_string();
        self.hide_at = Some(now_ms() + delay);
        app::state().banner_open.store(true, Ordering::Relaxed);
    }

    /// Hides the notice when its auto-hide time has passed. Call once per frame.
    ///
    /// @param now Current time in milliseconds.
    pub fn tick(&mut self, now: f64) {
        if let Some(hide_at) = self.hide_at {
            if now >= hide_at {
                self.hide();
            }
        }
    }

    /// Reports whether a point lies on the notice (clicks there must not reach the pet).
    ///
    /// @param x Point x in logical window pixels.
    /// @param y Point y in logical window pixels.
    /// @return True when the point is on the pill.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.visible && self.pill_area.contains(x, y)
    }

    /// Tracks whether the cursor is over the close cross, for the hover look.
    ///
    /// @param x Cursor x in logical window pixels.
    /// @param y Cursor y in logical window pixels.
    /// @return True when the hover state changed (the window should redraw).
    pub fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let hovered = self.visible && self.close_area.contains(x, y);
        let changed = hovered != self.close_hovered;
        self.close_hovered = hovered;
        changed
    }

    /// Handles a left click.
    ///
    /// @param x Click x in logical window pixels.
    /// @param y Click y in logical window pixels.
    /// @return True when the click hit the notice.
    pub fn handle_click(&mut self, x: f32, y: f32) -> bool {
        if !self.contains(x, y) {
            return false;
        }
        if self.close_area.contains(x, y) {
            self.hide();
        }
        true
    }

    /// Draws the notice centred at the top of the window and remembers where its close cross is.
    ///
    /// @param ctx The window canvas.
    /// @param window_width Window width in logical pixels.
    /// @param device_scale Physical pixels per logical pixel.
    pub fn draw(&mut self, ctx: &mut Canvas, window_width: f32, device_scale: f32) {
        if !self.visible {
            self.pill_area = Area::default();
            self.close_area = Area::default();
            return;
        }

        let text_font = FontSpec::new(500, 11.5);
        let gap = 7.0;
        let close_size = 18.0;
        // Inner padding of the pill: 11 px on the left, 8 px on the right.
        let padding = 11.0 + 8.0;
        let widest_pill = window_width - 20.0;

        let close_width = gap + close_size;
        let text_room = (widest_pill - padding - close_width).min(MAX_TEXT_WIDTH).max(0.0);
        let text = shorten_to_width(&self.text, text_font, text_room);
        let text_width = crate::render::text::measure_text(&text, text_font);

        let content_width = text_width + close_width;
        let pill_width = (padding + content_width).min(widest_pill).max(60.0);
        let pill_x = ((window_width - pill_width) / 2.0).round();
        let pill_y = TOP_MARGIN;
        self.pill_area = Area { x: pill_x, y: pill_y, width: pill_width, height: PILL_HEIGHT };

        // Pop-in: fade, slide down 6 px and grow from 95%.
        let progress = ((now_ms() - self.shown_at) / POP_DURATION_MS).clamp(0.0, 1.0) as f32;
        let eased = 1.0 - (1.0 - progress) * (1.0 - progress);
        let pop_scale = 0.95 + 0.05 * eased;
        let pop_offset = -6.0 * (1.0 - eased);
        let centre_x = pill_x + pill_width / 2.0;
        let centre_y = pill_y + PILL_HEIGHT / 2.0;

        ctx.save();
        ctx.set_transform(device_scale, 0.0, 0.0, device_scale, 0.0, 0.0);
        ctx.set_global_alpha(eased);
        ctx.translate(centre_x, centre_y + pop_offset);
        ctx.scale(pop_scale, pop_scale);
        ctx.translate(-centre_x, -centre_y);

        // Pill.
        ctx.save();
        ctx.set_shadow_color("rgba(0, 0, 0, 0.42)");
        ctx.set_shadow_blur(24.0 * device_scale);
        ctx.set_shadow_offset_y(8.0 * device_scale);
        ctx.set_fill_color("rgba(20, 22, 28, 0.92)");
        ctx.begin_path();
        ctx.round_rect(pill_x, pill_y, pill_width, PILL_HEIGHT, PILL_HEIGHT / 2.0);
        ctx.fill();
        ctx.restore();
        ctx.set_stroke_color("rgba(200, 245, 56, 0.5)");
        ctx.set_line_width(1.0);
        ctx.begin_path();
        ctx.round_rect(pill_x + 0.5, pill_y + 0.5, pill_width - 1.0, PILL_HEIGHT - 1.0, (PILL_HEIGHT - 1.0) / 2.0);
        ctx.stroke();

        // Message.
        let mut cursor_x = pill_x + 11.0;
        ctx.set_font(text_font);
        ctx.set_text_align(TextAlign::Left);
        ctx.set_text_baseline(TextBaseline::Middle);
        ctx.set_fill_color("#f4f6f8");
        ctx.fill_text(&text, cursor_x, centre_y);
        cursor_x += text_width + gap;

        // Close cross.
        let close_area = Area {
            x: cursor_x,
            y: centre_y - close_size / 2.0,
            width: close_size,
            height: close_size,
        };
        let close_hovered = self.close_hovered;
        if close_hovered {
            ctx.set_fill_color("rgba(255, 255, 255, 0.12)");
            ctx.begin_path();
            ctx.arc(close_area.x + close_size / 2.0, centre_y, close_size / 2.0, 0.0, std::f32::consts::TAU, false);
            ctx.fill();
        }
        let arm = 3.5;
        let close_centre_x = close_area.x + close_size / 2.0;
        ctx.set_stroke_color(if close_hovered { "#ffffff" } else { "rgba(230, 234, 240, 0.65)" });
        ctx.set_line_width(1.4);
        ctx.set_line_cap(crate::render::LineCap::Round);
        ctx.begin_path();
        ctx.move_to(close_centre_x - arm, centre_y - arm);
        ctx.line_to(close_centre_x + arm, centre_y + arm);
        ctx.move_to(close_centre_x + arm, centre_y - arm);
        ctx.line_to(close_centre_x - arm, centre_y + arm);
        ctx.stroke();
        self.close_area = close_area;

        ctx.restore();
    }
}

/// Shortens a text with "…" until it fits a width.
///
/// @param text The text.
/// @param font The font it is drawn with.
/// @param max_width The widest it may be.
/// @return The text, shortened when needed.
fn shorten_to_width(text: &str, font: FontSpec, max_width: f32) -> String {
    if crate::render::text::measure_text(text, font) <= max_width {
        return text.to_string();
    }
    let mut characters: Vec<char> = text.chars().collect();
    while !characters.is_empty() {
        characters.pop();
        let mut candidate: String = characters.iter().collect();
        candidate = candidate.trim_end().to_string();
        candidate.push('…');
        if crate::render::text::measure_text(&candidate, font) <= max_width {
            return candidate;
        }
    }
    "…".to_string()
}
