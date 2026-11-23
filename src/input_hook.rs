//! Global keyboard and mouse hooks (Win32 low-level hooks).
//!
//! The hooks run on their own thread with a message loop. Raw events go through a channel to a
//! dispatcher thread, which removes key auto-repeat duplicates, throttles mouse moves to about
//! 60 Hz and forwards everything to the UI thread as [`AppEvent::Input`].

use crate::events::{self, AppEvent};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

/// A keyboard or mouse event as seen by the pet.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InputEvent {
    /// A key went down. `repeat` is true for auto-repeat while the key is held.
    KeyDown { vk: u32, scan: u32, repeat: bool },
    /// A key went up.
    KeyUp { vk: u32, scan: u32 },
    /// A mouse button went down: "left", "right" or "middle".
    MouseDown { button: String },
    /// A mouse button went up: "left", "right" or "middle".
    MouseUp { button: String },
    /// The mouse wheel turned.
    MouseWheel { delta: i16 },
    /// The mouse moved. `x`/`y` are screen pixels, `nx`/`ny` are 0..1 across the virtual screen.
    MouseMove { x: i32, y: i32, nx: f64, ny: f64 },
    /// A key was pressed while a password field has focus. No key information is included.
    PrivacyTyping,
}

enum RawHookMsg {
    KeyDown { vk: u32, scan: u32 },
    KeyUp { vk: u32, scan: u32 },
    MouseDown(&'static str),
    MouseUp(&'static str),
    MouseWheel(i16),
    MouseMove { x: i32, y: i32 },
}

static HOOK_TX: OnceLock<mpsc::Sender<RawHookMsg>> = OnceLock::new();
static HOOKS_PAUSED: AtomicBool = AtomicBool::new(false);
static PASSWORD_PRIVACY_ACTIVE: AtomicBool = AtomicBool::new(false);
static WIN_HOOK_TID: AtomicU32 = AtomicU32::new(0);
/// Thread message that tells the hook thread to remove (wParam = 1) or reinstall (0) its hooks.
const WM_KURO_PAUSE_TOGGLE: u32 = 0x8007;

/// Tells the hooks whether a password field has focus. While it does, key codes never leave
/// this module; only [`InputEvent::PrivacyTyping`] is sent.
///
/// @param active True while a password field has focus.
pub fn set_password_privacy_active(active: bool) {
    PASSWORD_PRIVACY_ACTIVE.store(active, Ordering::SeqCst);
}

/// Checks whether Alt is held right now. Alt temporarily overrides the click-through lock.
///
/// @return True while an Alt key is down.
pub fn is_alt_pressed() -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_MENU};
    let async_down = unsafe { (GetAsyncKeyState(VK_MENU as i32) as u16 & 0x8000) != 0 };
    async_down
}

/// Removes or reinstalls the global hooks (used by the anti-cheat pause mode).
///
/// @param paused True to stop listening, false to listen again.
pub fn set_hooks_paused(paused: bool) {
    HOOKS_PAUSED.store(paused, Ordering::SeqCst);
    let tid = WIN_HOOK_TID.load(Ordering::SeqCst);
    if tid != 0 {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::PostThreadMessageW(
                tid,
                WM_KURO_PAUSE_TOGGLE,
                paused as usize,
                0,
            );
        }
    }
}

/// Longest pause between auto-repeat key-downs. Windows waits at most one second before
/// repeating, so a later key-down for a "held" key means its key-up was missed.
const MAX_REPEAT_GAP: Duration = Duration::from_millis(1100);

/// Tracks which keys are held, so auto-repeat key-downs can be marked as repeats and
/// key-ups for keys we never saw go down are dropped.
struct KeyDeduplicator {
    /// When each held key last sent a key-down; None while the key is up.
    pressed: Vec<Option<Instant>>,
}

impl KeyDeduplicator {
    /// Creates a tracker with no keys held.
    ///
    /// @return The new tracker.
    fn new() -> Self {
        Self { pressed: vec![None; 512] }
    }

    /// Records a key-down.
    ///
    /// @param vk The virtual-key code.
    /// @return True if this is an auto-repeat of a key that is already held.
    fn on_key_down(&mut self, vk: u32) -> bool {
        let index = vk as usize;
        if index >= self.pressed.len() {
            return false;
        }
        let now = Instant::now();
        let previous = self.pressed[index];
        self.pressed[index] = Some(now);
        match previous {
            Some(last_down) => now.duration_since(last_down) <= MAX_REPEAT_GAP,
            None => false,
        }
    }

    /// Records a key-up.
    ///
    /// @param vk The virtual-key code.
    /// @return True if the key-up should be forwarded (the key was held).
    fn on_key_up(&mut self, vk: u32) -> bool {
        let index = vk as usize;
        if index >= self.pressed.len() {
            return true;
        }
        if self.pressed[index].is_none() {
            return false;
        }
        self.pressed[index] = None;
        true
    }

    /// Forgets every held key, for example when listening pauses and key-ups would be missed.
    ///
    /// @return The virtual-key codes that were held.
    fn release_all(&mut self) -> Vec<u32> {
        let mut released = Vec::new();
        for (index, slot) in self.pressed.iter_mut().enumerate() {
            if slot.is_some() {
                released.push(index as u32);
                *slot = None;
            }
        }
        released
    }
}

/// Shortest time between two mouse-move events sent to the UI (about one frame).
const MOUSE_MOVE_INTERVAL: Duration = Duration::from_millis(16);

/// Sends a mouse-move event to the UI thread.
///
/// @param x Cursor x in physical screen pixels.
/// @param y Cursor y in physical screen pixels.
fn send_mouse_move(x: i32, y: i32) {
    let (nx, ny) = normalize_screen_point(x, y);
    events::send(AppEvent::Input(InputEvent::MouseMove { x, y, nx, ny }));
}

/// Gives keys that share a virtual-key code their own code, so the 108-key layout lights the
/// right key:
/// - Numpad Enter is VK_RETURN with the extended flag.
/// - With NumLock off, the numpad digits send navigation codes (Home, Up, ...) without the
///   extended flag, while the real navigation keys have it.
///
/// @param vk The virtual-key code from the hook.
/// @param extended True when the hook set the extended-key flag.
/// @return The code to use.
fn map_virtual_key(vk: u32, extended: bool) -> u32 {
    if extended {
        if vk == 0x0D {
            return 0x10D;
        }
        return vk;
    }
    match vk {
        0x2D => 0x60, // Insert -> Numpad 0
        0x23 => 0x61, // End -> Numpad 1
        0x28 => 0x62, // Down -> Numpad 2
        0x22 => 0x63, // Page Down -> Numpad 3
        0x25 => 0x64, // Left -> Numpad 4
        0x0C => 0x65, // Clear -> Numpad 5
        0x27 => 0x66, // Right -> Numpad 6
        0x24 => 0x67, // Home -> Numpad 7
        0x26 => 0x68, // Up -> Numpad 8
        0x21 => 0x69, // Page Up -> Numpad 9
        0x2E => 0x6E, // Delete -> Numpad decimal point
        _ => vk,
    }
}

/// Starts the hook thread and the dispatcher thread.
pub fn start_global_hooks() {
    let (tx, rx) = mpsc::channel::<RawHookMsg>();
    let _ = HOOK_TX.set(tx);

    std::thread::spawn(move || {
        let mut dedup = KeyDeduplicator::new();
        let mut last_mouse_emit = Instant::now() - Duration::from_secs(1);
        // A mouse position skipped by the throttle; sent later so the last position is never lost.
        let mut pending_move: Option<(i32, i32)> = None;

        loop {
            let msg = match pending_move {
                Some((x, y)) => {
                    let wait = MOUSE_MOVE_INTERVAL.saturating_sub(last_mouse_emit.elapsed());
                    match rx.recv_timeout(wait) {
                        Ok(msg) => msg,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            pending_move = None;
                            if !HOOKS_PAUSED.load(Ordering::Relaxed) {
                                last_mouse_emit = Instant::now();
                                send_mouse_move(x, y);
                            }
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
                None => match rx.recv() {
                    Ok(msg) => msg,
                    Err(_) => break,
                },
            };
            if HOOKS_PAUSED.load(Ordering::Relaxed) {
                // Key-ups are not seen while paused, so let go of every held key now.
                for vk in dedup.release_all() {
                    events::send(AppEvent::Input(InputEvent::KeyUp { vk, scan: 0 }));
                }
                continue;
            }
            match msg {
                RawHookMsg::KeyDown { vk, scan } => {
                    let repeat = dedup.on_key_down(vk);
                    if PASSWORD_PRIVACY_ACTIVE.load(Ordering::Relaxed) {
                        // Password keystrokes never leave this module: only a content-less pulse.
                        if !repeat {
                            events::send(AppEvent::Input(InputEvent::PrivacyTyping));
                        }
                        continue;
                    }
                    events::send(AppEvent::Input(InputEvent::KeyDown { vk, scan, repeat }));
                }
                RawHookMsg::KeyUp { vk, scan } => {
                    let was_held = dedup.on_key_up(vk);
                    if PASSWORD_PRIVACY_ACTIVE.load(Ordering::Relaxed) {
                        continue;
                    }
                    if was_held {
                        events::send(AppEvent::Input(InputEvent::KeyUp { vk, scan }));
                    }
                }
                RawHookMsg::MouseDown(button) => {
                    events::send(AppEvent::Input(InputEvent::MouseDown {
                        button: button.to_string(),
                    }));
                }
                RawHookMsg::MouseUp(button) => {
                    events::send(AppEvent::Input(InputEvent::MouseUp {
                        button: button.to_string(),
                    }));
                }
                RawHookMsg::MouseWheel(delta) => {
                    events::send(AppEvent::Input(InputEvent::MouseWheel { delta }));
                }
                RawHookMsg::MouseMove { x, y } => {
                    let now = Instant::now();
                    if now.duration_since(last_mouse_emit) >= MOUSE_MOVE_INTERVAL {
                        last_mouse_emit = now;
                        pending_move = None;
                        send_mouse_move(x, y);
                    } else {
                        pending_move = Some((x, y));
                    }
                }
            }
        }
    });

    std::thread::spawn(|| {
        run_windows_hook_loop();
    });
}

/// Converts a screen point to 0..1 coordinates across the whole virtual screen (all monitors).
///
/// @param x Screen x in pixels.
/// @param y Screen y in pixels.
/// @return (nx, ny), each clamped to 0..1.
fn normalize_screen_point(x: i32, y: i32) -> (f64, f64) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
        SM_YVIRTUALSCREEN,
    };
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN) as f64;
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN) as f64;
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1) as f64;
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1) as f64;
        let nx = ((x as f64 - left) / width).clamp(0.0, 1.0);
        let ny = ((y as f64 - top) / height).clamp(0.0, 1.0);
        (nx, ny)
    }
}

/// Low-level keyboard hook callback.
///
/// @param code Hook code; negative means "pass on without processing".
/// @param wparam The keyboard message (WM_KEYDOWN, WM_KEYUP, ...).
/// @param lparam Pointer to a KBDLLHOOKSTRUCT.
/// @return The result of the next hook in the chain.
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    if code >= 0 {
        let kb = &*(lparam as *const KBDLLHOOKSTRUCT);
        let msg = wparam as u32;
        let mapped_vk = map_virtual_key(kb.vkCode, (kb.flags & 0x01) != 0);
        if let Some(tx) = HOOK_TX.get() {
            match msg {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    let _ = tx.send(RawHookMsg::KeyDown { vk: mapped_vk, scan: kb.scanCode });
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    let _ = tx.send(RawHookMsg::KeyUp { vk: mapped_vk, scan: kb.scanCode });
                }
                _ => {}
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Low-level mouse hook callback.
///
/// @param code Hook code; negative means "pass on without processing".
/// @param wparam The mouse message (WM_MOUSEMOVE, WM_LBUTTONDOWN, ...).
/// @param lparam Pointer to an MSLLHOOKSTRUCT.
/// @return The result of the next hook in the chain.
unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: usize, lparam: isize) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
        WM_MBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP,
    };

    if code >= 0 {
        let ms = &*(lparam as *const MSLLHOOKSTRUCT);
        let msg = wparam as u32;
        if let Some(tx) = HOOK_TX.get() {
            match msg {
                WM_MOUSEMOVE => {
                    let _ = tx.send(RawHookMsg::MouseMove { x: ms.pt.x, y: ms.pt.y });
                }
                WM_LBUTTONDOWN => {
                    let _ = tx.send(RawHookMsg::MouseDown("left"));
                }
                WM_LBUTTONUP => {
                    let _ = tx.send(RawHookMsg::MouseUp("left"));
                }
                WM_RBUTTONDOWN => {
                    let _ = tx.send(RawHookMsg::MouseDown("right"));
                }
                WM_RBUTTONUP => {
                    let _ = tx.send(RawHookMsg::MouseUp("right"));
                }
                WM_MBUTTONDOWN => {
                    let _ = tx.send(RawHookMsg::MouseDown("middle"));
                }
                WM_MBUTTONUP => {
                    let _ = tx.send(RawHookMsg::MouseUp("middle"));
                }
                WM_MOUSEWHEEL => {
                    let delta = ((ms.mouseData >> 16) & 0xffff) as i16;
                    let _ = tx.send(RawHookMsg::MouseWheel(delta));
                }
                _ => {}
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Installs the hooks and runs the hook thread's message loop. Never returns while hooks are used.
fn run_windows_hook_loop() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetMessageW, PeekMessageW, SetWindowsHookExW, UnhookWindowsHookEx, MSG, PM_NOREMOVE,
        WH_KEYBOARD_LL, WH_MOUSE_LL,
    };

    unsafe {
        // Make sure the thread message queue exists before publishing the thread id.
        let mut init_msg: MSG = std::mem::zeroed();
        PeekMessageW(&mut init_msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
        WIN_HOOK_TID.store(GetCurrentThreadId(), Ordering::SeqCst);

        let module = GetModuleHandleW(std::ptr::null());
        let mut keyboard_hook = if HOOKS_PAUSED.load(Ordering::SeqCst) {
            std::ptr::null_mut()
        } else {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), module, 0)
        };
        let mut mouse_hook = if HOOKS_PAUSED.load(Ordering::SeqCst) {
            std::ptr::null_mut()
        } else {
            SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), module, 0)
        };

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.message != WM_KURO_PAUSE_TOGGLE {
                continue;
            }
            let pause = msg.wParam != 0;
            if pause {
                if !keyboard_hook.is_null() {
                    UnhookWindowsHookEx(keyboard_hook);
                    keyboard_hook = std::ptr::null_mut();
                }
                if !mouse_hook.is_null() {
                    UnhookWindowsHookEx(mouse_hook);
                    mouse_hook = std::ptr::null_mut();
                }
            } else {
                if keyboard_hook.is_null() {
                    keyboard_hook =
                        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), module, 0);
                }
                if mouse_hook.is_null() {
                    mouse_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), module, 0);
                }
            }
        }

        if !keyboard_hook.is_null() {
            UnhookWindowsHookEx(keyboard_hook);
        }
        if !mouse_hook.is_null() {
            UnhookWindowsHookEx(mouse_hook);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numpad_keys_get_their_own_codes() {
        assert_eq!(map_virtual_key(0x0D, true), 0x10D);
        assert_eq!(map_virtual_key(0x0D, false), 0x0D);
        assert_eq!(map_virtual_key(0x24, false), 0x67);
        assert_eq!(map_virtual_key(0x24, true), 0x24);
        assert_eq!(map_virtual_key(0x2E, false), 0x6E);
        assert_eq!(map_virtual_key(0x41, false), 0x41);
    }

    #[test]
    fn dedup_marks_auto_repeat_and_drops_unmatched_key_up() {
        let mut dedup = KeyDeduplicator::new();
        assert!(!dedup.on_key_down(0x41));
        assert!(dedup.on_key_down(0x41));
        assert!(dedup.on_key_up(0x41));
        assert!(!dedup.on_key_up(0x41));
        // A quick re-press after release is a fresh key-down, never a repeat.
        assert!(!dedup.on_key_down(0x41));
    }
}
