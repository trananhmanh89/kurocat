//! The right-click menu shown on the pet.
//!
//! The menu is rebuilt every time it opens, so its check marks always match the current settings.
//! Item ids start with `ctx:`; the tray menu uses `tray:` so the two never clash.

use crate::app;
use crate::cats::CatSkinConfig;
use crate::config::AppSettings;
use crate::events::{self, AppEvent, WindowCommand};
use muda::{CheckMenuItem, ContextMenu, Menu, MenuItem, PredefinedMenuItem, Submenu};

/// Scale presets offered in the menu, in percent.
const SCALE_PRESETS: [u32; 7] = [20, 25, 50, 75, 100, 125, 150];

/// Something the window must do after a menu item was chosen.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuOutcome {
    /// Nothing else to do (the settings were already changed).
    Done,
    /// Copy the MCP configuration to the clipboard.
    CopyMcpConfig,
}

/// Adds a check item to a menu.
///
/// @param menu The menu.
/// @param id Item id.
/// @param label Item text.
/// @param checked Whether the item shows a check mark.
fn append_check(menu: &Menu, id: &str, label: &str, checked: bool) {
    let item = CheckMenuItem::with_id(id, label, true, checked, None);
    let _ = menu.append(&item);
}

/// Adds a check item to a submenu.
///
/// @param submenu The submenu.
/// @param id Item id.
/// @param label Item text.
/// @param checked Whether the item shows a check mark.
fn append_check_to_submenu(submenu: &Submenu, id: &str, label: &str, checked: bool) {
    let item = CheckMenuItem::with_id(id, label, true, checked, None);
    let _ = submenu.append(&item);
}

/// Adds a plain item to a menu.
///
/// @param menu The menu.
/// @param id Item id.
/// @param label Item text.
fn append_item(menu: &Menu, id: &str, label: &str) {
    let item = MenuItem::with_id(id, label, true, None);
    let _ = menu.append(&item);
}

/// Adds a separator line to a menu.
///
/// @param menu The menu.
fn append_separator(menu: &Menu) {
    let _ = menu.append(&PredefinedMenuItem::separator());
}

/// Builds the right-click menu for the current settings.
///
/// @param settings The current settings.
/// @param cats All available cats.
/// @return The menu.
pub fn build_context_menu(settings: &AppSettings, cats: &[CatSkinConfig]) -> Menu {
    let menu = Menu::new();

    let active_name = cats
        .iter()
        .find(|cat| cat.id == settings.cat_breed)
        .map(|cat| cat.name.clone())
        .unwrap_or_else(|| "KuroCat".to_string());
    let title = MenuItem::with_id("ctx:title", format!("KuroCat · {}", active_name), false, None);
    let _ = menu.append(&title);
    append_separator(&menu);

    let appearance = Submenu::new("Appearance", true);
    for cat in cats {
        let id = format!("ctx:breed:{}", cat.id);
        append_check_to_submenu(&appearance, &id, &cat.name, cat.id == settings.cat_breed);
    }
    let _ = menu.append(&appearance);

    let keyboard = Submenu::new("Keyboard", true);
    append_check_to_submenu(&keyboard, "ctx:layout:65", "65% compact", settings.keyboard_layout != "108");
    append_check_to_submenu(&keyboard, "ctx:layout:108", "108-key full size", settings.keyboard_layout == "108");
    let _ = keyboard.append(&PredefinedMenuItem::separator());
    append_check_to_submenu(&keyboard, "ctx:orient:cat", "Facing the cat", settings.keyboard_orientation != "viewer");
    append_check_to_submenu(&keyboard, "ctx:orient:viewer", "Facing you", settings.keyboard_orientation == "viewer");
    let _ = menu.append(&keyboard);

    let size = Submenu::new("Size (Ctrl + scroll over the cat)", true);
    let current_percent = (settings.scale * 100.0).round() as u32;
    for percent in SCALE_PRESETS {
        let id = format!("ctx:scale:{}", percent);
        append_check_to_submenu(&size, &id, &format!("{}%", percent), percent == current_percent);
    }
    let _ = menu.append(&size);
    append_separator(&menu);

    append_check(&menu, "ctx:privacy", "Close eyes while typing passwords", settings.privacy_password_guard);
    append_check(&menu, "ctx:bubbles", "Key bubbles", settings.show_key_bubbles);
    append_check(&menu, "ctx:lock", "Lock click-through (hold Alt to unlock)", settings.click_through_locked);
    append_check(&menu, "ctx:sound", "Purr sound", settings.sound_enabled);
    append_check(&menu, "ctx:top", "Always on top", settings.always_on_top);
    append_check(&menu, "ctx:autostart", "Start with Windows", settings.autostart);
    append_separator(&menu);

    append_check(&menu, "ctx:mcp", "MCP server", settings.mcp_enabled);
    append_item(&menu, "ctx:copy-mcp", "Copy MCP config");
    append_separator(&menu);

    append_item(&menu, "ctx:shortcut", "Create desktop shortcut");
    append_item(&menu, "ctx:reset-pos", "Move to bottom-right corner");
    append_separator(&menu);
    append_item(&menu, "ctx:quit", "Quit");
    menu
}

/// Shows a menu at the cursor and blocks until it closes.
///
/// @param menu The menu to show.
/// @param hwnd The window that owns the menu.
/// @return True when an item was chosen.
pub fn show_context_menu(menu: &Menu, hwnd: isize) -> bool {
    unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd as windows_sys::Win32::Foundation::HWND);
        menu.show_context_menu_for_hwnd(hwnd, None)
    }
}

/// Runs the action of a context-menu item.
///
/// @param id The chosen item id.
/// @return What the window still has to do, or None when the id is not a context-menu item.
pub fn handle_context_menu_id(id: &str) -> Option<MenuOutcome> {
    let action = id.strip_prefix("ctx:")?;

    if let Some(breed) = action.strip_prefix("breed:") {
        app::apply_breed_and_save(breed);
        return Some(MenuOutcome::Done);
    }
    if let Some(layout) = action.strip_prefix("layout:") {
        app::apply_keyboard_layout_and_save(layout);
        return Some(MenuOutcome::Done);
    }
    if let Some(orientation) = action.strip_prefix("orient:") {
        app::apply_keyboard_orientation_and_save(orientation);
        return Some(MenuOutcome::Done);
    }
    if let Some(percent_text) = action.strip_prefix("scale:") {
        if let Ok(percent) = percent_text.parse::<f64>() {
            app::apply_scale_and_save(percent / 100.0);
        }
        return Some(MenuOutcome::Done);
    }

    match action {
        "privacy" => {
            app::change_settings(|s| s.privacy_password_guard = !s.privacy_password_guard);
        }
        "bubbles" => {
            app::change_settings(|s| s.show_key_bubbles = !s.show_key_bubbles);
        }
        "lock" => {
            // The window shows the "locked" / "unlocked" notice when the new settings arrive.
            app::change_settings(|s| s.click_through_locked = !s.click_through_locked);
        }
        "sound" => {
            app::change_settings(|s| s.sound_enabled = !s.sound_enabled);
        }
        "top" => {
            app::change_settings(|s| s.always_on_top = !s.always_on_top);
        }
        "autostart" => {
            app::change_settings(|s| s.autostart = !s.autostart);
        }
        "mcp" => {
            app::change_settings(|s| s.mcp_enabled = !s.mcp_enabled);
        }
        "copy-mcp" => return Some(MenuOutcome::CopyMcpConfig),
        "shortcut" => {
            let message = match crate::config::create_desktop_shortcut() {
                Ok(_) => "✓ Desktop shortcut created".to_string(),
                Err(error) => format!("Could not create the shortcut: {}", error),
            };
            events::send(AppEvent::Toast(message));
        }
        "reset-pos" => events::send(AppEvent::Window(WindowCommand::ResetPosition)),
        "quit" => events::send(AppEvent::Quit),
        _ => {}
    }
    Some(MenuOutcome::Done)
}
