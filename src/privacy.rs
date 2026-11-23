//! Password-field guard.
//!
//! A background thread checks every 140 ms whether keyboard focus is inside a password field
//! (Win32 `ES_PASSWORD` edit controls and UI Automation `IsPassword`). While it is, the input
//! hooks stop forwarding key codes and the pet closes or covers its eyes.

use crate::events::{self, AppEvent};
use crate::input_hook;
use std::sync::atomic::Ordering;
use std::time::Duration;

/// Reads whether the guard should run right now (setting on and not paused).
///
/// @return True when the password guard is enabled and listening is not paused.
fn guard_enabled() -> bool {
    let state = crate::app::state();
    if state.paused_guard.load(Ordering::Relaxed) {
        return false;
    }
    match state.settings.lock() {
        Ok(settings) => settings.privacy_password_guard,
        Err(_) => true,
    }
}

/// Turns privacy mode on or off and tells the hooks and the UI thread.
///
/// @param active True when a password field has focus.
fn set_privacy_active(active: bool) {
    input_hook::set_password_privacy_active(active);
    events::send(AppEvent::Privacy(active));
}

/// Starts the password-field watcher thread.
pub fn start_password_privacy_watcher() {
    std::thread::spawn(move || {
        let uia = win_impl::UiaDetector::new();
        let mut active = false;
        let mut clear_streak: u8 = 0;

        loop {
            std::thread::sleep(Duration::from_millis(140));

            if !guard_enabled() {
                if active {
                    active = false;
                    clear_streak = 0;
                    set_privacy_active(false);
                }
                continue;
            }

            let is_password = win_impl::is_focused_password_field(uia.as_ref());
            if is_password {
                clear_streak = 0;
                if !active {
                    active = true;
                    set_privacy_active(true);
                }
            } else if active {
                // Wait two ticks (~280 ms) before leaving, so focus flicker inside a login form
                // does not open the eyes for a moment.
                clear_streak = clear_streak.saturating_add(1);
                if clear_streak >= 2 {
                    active = false;
                    clear_streak = 0;
                    set_privacy_active(false);
                }
            }
        }
    });
}

mod win_impl {
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    };
    use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW, GetWindowThreadProcessId,
        GUITHREADINFO, GWL_STYLE,
    };

    const ES_PASSWORD: isize = 0x0020;

    /// A UI Automation client, used to ask whether the focused element is a password field.
    pub struct UiaDetector {
        uia: IUIAutomation,
        com_inited: bool,
    }

    impl UiaDetector {
        /// Initialises COM on this thread and creates the UI Automation client.
        ///
        /// @return The detector, or None if UI Automation is unavailable.
        pub fn new() -> Option<Self> {
            unsafe {
                let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
                let com_inited = hr.is_ok();
                let uia: IUIAutomation =
                    CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
                Some(Self { uia, com_inited })
            }
        }

        /// Asks UI Automation whether the focused element is a password field.
        ///
        /// @return True if it is.
        pub fn focused_is_password(&self) -> bool {
            unsafe {
                let Ok(element) = self.uia.GetFocusedElement() else {
                    return false;
                };
                match element.CurrentIsPassword() {
                    Ok(value) => value.as_bool(),
                    Err(_) => false,
                }
            }
        }
    }

    impl Drop for UiaDetector {
        /// Releases COM for this thread.
        fn drop(&mut self) {
            if self.com_inited {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    /// Checks whether the focused control of the foreground window is a classic Win32 password edit.
    ///
    /// @return True if the focused control has the ES_PASSWORD style.
    fn classic_win32_is_password() -> bool {
        unsafe {
            let foreground = GetForegroundWindow();
            if foreground.is_null() {
                return false;
            }
            let thread_id = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
            if thread_id == 0 {
                return false;
            }
            let mut info: GUITHREADINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
            if GetGUIThreadInfo(thread_id, &mut info) == 0 || info.hwndFocus.is_null() {
                return false;
            }
            // The ES_PASSWORD bit means something else on other control classes (for example
            // TVS_SHOWSELALWAYS on tree views), so only trust it on edit controls.
            if !is_edit_control(info.hwndFocus) {
                return false;
            }
            let style = GetWindowLongPtrW(info.hwndFocus, GWL_STYLE);
            (style & ES_PASSWORD) != 0
        }
    }

    /// Reports whether a window is a text edit control (Edit, RichEdit or a WinForms edit box).
    ///
    /// @param hwnd The window.
    /// @return True for edit control classes.
    unsafe fn is_edit_control(hwnd: windows_sys::Win32::Foundation::HWND) -> bool {
        let mut buffer = [0u16; 128];
        let length = windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32);
        if length <= 0 {
            return false;
        }
        let class_name = String::from_utf16_lossy(&buffer[..length as usize]).to_ascii_lowercase();
        class_name == "edit" || class_name.starts_with("richedit") || class_name.contains(".edit.")
    }

    /// Checks whether keyboard focus is in a password field, using both detection methods.
    ///
    /// @param uia The UI Automation detector, if it could be created.
    /// @return True if focus is in a password field.
    pub fn is_focused_password_field(uia: Option<&UiaDetector>) -> bool {
        if classic_win32_is_password() {
            return true;
        }
        if let Some(detector) = uia {
            return detector.focused_is_password();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_password_detector_initializes_and_probes_safely() {
        let detector = super::win_impl::UiaDetector::new();
        assert!(detector.is_some(), "CUIAutomation COM object should initialise on Windows");
        let _ = super::win_impl::is_focused_password_field(detector.as_ref());
    }
}
