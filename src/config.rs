//! User settings (`%APPDATA%\kuro-cat-pet\settings.json`), autostart and the desktop shortcut.

use serde::{Deserialize, Serialize};
use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

/// Default cat id.
///
/// @return "black".
fn default_cat_breed() -> String {
    "black".to_string()
}

/// Default for the MCP server switch.
///
/// @return true.
fn default_mcp_enabled() -> bool {
    true
}

/// Default for the password-field guard.
///
/// @return true.
fn default_privacy_password_guard() -> bool {
    true
}

/// Default port of the local MCP HTTP server.
///
/// @return 27420.
fn default_mcp_port() -> u16 {
    27420
}

/// Default keyboard layout. Builds with the `layout-108` feature start with the full-size keyboard.
///
/// @return "65" or "108".
pub fn default_keyboard_layout() -> String {
    if cfg!(feature = "layout-108") {
        "108".to_string()
    } else {
        "65".to_string()
    }
}

/// Cleans up a keyboard layout value.
///
/// @param raw The value to check.
/// @return "108" if `raw` is "108", otherwise "65".
pub fn normalize_keyboard_layout(raw: &str) -> String {
    if raw.trim() == "108" {
        "108".to_string()
    } else {
        "65".to_string()
    }
}

/// Default keyboard orientation.
///
/// @return "cat" (legends face the cat).
pub fn default_keyboard_orientation() -> String {
    "cat".to_string()
}

/// Cleans up a keyboard orientation value.
///
/// @param raw The value to check.
/// @return "viewer" if `raw` is "viewer", otherwise "cat".
pub fn normalize_keyboard_orientation(raw: &str) -> String {
    if raw.trim() == "viewer" {
        "viewer".to_string()
    } else {
        "cat".to_string()
    }
}

/// All user settings. Stored as camelCase JSON so files from older versions keep working.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// Window left edge in physical screen pixels (None = default bottom-right corner).
    pub x: Option<i32>,
    /// Window top edge in physical screen pixels (None = default bottom-right corner).
    pub y: Option<i32>,
    /// Pet scale, 0.2 to 1.5.
    pub scale: f64,
    /// Keep the pet above other windows.
    pub always_on_top: bool,
    /// Let all clicks pass through the pet (hold Alt to override).
    pub click_through_locked: bool,
    /// Show the key bubbles above the keyboard.
    pub show_key_bubbles: bool,
    /// Play the purr sound while petting.
    pub sound_enabled: bool,
    /// Start KuroCat when the user signs in.
    pub autostart: bool,
    /// Id of the visible cat.
    #[serde(default = "default_cat_breed")]
    pub cat_breed: String,
    /// Run the local MCP HTTP server.
    #[serde(default = "default_mcp_enabled")]
    pub mcp_enabled: bool,
    /// Port of the local MCP HTTP server.
    #[serde(default = "default_mcp_port")]
    pub mcp_port: u16,
    /// "65" or "108".
    #[serde(default = "default_keyboard_layout")]
    pub keyboard_layout: String,
    /// "cat" (legends face the cat) or "viewer" (legends face the user).
    #[serde(default = "default_keyboard_orientation")]
    pub keyboard_orientation: String,
    /// Close the cat's eyes and hide keys while typing into a password field.
    #[serde(default = "default_privacy_password_guard")]
    pub privacy_password_guard: bool,
}

impl Default for AppSettings {
    /// Settings used on first start.
    ///
    /// @return The default settings.
    fn default() -> Self {
        Self {
            x: None,
            y: None,
            scale: 1.0,
            always_on_top: true,
            click_through_locked: false,
            show_key_bubbles: true,
            sound_enabled: true,
            autostart: false,
            cat_breed: default_cat_breed(),
            mcp_enabled: default_mcp_enabled(),
            mcp_port: default_mcp_port(),
            keyboard_layout: default_keyboard_layout(),
            keyboard_orientation: default_keyboard_orientation(),
            privacy_password_guard: default_privacy_password_guard(),
        }
    }
}

/// Folder that holds the settings and custom cats.
///
/// @return `%APPDATA%\kuro-cat-pet`.
pub fn app_data_dir() -> PathBuf {
    let base = match std::env::var("APPDATA") {
        Ok(path) => PathBuf::from(path),
        Err(_) => PathBuf::from("."),
    };
    base.join("kuro-cat-pet")
}

/// Path of the settings file.
///
/// @return `%APPDATA%\kuro-cat-pet\settings.json`.
pub fn config_path() -> PathBuf {
    app_data_dir().join("settings.json")
}

/// Loads the settings from disk.
///
/// @return The saved settings, or the defaults if the file is missing or invalid.
pub fn load_settings() -> AppSettings {
    match try_load_settings() {
        Some(settings) => settings,
        None => AppSettings::default(),
    }
}

/// Loads the settings from disk without falling back to defaults.
///
/// @return The saved settings, or None if the file is missing or cannot be read as settings.
pub fn try_load_settings() -> Option<AppSettings> {
    let content = fs::read_to_string(config_path()).ok()?;
    let mut parsed = serde_json::from_str::<AppSettings>(&content).ok()?;
    parsed.scale = parsed.scale.clamp(0.20, 1.50);
    Some(parsed)
}

/// Replaces a file in one step: writes a temporary file next to it, then renames it over the
/// original. Readers never see a half-written file.
///
/// @param path The file to write.
/// @param contents The new contents.
/// @return Ok, or the I/O error.
pub fn write_file_atomically(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    let mut temporary_name = path.as_os_str().to_owned();
    temporary_name.push(".tmp");
    let temporary_path = PathBuf::from(temporary_name);
    fs::write(&temporary_path, contents)?;
    let result = fs::rename(&temporary_path, path);
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    result
}

/// Writes the settings to disk. Errors are ignored (the pet keeps running with in-memory settings).
///
/// @param settings The settings to save.
pub fn save_settings(settings: &AppSettings) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(settings) {
        let _ = write_file_atomically(&path, &json);
    }
}

/// Converts a string to a null-terminated UTF-16 buffer for Win32 calls.
///
/// @param text The text to convert.
/// @return The UTF-16 buffer, ending with 0.
fn to_wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain(std::iter::once(0)).collect()
}

/// Turns "start with Windows" on or off through the `HKCU\...\Run` registry key.
///
/// @param enabled True to start with Windows.
/// @return Ok, or an error message if the registry could not be changed.
pub fn set_windows_autostart(enabled: bool) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
        KEY_SET_VALUE, REG_SZ,
    };

    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
    let value_name = to_wide("KuroCat");
    let legacy_value_name = to_wide("KuroCatPet");

    unsafe {
        let mut hkey: HKEY = std::ptr::null_mut();
        let status = RegOpenKeyExW(HKEY_CURRENT_USER, subkey.as_ptr(), 0, KEY_SET_VALUE, &mut hkey);
        if status != 0 {
            return Err(format!("RegOpenKeyExW failed with code {}", status));
        }

        // Remove the old (v4.2 and earlier) value so upgraded users never start two copies.
        let _ = RegDeleteValueW(hkey, legacy_value_name.as_ptr());

        let result = if enabled {
            match std::env::current_exe() {
                Ok(exe) => {
                    let quoted = format!("\"{}\"", exe.to_string_lossy());
                    let data = to_wide(&quoted);
                    let byte_len = (data.len() * std::mem::size_of::<u16>()) as u32;
                    let set_status = RegSetValueExW(
                        hkey,
                        value_name.as_ptr(),
                        0,
                        REG_SZ,
                        data.as_ptr() as *const u8,
                        byte_len,
                    );
                    if set_status == 0 {
                        Ok(())
                    } else {
                        Err(format!("RegSetValueExW failed with code {}", set_status))
                    }
                }
                Err(error) => Err(error.to_string()),
            }
        } else {
            let _ = RegDeleteValueW(hkey, value_name.as_ptr());
            Ok(())
        };

        RegCloseKey(hkey);
        result
    }
}

/// Creates a `KuroCat.lnk` shortcut on the user's desktop.
///
/// @return The path of the shortcut, or an error message.
pub fn create_desktop_shortcut() -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let exe_path = std::env::current_exe().map_err(|error| error.to_string())?;
    let exe_text = exe_path.to_string_lossy().replace('\'', "''");
    let work_dir = match exe_path.parent() {
        Some(parent) => parent.to_string_lossy().replace('\'', "''"),
        None => String::new(),
    };

    let script = format!(
        "$desktop = [Environment]::GetFolderPath('Desktop'); \
         $lnk = Join-Path $desktop 'KuroCat.lnk'; \
         $ws = New-Object -ComObject WScript.Shell; \
         $s = $ws.CreateShortcut($lnk); \
         $s.TargetPath = '{exe}'; \
         $s.WorkingDirectory = '{wd}'; \
         $s.IconLocation = '{exe},0'; \
         $s.Description = 'KuroCat Desktop Companion'; \
         $s.Save(); \
         Write-Output $lnk",
        exe = exe_text,
        wd = work_dir,
    );

    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("Failed to start PowerShell: {}", error))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let error_text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if error_text.is_empty() {
            Err("Failed to create the desktop shortcut".to_string())
        } else {
            Err(error_text)
        }
    }
}
