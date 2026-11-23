//! The pill-shaped notice at the top of the pet window: update progress, "copied" confirmations,
//! click-through lock messages and so on. It is drawn on the same canvas as the pet and has up
//! to three buttons: "Update now", "Release page" and a close cross.

use crate::app;
use crate::engine::now_ms;
use crate::render::{Canvas, FontSpec, TextAlign, TextBaseline};
use crate::updater::{self, UpdateInfo, UpdateProgressEvent};
use std::sync::atomic::Ordering;

/// Release page used when no release URL is known yet.
const FALLBACK_RELEASE_URL: &str = "https://github.com/qianyiai/kurocat/releases/latest";
/// Widest the message text may get before it is shortened with "…".
const MAX_TEXT_WIDTH: f32 = 268.0;
/// Narrowest message width before the secondary button is dropped, in logical pixels.
const MIN_TEXT_WIDTH: f32 = 90.0;
/// Distance of the pill from the top of the window.
const TOP_MARGIN: f32 = 10.0;
/// Height of the pill.
const PILL_HEIGHT: f32 = 28.0;
/// Length of the pop-in animation in milliseconds.
const POP_DURATION_MS: f64 = 200.0;

/// A button inside the notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BannerButton {
    /// Download and install the update.
    Install,
    /// Open the release page in the browser.
    OpenRelease,
    /// Hide the notice.
    Close,
}

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

/// Options for [`Banner::show`].
#[derive(Clone, Copy, Debug, Default)]
struct ShowOptions {
    show_install: bool,
    show_open: bool,
    busy: bool,
    auto_hide_ms: Option<f64>,
}

/// The notice and the update state behind it.
pub struct Banner {
    visible: bool,
    text: String,
    show_install: bool,
    show_open: bool,
    busy: bool,
    shown_at: f64,
    hide_at: Option<f64>,
    hovered: Option<BannerButton>,
    pill_area: Area,
    button_areas: Vec<(BannerButton, Area)>,

    latest_info: Option<UpdateInfo>,
    update_busy: bool,
    dismissed_version: Option<String>,
    manual_check: bool,
}

impl Banner {
    /// @return A hidden notice with no update known.
    pub fn new() -> Banner {
        Banner {
            visible: false,
            text: String::new(),
            show_install: false,
            show_open: false,
            busy: false,
            shown_at: 0.0,
            hide_at: None,
            hovered: None,
            pill_area: Area::default(),
            button_areas: Vec::new(),
            latest_info: None,
            update_busy: false,
            dismissed_version: None,
            manual_check: false,
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

    /// Shows the notice.
    ///
    /// @param text The message.
    /// @param options Buttons, busy state and auto-hide delay.
    fn show(&mut self, text: &str, options: ShowOptions) {
        let was_visible = self.visible;
        self.visible = true;
        self.text = text.to_string();
        self.show_install = options.show_install;
        self.show_open = options.show_open;
        self.busy = options.busy;
        if !was_visible {
            self.shown_at = now_ms();
        }
        self.hide_at = match options.auto_hide_ms {
            Some(delay) if delay > 0.0 => Some(now_ms() + delay),
            _ => None,
        };
        app::state().update_banner_open.store(true, Ordering::Relaxed);
    }

    /// Hides the notice.
    pub fn hide(&mut self) {
        self.visible = false;
        self.hovered = None;
        self.hide_at = None;
        app::state().update_banner_open.store(false, Ordering::Relaxed);
    }

    /// Shows a short message without buttons. It hides itself after a delay that grows with
    /// the length of the text.
    ///
    /// @param text The message.
    pub fn show_message(&mut self, text: &str) {
        let delay = (2200.0 + text.chars().count() as f64 * 45.0).min(5000.0);
        self.show(
            text,
            ShowOptions {
                auto_hide_ms: Some(delay),
                ..ShowOptions::default()
            },
        );
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

    /// Updates the notice for an update-progress event from the updater thread.
    ///
    /// @param event The progress event.
    pub fn handle_update_progress(&mut self, event: &UpdateProgressEvent) {
        let from_manual = self.manual_check;
        if let Some(info) = &event.info {
            self.latest_info = Some(info.clone());
        }
        let current_version = match &self.latest_info {
            Some(info) => info.current_version.clone(),
            None => updater::CURRENT_VERSION.to_string(),
        };

        match event.stage.as_str() {
            "checking" => {
                self.update_busy = true;
                self.show("Checking for updates…", ShowOptions { busy: true, ..ShowOptions::default() });
            }
            "available" => {
                self.update_busy = false;
                self.manual_check = false;
                let Some(info) = self.latest_info.clone() else {
                    return;
                };
                if !info.has_update {
                    return;
                }
                let dismissed = self.dismissed_version.as_deref() == Some(info.latest_version.as_str());
                if from_manual || !dismissed {
                    let has_download = info.download_url.is_some();
                    self.show(
                        &format!("New version v{} available", info.latest_version),
                        ShowOptions {
                            show_install: has_download,
                            show_open: !has_download,
                            ..ShowOptions::default()
                        },
                    );
                }
            }
            "up_to_date" => {
                self.update_busy = false;
                self.manual_check = false;
                self.show(
                    &format!("✓ You have the latest version v{}", current_version),
                    ShowOptions { auto_hide_ms: Some(2600.0), ..ShowOptions::default() },
                );
            }
            "downloading" | "verifying" | "installing" => {
                self.update_busy = true;
                self.show(&event.message, ShowOptions { busy: true, ..ShowOptions::default() });
            }
            "error" => {
                self.update_busy = false;
                self.manual_check = false;
                self.show(
                    &format!("⚠ {}", event.message),
                    ShowOptions { show_open: true, auto_hide_ms: Some(5200.0), ..ShowOptions::default() },
                );
            }
            _ => {}
        }
    }

    /// Starts a manual update check from the menu. When an update is already known, it starts
    /// installing it instead.
    pub fn start_manual_check(&mut self) {
        if self.update_busy {
            return;
        }
        if let Some(info) = &self.latest_info {
            if info.has_update && info.download_url.is_some() {
                self.start_install();
                return;
            }
        }
        self.update_busy = true;
        self.manual_check = true;
        self.show("Checking for updates…", ShowOptions { busy: true, ..ShowOptions::default() });
        std::thread::spawn(|| match updater::check_latest_update() {
            Ok(info) => {
                if info.has_update {
                    let message = format!("New version v{} available", info.latest_version);
                    updater::emit_progress("available", message, Some(info));
                } else {
                    let message = format!("You have the latest version v{}", info.current_version);
                    updater::emit_progress("up_to_date", message, Some(info));
                }
            }
            Err(error) => updater::emit_progress("error", error, None),
        });
    }

    /// Downloads and installs the known update on a background thread.
    fn start_install(&mut self) {
        if self.update_busy {
            return;
        }
        let Some(info) = self.latest_info.clone() else {
            return;
        };
        if !info.has_update {
            return;
        }
        self.update_busy = true;
        self.show(
            &format!("Downloading v{}…", info.latest_version),
            ShowOptions { busy: true, ..ShowOptions::default() },
        );
        std::thread::spawn(move || {
            if let Err(error) = updater::perform_update_install(info) {
                updater::emit_progress("error", error, None);
            }
        });
    }

    /// Finds the button under a point.
    ///
    /// @param x Point x in logical window pixels.
    /// @param y Point y in logical window pixels.
    /// @return The button, or None.
    fn button_at(&self, x: f32, y: f32) -> Option<BannerButton> {
        if !self.visible {
            return None;
        }
        for (button, area) in &self.button_areas {
            if area.contains(x, y) {
                return Some(*button);
            }
        }
        None
    }

    /// Reports whether a point lies on the notice (clicks there must not reach the pet).
    ///
    /// @param x Point x in logical window pixels.
    /// @param y Point y in logical window pixels.
    /// @return True when the point is on the pill.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        self.visible && self.pill_area.contains(x, y)
    }

    /// Tracks which button the cursor is over, for the hover look.
    ///
    /// @param x Cursor x in logical window pixels.
    /// @param y Cursor y in logical window pixels.
    /// @return True when the hovered button changed (the window should redraw).
    pub fn update_hover(&mut self, x: f32, y: f32) -> bool {
        let hovered = self.button_at(x, y);
        let changed = hovered != self.hovered;
        self.hovered = hovered;
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
        match self.button_at(x, y) {
            Some(BannerButton::Install) => {
                if !self.busy {
                    self.start_install();
                }
            }
            Some(BannerButton::OpenRelease) => {
                let url = match &self.latest_info {
                    Some(info) if !info.release_url.is_empty() => info.release_url.clone(),
                    _ => FALLBACK_RELEASE_URL.to_string(),
                };
                updater::open_url_in_browser(&url);
            }
            Some(BannerButton::Close) => {
                if let Some(info) = &self.latest_info {
                    if info.has_update {
                        self.dismissed_version = Some(info.latest_version.clone());
                    }
                }
                self.hide();
            }
            None => {}
        }
        true
    }

    /// Draws the notice centred at the top of the window and remembers where its buttons are.
    ///
    /// @param ctx The window canvas.
    /// @param window_width Window width in logical pixels.
    /// @param device_scale Physical pixels per logical pixel.
    pub fn draw(&mut self, ctx: &mut Canvas, window_width: f32, device_scale: f32) {
        self.button_areas.clear();
        if !self.visible {
            self.pill_area = Area::default();
            return;
        }

        let text_font = FontSpec::new(500, 11.5);
        let button_font = FontSpec::new(700, 11.0);
        let gap = 7.0;
        let button_height = 19.0;
        let close_size = 18.0;
        // Inner padding of the pill: 11 px on the left, 8 px on the right.
        let padding = 11.0 + 8.0;
        let widest_pill = window_width - 20.0;

        let mut buttons: Vec<(BannerButton, &str)> = Vec::new();
        if self.show_install {
            buttons.push((BannerButton::Install, "Update now"));
        }
        if self.show_open {
            buttons.push((BannerButton::OpenRelease, "Release page"));
        }

        // The buttons and the close cross always fit; the message gets the room that is left.
        // In a small window the secondary "Release page" button is dropped to keep the message
        // readable.
        let mut button_widths: Vec<f32> = Vec::new();
        let mut buttons_width = gap + close_size;
        for (_, label) in &buttons {
            let width = crate::render::text::measure_text(label, button_font) + 18.0;
            button_widths.push(width);
            buttons_width += gap + width;
        }
        if widest_pill - padding - buttons_width < MIN_TEXT_WIDTH {
            let mut release_index = None;
            for (index, (button, _)) in buttons.iter().enumerate() {
                if *button == BannerButton::OpenRelease {
                    release_index = Some(index);
                }
            }
            if let Some(index) = release_index {
                buttons_width -= gap + button_widths[index];
                buttons.remove(index);
                button_widths.remove(index);
            }
        }
        let text_room = (widest_pill - padding - buttons_width).min(MAX_TEXT_WIDTH).max(0.0);
        let text = shorten_to_width(&self.text, text_font, text_room);
        let text_width = crate::render::text::measure_text(&text, text_font);

        let content_width = text_width + buttons_width;
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

        // Buttons.
        ctx.set_font(button_font);
        for (index, (button, label)) in buttons.iter().enumerate() {
            let width = button_widths[index];
            let area = Area {
                x: cursor_x,
                y: centre_y - button_height / 2.0,
                width,
                height: button_height,
            };
            let hovered = self.hovered == Some(*button) && !self.busy;
            let lift = if hovered { -0.5 } else { 0.0 };
            ctx.save();
            if self.busy {
                ctx.set_global_alpha(ctx.global_alpha() * 0.65);
            }
            if *button == BannerButton::Install {
                ctx.set_fill_color(if hovered { "#d2fa4a" } else { "#c8f538" });
            } else {
                ctx.set_fill_color(if hovered { "rgba(255, 255, 255, 0.2)" } else { "rgba(255, 255, 255, 0.14)" });
            }
            ctx.begin_path();
            ctx.round_rect(area.x, area.y + lift, area.width, area.height, area.height / 2.0);
            ctx.fill();
            ctx.set_text_align(TextAlign::Center);
            ctx.set_fill_color(if *button == BannerButton::Install { "#111214" } else { "#f3f4f6" });
            ctx.fill_text(label, area.x + area.width / 2.0, centre_y + lift);
            ctx.restore();
            self.button_areas.push((*button, area));
            cursor_x += width + gap;
        }

        // Close cross.
        let close_area = Area {
            x: cursor_x,
            y: centre_y - close_size / 2.0,
            width: close_size,
            height: close_size,
        };
        let close_hovered = self.hovered == Some(BannerButton::Close);
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
        self.button_areas.push((BannerButton::Close, close_area));

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
