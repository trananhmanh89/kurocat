//! Notification-area (tray) icon and its native menu.
//!
//! The tray lives on the UI thread. The window loop forwards muda menu events whose id starts
//! with `tray:` to [`Tray::handle_menu_event`], tray icon events to [`Tray::handle_icon_event`],
//! and calls [`Tray::sync`] / [`Tray::rebuild_cats`] whenever settings or cats change.

use crate::app;
use crate::cats::CatSkinConfig;
use crate::config::AppSettings;
use crate::events::{self, AppEvent, WindowCommand};
use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use std::sync::atomic::Ordering;
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// Prefix of every tray menu id.
pub const ID_PREFIX: &str = "tray:";

const ID_TOGGLE_SHOW: &str = "tray:toggle_show";
const ID_PAUSE_GUARD: &str = "tray:pause_guard";
const ID_RESTART_ADMIN: &str = "tray:restart_as_admin";
const ID_CLICK_THROUGH: &str = "tray:click_through";
const ID_PRIVACY_GUARD: &str = "tray:privacy_guard";
const ID_KEY_BUBBLES: &str = "tray:key_bubbles";
const ID_SOUND: &str = "tray:sound_toggle";
const ID_ALWAYS_ON_TOP: &str = "tray:always_on_top";
const ID_KB_65: &str = "tray:kb_65";
const ID_KB_108: &str = "tray:kb_108";
const ID_KB_CAT: &str = "tray:kb_cat";
const ID_KB_VIEWER: &str = "tray:kb_viewer";
const ID_DESKTOP_SHORTCUT: &str = "tray:desktop_shortcut";
const ID_RESET_POS: &str = "tray:reset_pos";
const ID_AUTOSTART: &str = "tray:autostart";
const ID_MCP: &str = "tray:mcp_toggle";
const ID_CHECK_UPDATE: &str = "tray:check_update";
const ID_QUIT: &str = "tray:quit";
const BREED_ID_PREFIX: &str = "tray:breed:";
const SCALE_ID_PREFIX: &str = "tray:scale:";

/// Scale choices in percent, with their menu labels.
const SCALE_CHOICES: [(u32, &str); 7] = [
    (20, "20% (Tiny)"),
    (25, "25% (Mini)"),
    (50, "50% (Small)"),
    (75, "75% (Medium)"),
    (100, "100% (Default)"),
    (125, "125% (Large)"),
    (150, "150% (Extra large)"),
];

/// The tray icon and the menu items whose check marks follow the settings.
pub struct Tray {
    icon: TrayIcon,
    pause_guard: CheckMenuItem,
    click_through: CheckMenuItem,
    privacy_guard: CheckMenuItem,
    key_bubbles: CheckMenuItem,
    sound: CheckMenuItem,
    always_on_top: CheckMenuItem,
    autostart: CheckMenuItem,
    mcp: CheckMenuItem,
    kb_65: CheckMenuItem,
    kb_108: CheckMenuItem,
    kb_cat: CheckMenuItem,
    kb_viewer: CheckMenuItem,
    breed_menu: Submenu,
    breed_items: Vec<(String, CheckMenuItem)>,
    scale_items: Vec<(u32, CheckMenuItem)>,
}

/// Tooltip shown while listening is active.
///
/// @return "KuroCat", with a note when running as administrator.
fn default_tooltip() -> &'static str {
    if win_is_elevated() {
        "KuroCat (Administrator - game input ready)"
    } else {
        "KuroCat"
    }
}

/// Tooltip shown while listening is paused.
///
/// @return The paused tooltip text.
fn paused_tooltip() -> &'static str {
    "KuroCat (Listening paused - anti-cheat safe)"
}

/// Decodes the embedded tray PNG into a tray icon.
///
/// @return The icon, or None if decoding failed.
fn load_tray_icon() -> Option<tray_icon::Icon> {
    let pixmap = tiny_skia::Pixmap::decode_png(include_bytes!("../assets/icons/tray-icon.png")).ok()?;
    let mut rgba: Vec<u8> = Vec::with_capacity(pixmap.pixels().len() * 4);
    for pixel in pixmap.pixels() {
        let color = pixel.demultiply();
        rgba.push(color.red());
        rgba.push(color.green());
        rgba.push(color.blue());
        rgba.push(color.alpha());
    }
    tray_icon::Icon::from_rgba(rgba, pixmap.width(), pixmap.height()).ok()
}

/// Creates a check menu item without a keyboard accelerator.
///
/// @param id The menu id.
/// @param text The label.
/// @param checked The initial check state.
/// @return The new item.
fn check_item(id: &str, text: &str, checked: bool) -> CheckMenuItem {
    CheckMenuItem::with_id(id, text, true, checked, None)
}

/// Creates a plain menu item without a keyboard accelerator.
///
/// @param id The menu id.
/// @param text The label.
/// @param enabled Whether the item can be clicked.
/// @return The new item.
fn plain_item(id: &str, text: &str, enabled: bool) -> MenuItem {
    MenuItem::with_id(id, text, enabled, None)
}

/// Label of a cat in the appearance menu.
///
/// @param cat The cat.
/// @return The name, with the cat's emoji in front if it has one.
fn cat_label(cat: &CatSkinConfig) -> String {
    match cat.emoji.as_deref() {
        Some(emoji) if !emoji.trim().is_empty() => format!("{} {}", emoji.trim(), cat.name),
        _ => cat.name.clone(),
    }
}

impl Tray {
    /// Creates the tray icon and its menu. Must be called on the UI thread.
    ///
    /// @param initial The settings used for the initial check marks.
    /// @return The tray, or None if the icon could not be created.
    pub fn create(initial: &AppSettings) -> Option<Tray> {
        let elevated = win_is_elevated();

        let toggle_show = plain_item(ID_TOGGLE_SHOW, "Show / hide KuroCat", true);
        let pause_guard = check_item(
            ID_PAUSE_GUARD,
            "Pause listening and hide (anti-cheat safe)",
            app::state().paused_guard.load(Ordering::SeqCst),
        );
        let restart_label = if elevated {
            "Running as administrator (game input ready)"
        } else {
            "Restart as administrator (capture input in games)"
        };
        let restart_as_admin = plain_item(ID_RESTART_ADMIN, restart_label, !elevated);
        let click_through = check_item(
            ID_CLICK_THROUGH,
            "Lock click-through (hold Alt to interact)",
            initial.click_through_locked,
        );
        let privacy_guard = check_item(
            ID_PRIVACY_GUARD,
            "Close eyes while typing passwords",
            initial.privacy_password_guard,
        );
        let key_bubbles = check_item(ID_KEY_BUBBLES, "Show key bubbles", initial.show_key_bubbles);
        let sound = check_item(ID_SOUND, "Purr when petted", initial.sound_enabled);
        let always_on_top = check_item(ID_ALWAYS_ON_TOP, "Always on top", initial.always_on_top);

        let breed_menu = Submenu::new("Appearance", true);

        let kb_65 = check_item(ID_KB_65, "65% compact keyboard", initial.keyboard_layout == "65");
        let kb_108 = check_item(
            ID_KB_108,
            "108-key full size (with numpad)",
            initial.keyboard_layout == "108",
        );
        let kb_separator = PredefinedMenuItem::separator();
        let kb_cat = check_item(
            ID_KB_CAT,
            "Facing the cat (realistic 180° view)",
            initial.keyboard_orientation == "cat",
        );
        let kb_viewer = check_item(
            ID_KB_VIEWER,
            "Facing the viewer (readable legends)",
            initial.keyboard_orientation == "viewer",
        );
        let keyboard_menu = Submenu::new("Keyboard layout and direction", true);
        let _ = keyboard_menu.append(&kb_65);
        let _ = keyboard_menu.append(&kb_108);
        let _ = keyboard_menu.append(&kb_separator);
        let _ = keyboard_menu.append(&kb_cat);
        let _ = keyboard_menu.append(&kb_viewer);

        let scale_menu = Submenu::new("Size", true);
        let mut scale_items: Vec<(u32, CheckMenuItem)> = Vec::new();
        for (percent, label) in SCALE_CHOICES {
            let id = format!("{}{}", SCALE_ID_PREFIX, percent);
            let item = check_item(&id, label, false);
            let _ = scale_menu.append(&item);
            scale_items.push((percent, item));
        }

        let desktop_shortcut = plain_item(ID_DESKTOP_SHORTCUT, "Create desktop shortcut (KuroCat)", true);
        let reset_pos = plain_item(ID_RESET_POS, "Move back to the bottom-right corner", true);
        let autostart = check_item(ID_AUTOSTART, "Start with Windows", initial.autostart);
        let mcp = check_item(ID_MCP, "Enable MCP extension server", initial.mcp_enabled);
        let update_label = format!("Check for updates (v{})", crate::updater::CURRENT_VERSION);
        let check_update = plain_item(ID_CHECK_UPDATE, &update_label, true);
        let quit = plain_item(ID_QUIT, "Quit KuroCat", true);
        let separator_top = PredefinedMenuItem::separator();
        let separator_bottom = PredefinedMenuItem::separator();

        let menu = Menu::with_items(&[
            &toggle_show,
            &pause_guard,
            &restart_as_admin,
            &separator_top,
            &click_through,
            &privacy_guard,
            &key_bubbles,
            &sound,
            &always_on_top,
            &breed_menu,
            &keyboard_menu,
            &scale_menu,
            &desktop_shortcut,
            &reset_pos,
            &autostart,
            &mcp,
            &check_update,
            &separator_bottom,
            &quit,
        ])
        .ok()?;

        let mut builder = TrayIconBuilder::new()
            .with_id("kuro-tray")
            .with_tooltip(default_tooltip())
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false);
        if let Some(icon) = load_tray_icon() {
            builder = builder.with_icon(icon);
        }
        let icon = builder.build().ok()?;

        let mut tray = Tray {
            icon,
            pause_guard,
            click_through,
            privacy_guard,
            key_bubbles,
            sound,
            always_on_top,
            autostart,
            mcp,
            kb_65,
            kb_108,
            kb_cat,
            kb_viewer,
            breed_menu,
            breed_items: Vec::new(),
            scale_items,
        };
        tray.rebuild_cats(&crate::cats::all_cats(), &initial.cat_breed);
        tray.sync(initial);
        Some(tray)
    }

    /// Updates every check mark to match the settings.
    ///
    /// @param settings The current settings.
    pub fn sync(&self, settings: &AppSettings) {
        self.pause_guard.set_checked(app::state().paused_guard.load(Ordering::SeqCst));
        self.click_through.set_checked(settings.click_through_locked);
        self.privacy_guard.set_checked(settings.privacy_password_guard);
        self.key_bubbles.set_checked(settings.show_key_bubbles);
        self.sound.set_checked(settings.sound_enabled);
        self.always_on_top.set_checked(settings.always_on_top);
        self.autostart.set_checked(settings.autostart);
        self.mcp.set_checked(settings.mcp_enabled);
        self.kb_65.set_checked(settings.keyboard_layout != "108");
        self.kb_108.set_checked(settings.keyboard_layout == "108");
        self.kb_cat.set_checked(settings.keyboard_orientation != "viewer");
        self.kb_viewer.set_checked(settings.keyboard_orientation == "viewer");

        for (cat_id, item) in &self.breed_items {
            item.set_checked(*cat_id == settings.cat_breed);
        }
        let current_percent = (settings.scale * 100.0).round() as u32;
        for (percent, item) in &self.scale_items {
            item.set_checked(*percent == current_percent);
        }
    }

    /// Replaces the cats in the appearance submenu.
    ///
    /// @param cats All cats, built-in and custom.
    /// @param active Id of the visible cat (gets the check mark).
    pub fn rebuild_cats(&mut self, cats: &[CatSkinConfig], active: &str) {
        for (_, item) in &self.breed_items {
            let _ = self.breed_menu.remove(item);
        }
        self.breed_items.clear();

        for cat in cats {
            let id = format!("{}{}", BREED_ID_PREFIX, cat.id);
            let item = check_item(&id, &cat_label(cat), cat.id == active);
            let _ = self.breed_menu.append(&item);
            self.breed_items.push((cat.id.clone(), item));
        }
    }

    /// Handles a click on a tray menu item.
    ///
    /// @param id The muda menu id of the clicked item.
    /// @return True if the id belonged to the tray menu.
    pub fn handle_menu_event(&self, id: &str) -> bool {
        if !id.starts_with(ID_PREFIX) {
            return false;
        }

        if let Some(cat_id) = id.strip_prefix(BREED_ID_PREFIX) {
            app::apply_breed_and_save(cat_id);
            return true;
        }
        if let Some(percent_text) = id.strip_prefix(SCALE_ID_PREFIX) {
            if let Ok(percent) = percent_text.parse::<u32>() {
                app::apply_scale_and_save(percent as f64 / 100.0);
            }
            return true;
        }

        match id {
            ID_TOGGLE_SHOW => self.toggle_or_resume(),
            ID_PAUSE_GUARD => {
                let paused = !app::state().paused_guard.load(Ordering::SeqCst);
                self.set_paused(paused);
            }
            ID_RESTART_ADMIN => {
                if !win_is_elevated() {
                    win_restart_as_admin();
                }
            }
            ID_CLICK_THROUGH => {
                app::change_settings(|s| s.click_through_locked = !s.click_through_locked);
            }
            ID_PRIVACY_GUARD => {
                app::change_settings(|s| s.privacy_password_guard = !s.privacy_password_guard);
            }
            ID_KEY_BUBBLES => {
                app::change_settings(|s| s.show_key_bubbles = !s.show_key_bubbles);
            }
            ID_SOUND => {
                app::change_settings(|s| s.sound_enabled = !s.sound_enabled);
            }
            ID_ALWAYS_ON_TOP => {
                app::change_settings(|s| s.always_on_top = !s.always_on_top);
            }
            ID_AUTOSTART => {
                app::change_settings(|s| s.autostart = !s.autostart);
            }
            ID_MCP => {
                app::change_settings(|s| s.mcp_enabled = !s.mcp_enabled);
            }
            ID_KB_65 => app::apply_keyboard_layout_and_save("65"),
            ID_KB_108 => app::apply_keyboard_layout_and_save("108"),
            ID_KB_CAT => app::apply_keyboard_orientation_and_save("cat"),
            ID_KB_VIEWER => app::apply_keyboard_orientation_and_save("viewer"),
            ID_DESKTOP_SHORTCUT => {
                let message = match crate::config::create_desktop_shortcut() {
                    Ok(_) => "Desktop shortcut created".to_string(),
                    Err(error) => format!("Could not create the shortcut: {}", error),
                };
                events::send(AppEvent::Toast(message));
            }
            ID_RESET_POS => events::send(AppEvent::Window(WindowCommand::ResetPosition)),
            ID_CHECK_UPDATE => {
                // The pet must be listening again to show the result.
                if app::state().paused_guard.load(Ordering::SeqCst) {
                    self.set_paused(false);
                }
                events::send(AppEvent::Window(WindowCommand::CheckForUpdate));
            }
            ID_QUIT => events::send(AppEvent::Quit),
            _ => {}
        }
        // Keep check marks right even when a change was refused (for example autostart failed).
        self.sync(&app::settings());
        true
    }

    /// Handles clicks on the tray icon. A left click shows or hides the pet, or resumes
    /// listening if it was paused.
    ///
    /// @param event The tray icon event.
    pub fn handle_icon_event(&self, event: &TrayIconEvent) {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            self.toggle_or_resume();
        }
    }

    /// Resumes listening if paused, otherwise toggles the pet window.
    fn toggle_or_resume(&self) {
        if app::state().paused_guard.load(Ordering::SeqCst) {
            self.set_paused(false);
        } else {
            events::send(AppEvent::Window(WindowCommand::ToggleVisible));
        }
    }

    /// Pauses or resumes listening. Pausing also hides the pet.
    ///
    /// @param paused True to pause.
    fn set_paused(&self, paused: bool) {
        app::state().paused_guard.store(paused, Ordering::SeqCst);
        crate::input_hook::set_hooks_paused(paused);
        self.pause_guard.set_checked(paused);
        let tooltip = if paused { paused_tooltip() } else { default_tooltip() };
        let _ = self.icon.set_tooltip(Some(tooltip));

        if paused {
            events::send(AppEvent::Window(WindowCommand::Hide));
        } else {
            events::send(AppEvent::Window(WindowCommand::Show));
            if app::settings().always_on_top {
                events::send(AppEvent::Window(WindowCommand::ReassertTopmost));
            }
        }
    }
}

/// Checks whether this process runs with administrator rights.
///
/// @return True if the process token is elevated.
pub fn win_is_elevated() -> bool {
    use std::ffi::c_void;
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, HANDLE};
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    #[link(name = "advapi32")]
    extern "system" {
        fn OpenProcessToken(process_handle: HANDLE, desired_access: u32, token_handle: *mut HANDLE) -> BOOL;
        fn GetTokenInformation(
            token_handle: HANDLE,
            token_information_class: u32,
            token_information: *mut c_void,
            token_information_length: u32,
            return_length: *mut u32,
        ) -> BOOL;
    }

    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_ELEVATION_CLASS: u32 = 20;

    #[repr(C)]
    struct TokenElevation {
        token_is_elevated: u32,
    }

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 || token.is_null() {
            return false;
        }
        let mut elevation = TokenElevation { token_is_elevated: 0 };
        let mut returned_length: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TOKEN_ELEVATION_CLASS,
            (&mut elevation as *mut TokenElevation).cast(),
            std::mem::size_of::<TokenElevation>() as u32,
            &mut returned_length,
        );
        let _ = CloseHandle(token);
        ok != 0 && elevation.token_is_elevated != 0
    }
}

/// Starts a new elevated copy (UAC prompt) that waits for this process, then quits this one.
/// Nothing happens if the user declines the prompt.
pub fn win_restart_as_admin() {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: HWND,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    /// Converts an OS string to a null-terminated UTF-16 buffer.
    ///
    /// @param text The text to convert.
    /// @return The UTF-16 buffer, ending with 0.
    fn to_wide(text: &std::ffi::OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let current_pid = std::process::id();

    // ShellExecuteW blocks while the UAC prompt is open, so run it off the UI thread.
    std::thread::spawn(move || {
        let verb = to_wide(std::ffi::OsStr::new("runas"));
        let file = to_wide(exe.as_os_str());
        let parameters_text = format!("--wait-pid {}", current_pid);
        let parameters = to_wide(std::ffi::OsStr::new(&parameters_text));
        let directory = exe.parent().map(|parent| to_wide(parent.as_os_str()));
        let directory_ptr = match directory.as_ref() {
            Some(buffer) => buffer.as_ptr(),
            None => std::ptr::null(),
        };

        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                parameters.as_ptr(),
                directory_ptr,
                SW_SHOWNORMAL,
            )
        };
        if result > 32 {
            crate::input_hook::set_hooks_paused(true);
            events::send(AppEvent::Quit);
        }
    });
}
