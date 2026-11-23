//! Messages sent from background threads to the UI thread.
//!
//! Any thread can call [`send`]. The event is queued and the UI thread is woken up with a
//! `WM_APP_EVENT` message, then drains the queue with [`drain`] on its next loop iteration.

use crate::cats::CatSkinConfig;
use crate::config::AppSettings;
use crate::input_hook::InputEvent;
use crate::updater::UpdateProgressEvent;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};

/// Window message posted to the main window whenever a new event is queued.
pub const WM_APP_EVENT: u32 = 0x8000 + 41;

/// Where the cursor is relative to the pet, computed by the hit-test thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct WindowCursorInfo {
    /// Cursor x relative to the pet area (0..1 inside the pet).
    pub rel_x: f64,
    /// Cursor y relative to the pet area (0..1 inside the pet).
    pub rel_y: f64,
    /// True when the cursor hovers over the cat's head or body.
    pub over_cat: bool,
    /// True while Alt is held, which temporarily overrides the click-through lock.
    pub alt_override: bool,
}

/// Window operations that must run on the UI thread.
#[derive(Clone, Debug)]
pub enum WindowCommand {
    /// Show the window and put it back on top.
    Show,
    /// Hide the window.
    Hide,
    /// Show the window if hidden, hide it if shown.
    ToggleVisible,
    /// Turn the always-on-top flag on or off.
    SetAlwaysOnTop(bool),
    /// Re-apply `HWND_TOPMOST` without stealing focus.
    ReassertTopmost,
    /// Resize for a new layout and scale, keeping the bottom-right corner in place.
    Resize { layout: String, scale: f64 },
    /// Move the window back to the default bottom-right corner of the screen.
    ResetPosition,
    /// Show the pet and check for updates, reporting in the update banner.
    CheckForUpdate,
}

/// Everything the UI thread can be told about.
#[derive(Clone, Debug)]
pub enum AppEvent {
    /// A global keyboard or mouse event from the input hooks.
    Input(InputEvent),
    /// The password-field guard turned on (true) or off (false).
    Privacy(bool),
    /// The cursor moved relative to the pet.
    Cursor(WindowCursorInfo),
    /// The settings changed and should be applied.
    Settings(AppSettings),
    /// The list of available cats changed.
    Cats(Vec<CatSkinConfig>),
    /// The tray icon was clicked or hovered.
    TrayIcon(tray_icon::TrayIconEvent),
    /// Progress of an update check or install.
    UpdateStatus(UpdateProgressEvent),
    /// A short message to show in the toast at the top of the pet.
    Toast(String),
    /// A window operation to run on the UI thread.
    Window(WindowCommand),
    /// Quit the application.
    Quit,
}

struct Channel {
    sender: Mutex<Sender<AppEvent>>,
    receiver: Mutex<Receiver<AppEvent>>,
}

static CHANNEL: OnceLock<Channel> = OnceLock::new();
static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);

/// Returns the global event channel, creating it on first use.
///
/// @return The process-wide channel.
fn channel() -> &'static Channel {
    CHANNEL.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        Channel {
            sender: Mutex::new(sender),
            receiver: Mutex::new(receiver),
        }
    })
}

/// Registers the main window so that [`send`] can wake it up.
///
/// @param hwnd The main window handle, as an integer.
pub fn set_main_window(hwnd: isize) {
    MAIN_HWND.store(hwnd, Ordering::SeqCst);
}

/// Returns the main window handle, or 0 if the window does not exist yet.
///
/// @return The main window handle as an integer.
pub fn main_window() -> isize {
    MAIN_HWND.load(Ordering::SeqCst)
}

/// Queues an event for the UI thread and wakes the UI thread up. Safe to call from any thread.
///
/// @param event The event to deliver.
pub fn send(event: AppEvent) {
    if let Ok(sender) = channel().sender.lock() {
        let _ = sender.send(event);
    }
    let hwnd = main_window();
    if hwnd != 0 {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                hwnd as windows_sys::Win32::Foundation::HWND,
                WM_APP_EVENT,
                0,
                0,
            );
        }
    }
}

/// Takes all queued events. Called by the UI thread only.
///
/// @return The queued events, oldest first.
pub fn drain() -> Vec<AppEvent> {
    let mut events = Vec::new();
    if let Ok(receiver) = channel().receiver.lock() {
        while let Ok(event) = receiver.try_recv() {
            events.push(event);
        }
    }
    events
}
