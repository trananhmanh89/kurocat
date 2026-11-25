//! Application start-up, shared state and the settings actions used by the menus, tray and MCP.

use crate::cats::{all_cats, cat_exists, cats_path, sanitize_cat_id};
use crate::config::{
    config_path, load_settings, normalize_keyboard_layout, normalize_keyboard_orientation,
    save_settings, set_windows_autostart, try_load_settings, AppSettings,
};
use crate::events::{self, AppEvent, WindowCommand};
use crate::input_hook::InputEvent;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime};

/// Logical size of the pet with the 65% keyboard, at 100% scale.
pub const BASE_WIDTH: f64 = 420.0;
/// Logical height of the pet with the 65% keyboard, at 100% scale.
pub const BASE_HEIGHT: f64 = 440.0;
/// Logical width of the pet with the 108-key keyboard, at 100% scale.
pub const BASE_WIDTH_108: f64 = 540.0;
/// Logical height of the pet with the 108-key keyboard, at 100% scale.
pub const BASE_HEIGHT_108: f64 = 456.0;
/// The window never gets narrower than this, so the toast always fits.
pub const MIN_WINDOW_WIDTH: f64 = 260.0;
/// The window never gets shorter than this, so the toast always fits.
pub const MIN_WINDOW_HEIGHT: f64 = 410.0;
/// Smallest allowed scale.
pub const MIN_SCALE: f64 = 0.20;
/// Largest allowed scale.
pub const MAX_SCALE: f64 = 1.50;

/// State shared between the UI thread and the background threads.
pub struct AppState {
    /// The current settings. Saved to disk on every change.
    pub settings: Mutex<AppSettings>,
    /// True while the context menu or tray menu is open.
    pub menu_open: AtomicBool,
    /// True while the user drags the window.
    pub dragging: AtomicBool,
    /// True while the MCP HTTP server should answer requests.
    pub mcp_enabled: AtomicBool,
    /// True while the toast banner is shown (its close cross stays clickable).
    pub banner_open: AtomicBool,
    /// True while input listening is paused and the pet is hidden (anti-cheat mode).
    pub paused_guard: AtomicBool,
}

static STATE: OnceLock<AppState> = OnceLock::new();

/// Returns the shared application state.
///
/// @return The process-wide state. Panics if called before [`run`] initialised it.
pub fn state() -> &'static AppState {
    STATE.get().expect("app state is not initialised")
}

/// Returns a copy of the current settings.
///
/// @return The settings.
pub fn settings() -> AppSettings {
    state().settings.lock().unwrap().clone()
}

/// Returns the logical size of the pet for a keyboard layout at 100% scale.
///
/// @param layout "65" or "108".
/// @return (width, height) in logical pixels.
pub fn base_dimensions(layout: &str) -> (f64, f64) {
    if layout == "108" {
        (BASE_WIDTH_108, BASE_HEIGHT_108)
    } else {
        (BASE_WIDTH, BASE_HEIGHT)
    }
}

/// Returns the logical window size for a layout and scale, respecting the minimum size.
///
/// @param layout "65" or "108".
/// @param scale The pet scale (0.2 to 1.5).
/// @return (width, height) in logical pixels.
pub fn window_dimensions(layout: &str, scale: f64) -> (f64, f64) {
    let (base_width, base_height) = base_dimensions(layout);
    let clamped = scale.clamp(MIN_SCALE, MAX_SCALE);
    let width = (base_width * clamped).round().max(MIN_WINDOW_WIDTH);
    let height = (base_height * clamped).round().max(MIN_WINDOW_HEIGHT);
    (width, height)
}

/// Builds the MCP client configuration JSON that the user can paste into an AI client.
///
/// @return Pretty-printed JSON text.
pub fn mcp_config_json() -> String {
    let port = settings().mcp_port;
    let exe = match std::env::current_exe() {
        Ok(path) => path.to_string_lossy().to_string(),
        Err(_) => "KuroCat.exe".to_string(),
    };
    let config = serde_json::json!({
        "mcpServers": {
            "kurocat": {
                "command": exe,
                "args": ["--mcp"],
                "url": format!("http://127.0.0.1:{}/mcp", port)
            }
        }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

/// Saves new settings, applies their side effects (autostart, topmost, window size) and
/// tells the UI thread about them.
///
/// @param patch The complete new settings.
/// @return The settings after cleaning up out-of-range values.
pub fn update_settings(mut patch: AppSettings) -> AppSettings {
    patch.scale = patch.scale.clamp(MIN_SCALE, MAX_SCALE);
    let clean_breed = sanitize_cat_id(&patch.cat_breed);
    patch.cat_breed = if cat_exists(&clean_breed) {
        clean_breed
    } else {
        "black".to_string()
    };
    if patch.mcp_port == 0 {
        patch.mcp_port = 27420;
    }
    patch.keyboard_layout = normalize_keyboard_layout(&patch.keyboard_layout);
    patch.keyboard_orientation = normalize_keyboard_orientation(&patch.keyboard_orientation);

    let previous = settings();
    if patch.autostart != previous.autostart {
        if set_windows_autostart(patch.autostart).is_err() {
            // Keep the menu honest: the registry was not changed.
            patch.autostart = previous.autostart;
        }
    }
    state().mcp_enabled.store(patch.mcp_enabled, Ordering::Relaxed);

    if patch.always_on_top != previous.always_on_top {
        events::send(AppEvent::Window(WindowCommand::SetAlwaysOnTop(patch.always_on_top)));
    }
    let size_changed = (patch.scale - previous.scale).abs() > 0.005
        || patch.keyboard_layout != previous.keyboard_layout;

    {
        let mut current = state().settings.lock().unwrap();
        *current = patch.clone();
        save_settings(&current);
    }
    if size_changed {
        events::send(AppEvent::Window(WindowCommand::Resize {
            layout: patch.keyboard_layout.clone(),
            scale: patch.scale,
        }));
    }
    events::send(AppEvent::Settings(patch.clone()));
    patch
}

/// Changes one or more settings fields, then saves and applies them.
///
/// @param change A function that edits the settings in place.
/// @return The settings after the change.
pub fn change_settings(change: impl FnOnce(&mut AppSettings)) -> AppSettings {
    let mut next = settings();
    change(&mut next);
    update_settings(next)
}

/// Switches the visible cat. Unknown ids fall back to the black cat.
///
/// @param breed The cat id.
pub fn apply_breed_and_save(breed: &str) {
    let breed = breed.to_string();
    change_settings(move |s| s.cat_breed = breed);
}

/// Changes the pet scale.
///
/// @param scale The new scale (clamped to 0.2..1.5).
pub fn apply_scale_and_save(scale: f64) {
    change_settings(move |s| s.scale = scale);
}

/// Switches between the 65% and 108-key keyboard.
///
/// @param layout "65" or "108".
pub fn apply_keyboard_layout_and_save(layout: &str) {
    let layout = layout.to_string();
    change_settings(move |s| s.keyboard_layout = layout);
}

/// Switches the keyboard between facing the cat and facing the viewer.
///
/// @param orientation "cat" or "viewer".
pub fn apply_keyboard_orientation_and_save(orientation: &str) {
    let orientation = orientation.to_string();
    change_settings(move |s| s.keyboard_orientation = orientation);
}

/// Stores the window position after the user moved it.
///
/// @param x Left edge in physical screen pixels.
/// @param y Top edge in physical screen pixels.
pub fn save_window_position(x: i32, y: i32) {
    let mut current = state().settings.lock().unwrap();
    current.x = Some(x);
    current.y = Some(y);
    save_settings(&current);
}

/// Re-reads settings and cats from disk (after an MCP client or the user edited them) and
/// applies them.
pub fn reload_and_emit_all() {
    // Read and replace under the lock, so a change saved meanwhile is not overwritten with
    // older contents. A file that cannot be read keeps the settings in memory.
    let (previous, disk_settings) = {
        let mut current = state().settings.lock().unwrap();
        let previous = current.clone();
        let disk_settings = match try_load_settings() {
            Some(loaded) => loaded,
            None => previous.clone(),
        };
        *current = disk_settings.clone();
        (previous, disk_settings)
    };
    let size_changed = (previous.scale - disk_settings.scale).abs() > 0.01
        || previous.keyboard_layout != disk_settings.keyboard_layout;
    let top_changed = previous.always_on_top != disk_settings.always_on_top;
    state().mcp_enabled.store(disk_settings.mcp_enabled, Ordering::Relaxed);

    if top_changed {
        events::send(AppEvent::Window(WindowCommand::SetAlwaysOnTop(disk_settings.always_on_top)));
    }
    if size_changed {
        events::send(AppEvent::Window(WindowCommand::Resize {
            layout: disk_settings.keyboard_layout.clone(),
            scale: disk_settings.scale,
        }));
    }
    events::send(AppEvent::Cats(all_cats()));
    events::send(AppEvent::Settings(disk_settings));
}

/// Returns a file's modification time, if the file exists.
///
/// @param path The file to check.
/// @return The modification time, or None.
fn file_modified_time(path: &std::path::Path) -> Option<SystemTime> {
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.modified().ok(),
        Err(_) => None,
    }
}

/// Starts a thread that reloads everything when `cats.json` or the settings file changes on disk.
/// This is how changes made by `KuroCat.exe --mcp` (a separate process) reach the running pet.
fn start_storage_watcher() {
    std::thread::spawn(move || {
        let cats_file = cats_path();
        let settings_file = config_path();
        let mut last_cats_time = file_modified_time(&cats_file);
        let mut last_settings_time = file_modified_time(&settings_file);
        loop {
            std::thread::sleep(Duration::from_millis(400));
            let cats_time = file_modified_time(&cats_file);
            let settings_time = file_modified_time(&settings_file);
            if cats_time != last_cats_time || settings_time != last_settings_time {
                last_cats_time = cats_time;
                last_settings_time = settings_time;
                reload_and_emit_all();
            }
        }
    });
}

/// When started with `--wait-pid <pid>` (after "restart as administrator"), waits
/// for the previous process to exit so two pets never run at once.
fn wait_for_previous_process_if_requested() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const PROCESS_SYNCHRONIZE: u32 = 0x0010_0000;
    let mut args = std::env::args();
    while let Some(arg) = args.next() {
        if arg != "--wait-pid" {
            continue;
        }
        if let Some(pid_text) = args.next() {
            if let Ok(pid) = pid_text.parse::<u32>() {
                if pid != 0 && pid != std::process::id() {
                    unsafe {
                        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
                        if !handle.is_null() {
                            let _ = WaitForSingleObject(handle, 4000);
                            let _ = CloseHandle(handle);
                        }
                    }
                    std::thread::sleep(Duration::from_millis(150));
                }
            }
        }
        break;
    }
}

/// Starts a thread that sends a short scripted sequence of fake input (for `--demo`).
fn start_demo_input() {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1200));
        let keys: [u32; 7] = [0x46, 0x53, 0x41, 0x45, 0x20, 0xA0, 0x4B];
        for vk in keys {
            events::send(AppEvent::Input(InputEvent::KeyDown { vk, scan: 0, repeat: false }));
            std::thread::sleep(Duration::from_millis(100));
            events::send(AppEvent::Input(InputEvent::KeyUp { vk, scan: 0 }));
        }
        events::send(AppEvent::Input(InputEvent::MouseMove { x: 400, y: 400, nx: 0.25, ny: 0.4 }));
        events::send(AppEvent::Input(InputEvent::MouseDown { button: "left".to_string() }));
        events::send(AppEvent::Input(InputEvent::KeyDown { vk: 0x4B, scan: 0, repeat: false }));
    });
}

/// Runs the desktop pet until the user quits.
pub fn run() {
    wait_for_previous_process_if_requested();

    let mut initial_settings = load_settings();
    // Always start unlocked, so a user who locked click-through by accident can never be trapped.
    if initial_settings.click_through_locked {
        initial_settings.click_through_locked = false;
        save_settings(&initial_settings);
    }
    let _ = STATE.set(AppState {
        settings: Mutex::new(initial_settings.clone()),
        menu_open: AtomicBool::new(false),
        dragging: AtomicBool::new(false),
        mcp_enabled: AtomicBool::new(initial_settings.mcp_enabled),
        banner_open: AtomicBool::new(false),
        paused_guard: AtomicBool::new(false),
    });

    let mut pet_window = match crate::window::PetWindow::create(&initial_settings) {
        Ok(window) => window,
        Err(message) => {
            eprintln!("KuroCat could not create its window: {}", message);
            return;
        }
    };

    let tray = crate::tray::Tray::create(&initial_settings);

    crate::input_hook::start_global_hooks();
    crate::privacy::start_password_privacy_watcher();
    crate::hit_test::start_hit_test_loop();
    start_storage_watcher();

    let mcp_port = if initial_settings.mcp_port == 0 { 27420 } else { initial_settings.mcp_port };
    crate::mcp::start_http_mcp_server(mcp_port, reload_and_emit_all);

    if std::env::args().any(|arg| arg == "--demo") {
        start_demo_input();
    }

    pet_window.run(tray);
}
