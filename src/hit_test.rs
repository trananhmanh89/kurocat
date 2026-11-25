//! Background loop that decides, 40 times a second, whether the pet window should let mouse
//! clicks pass through, keeps the window above full-screen games, and reports where the cursor
//! is relative to the cat (for petting and eye tracking).

use crate::app::{self, base_dimensions};
use crate::events::{self, AppEvent, WindowCommand, WindowCursorInfo};
use crate::input_hook;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, RECT};
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect,
    IsIconic, IsWindowVisible, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_LAYERED, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT,
};

/// Returns the main window handle, or None when it does not exist yet.
///
/// @return The window handle.
fn main_hwnd() -> Option<HWND> {
    let raw = events::main_window();
    if raw == 0 {
        None
    } else {
        Some(raw as HWND)
    }
}

/// Reports whether a window ignores mouse input (has `WS_EX_TRANSPARENT`).
///
/// @param hwnd The window.
/// @return True when clicks pass through it.
fn is_click_through(hwnd: HWND) -> bool {
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    (ex_style & (WS_EX_TRANSPARENT as isize)) != 0
}

/// Makes a window ignore (true) or accept (false) mouse input.
///
/// @param hwnd The window.
/// @param ignore True to let clicks pass through.
pub fn set_click_through(hwnd: HWND, ignore: bool) {
    unsafe {
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let new_style = if ignore {
            ex_style | (WS_EX_TRANSPARENT as isize) | (WS_EX_LAYERED as isize)
        } else {
            ex_style & !(WS_EX_TRANSPARENT as isize)
        };
        if new_style != ex_style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, new_style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// Reports whether a window is in the topmost band.
///
/// @param hwnd The window.
/// @return True when the window has `WS_EX_TOPMOST`.
pub fn is_topmost(hwnd: HWND) -> bool {
    if hwnd.is_null() {
        return false;
    }
    let ex_style = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) };
    (ex_style & (WS_EX_TOPMOST as isize)) != 0
}

/// Reports whether a window belongs to the Windows shell (taskbar, desktop, task view…),
/// which should never trigger a z-order fix.
///
/// @param hwnd The window.
/// @return True for shell windows.
fn is_ignored_shell_window(hwnd: HWND) -> bool {
    let mut buffer = [0u16; 64];
    let length = unsafe { GetClassNameW(hwnd, buffer.as_mut_ptr(), buffer.len() as i32) };
    if length <= 0 {
        return false;
    }
    let class_name = String::from_utf16_lossy(&buffer[..length as usize]);
    matches!(
        class_name.as_str(),
        "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "Progman"
            | "WorkerW"
            | "Windows.UI.Core.CoreWindow"
            | "XamlExplorerHostIslandWindow"
            | "MultitaskingViewFrame"
            | "TaskListThumbnailWnd"
    )
}

/// Reports whether two windows overlap on screen.
///
/// @param first First window.
/// @param second Second window.
/// @return True when their rectangles overlap (or cannot be read).
fn rects_overlap(first: HWND, second: HWND) -> bool {
    let mut first_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    let mut second_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    unsafe {
        if GetWindowRect(first, &mut first_rect) == 0 || GetWindowRect(second, &mut second_rect) == 0 {
            return true;
        }
    }
    first_rect.left < second_rect.right
        && first_rect.right > second_rect.left
        && first_rect.top < second_rect.bottom
        && first_rect.bottom > second_rect.top
}

/// Data passed through `EnumWindows` while comparing z-order.
#[repr(C)]
struct OrderSearch {
    candidate: HWND,
    pet: HWND,
    candidate_above: bool,
}

/// `EnumWindows` callback: stops at whichever of the two windows comes first (topmost first).
///
/// @param hwnd The window being visited.
/// @param lparam Pointer to an [`OrderSearch`].
/// @return 0 to stop, 1 to continue.
unsafe extern "system" fn find_first_of_two(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut OrderSearch);
    if hwnd == search.candidate {
        search.candidate_above = true;
        0
    } else if hwnd == search.pet {
        0
    } else {
        1
    }
}

/// Reports whether another window is stacked above the pet and overlaps it, e.g. a
/// borderless full-screen game that pushed itself into the topmost band.
///
/// @param candidate The other window (usually the foreground window).
/// @param pet The pet window.
/// @return True when the candidate covers the pet.
pub fn window_is_above(candidate: HWND, pet: HWND) -> bool {
    if candidate.is_null() || pet.is_null() || candidate == pet {
        return false;
    }
    if is_ignored_shell_window(candidate) || !rects_overlap(candidate, pet) {
        return false;
    }
    let mut search = OrderSearch { candidate, pet, candidate_above: false };
    unsafe {
        EnumWindows(Some(find_first_of_two), (&mut search as *mut OrderSearch) as LPARAM);
    }
    search.candidate_above
}

/// Decides whether a pet-relative point is over the cat or over the desk (keyboard and mouse).
///
/// @param rx Cursor x relative to the pet (0..1).
/// @param ry Cursor y relative to the pet (0..1).
/// @param is_108 True with the 108-key layout.
/// @return (over the cat or desk, over the cat).
pub fn is_over_interactive_silhouette(rx: f64, ry: f64, is_108: bool) -> (bool, bool) {
    let (cat_centre_x, cat_radius_x) = if is_108 { (0.51, 0.21) } else { (0.48, 0.25) };
    let dx = (rx - cat_centre_x) / cat_radius_x;
    let dy = (ry - 0.49) / 0.27;
    let over_cat = dx * dx + dy * dy <= 1.0;
    let over_desk = if is_108 {
        (0.04..=0.94).contains(&rx) && (0.65..=0.94).contains(&ry)
    } else {
        (0.06..=0.92).contains(&rx) && (0.65..=0.91).contains(&ry)
    };
    (over_cat || over_desk, over_cat)
}

/// State kept between ticks by the z-order guard.
struct TopmostGuard {
    last_foreground: isize,
    check_ticks: u8,
    attempts_while_covered: u8,
    last_reassert: Instant,
}

impl TopmostGuard {
    /// @return A guard that has not checked anything yet.
    fn new() -> TopmostGuard {
        TopmostGuard {
            last_foreground: 0,
            check_ticks: 0,
            attempts_while_covered: 0,
            last_reassert: Instant::now() - Duration::from_secs(5),
        }
    }

    /// Clears the counters (when the guard is not active).
    fn reset(&mut self) {
        self.check_ticks = 0;
        self.attempts_while_covered = 0;
    }

    /// Runs one tick of the guard. About every 150 ms (and shortly after the foreground window
    /// changes) it checks whether the pet lost its topmost flag or is covered by the foreground
    /// window, and if so asks the UI thread to put it back on top without taking focus.
    ///
    /// @param pet The pet window.
    fn tick(&mut self, pet: HWND) {
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_id = foreground as isize;
        if foreground_id != self.last_foreground {
            self.last_foreground = foreground_id;
            self.attempts_while_covered = 0;
            self.check_ticks = 4;
        }
        self.check_ticks = self.check_ticks.saturating_add(1);
        if self.check_ticks < 6 {
            return;
        }
        self.check_ticks = 0;
        if unsafe { IsIconic(pet) } != 0 {
            return;
        }
        let lost_topmost = !is_topmost(pet);
        let covered = !lost_topmost && !foreground.is_null() && foreground != pet && window_is_above(foreground, pet);

        if !lost_topmost && !covered {
            self.attempts_while_covered = 0;
        } else if lost_topmost {
            if self.last_reassert.elapsed() >= Duration::from_millis(250) {
                events::send(AppEvent::Window(WindowCommand::ReassertTopmost));
                self.last_reassert = Instant::now();
            }
        } else if self.attempts_while_covered < 3 {
            let cooldown = match self.attempts_while_covered {
                0 => Duration::from_millis(0),
                1 => Duration::from_millis(350),
                _ => Duration::from_millis(900),
            };
            if self.last_reassert.elapsed() >= cooldown {
                events::send(AppEvent::Window(WindowCommand::ReassertTopmost));
                self.last_reassert = Instant::now();
                self.attempts_while_covered = self.attempts_while_covered.saturating_add(1);
            }
        }
    }
}

/// Starts the hit-test thread.
pub fn start_hit_test_loop() {
    std::thread::spawn(move || {
        let mut last_ignore: Option<bool> = None;
        let mut last_cursor: (i32, i32) = (i32::MIN, i32::MIN);
        let mut last_alt_override = false;
        let mut last_effective_locked = false;
        let mut drift_ticks: u8 = 0;
        let mut guard = TopmostGuard::new();

        loop {
            std::thread::sleep(Duration::from_millis(25));
            let state = app::state();
            if state.paused_guard.load(Ordering::Relaxed) {
                continue;
            }
            let Some(hwnd) = main_hwnd() else {
                continue;
            };
            if unsafe { IsWindowVisible(hwnd) } == 0 {
                continue;
            }

            let mut cursor = POINT { x: 0, y: 0 };
            if unsafe { GetCursorPos(&mut cursor) } == 0 {
                continue;
            }
            let mut window_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            if unsafe { GetWindowRect(hwnd, &mut window_rect) } == 0 {
                continue;
            }
            let width = (window_rect.right - window_rect.left).max(1) as f64;
            let height = (window_rect.bottom - window_rect.top).max(1) as f64;
            let rx = (cursor.x - window_rect.left) as f64 / width;
            let ry = (cursor.y - window_rect.top) as f64 / height;
            let inside = (0.0..=1.0).contains(&rx) && (0.0..=1.0).contains(&ry);

            let settings = app::settings();
            let locked = settings.click_through_locked;
            let is_108 = settings.keyboard_layout == "108";
            let scale = settings.scale.clamp(app::MIN_SCALE, app::MAX_SCALE);

            let menu_open = state.menu_open.load(Ordering::Relaxed);
            let dragging = state.dragging.load(Ordering::Relaxed);
            let banner_open = state.banner_open.load(Ordering::Relaxed);

            if settings.always_on_top && !menu_open && !dragging {
                guard.tick(hwnd);
            } else {
                guard.reset();
            }

            let alt_override = input_hook::is_alt_pressed();
            let effective_locked = locked && !alt_override;

            // The pet is drawn in the bottom-right corner of the window, which can be larger than
            // the pet at small scales (the window has a minimum size).
            let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96) as f64;
            let device_scale = dpi / 96.0;
            let (base_width, base_height) = base_dimensions(&settings.keyboard_layout);
            let logical_width = (width / device_scale).max(1.0);
            let logical_height = (height / device_scale).max(1.0);
            let pet_width_fraction = ((base_width * scale) / logical_width).clamp(0.05, 1.0);
            let pet_height_fraction = ((base_height * scale) / logical_height).clamp(0.05, 1.0);
            let pet_rx = (rx - (1.0 - pet_width_fraction)) / pet_width_fraction;
            let pet_ry = (ry - (1.0 - pet_height_fraction)) / pet_height_fraction;
            let inside_pet = (0.0..=1.0).contains(&pet_rx) && (0.0..=1.0).contains(&pet_ry);

            let (over_interactive, raw_over_cat) = if inside_pet {
                is_over_interactive_silhouette(pet_rx, pet_ry, is_108)
            } else {
                (false, false)
            };
            // While click-through is locked (and Alt is not held) petting is disabled too, so the
            // cat does not squint at a cursor that cannot touch it.
            let over_cat = if effective_locked { false } else { raw_over_cat };
            let over_banner = banner_open && inside && ry <= 0.19;

            let should_ignore = if menu_open || dragging || over_banner {
                false
            } else if effective_locked {
                true
            } else {
                !over_interactive
            };

            if last_ignore != Some(should_ignore) {
                set_click_through(hwnd, should_ignore);
                last_ignore = Some(should_ignore);
                drift_ticks = 0;
            } else if is_click_through(hwnd) != should_ignore {
                // Something else (Show Desktop, wallpaper tools…) changed the style: fix it after 100 ms.
                drift_ticks = drift_ticks.saturating_add(1);
                if drift_ticks >= 4 {
                    set_click_through(hwnd, should_ignore);
                    drift_ticks = 0;
                }
            } else {
                drift_ticks = 0;
            }

            let cursor_changed = (cursor.x, cursor.y) != last_cursor;
            if cursor_changed || alt_override != last_alt_override || effective_locked != last_effective_locked {
                last_cursor = (cursor.x, cursor.y);
                last_alt_override = alt_override;
                last_effective_locked = effective_locked;
                events::send(AppEvent::Cursor(WindowCursorInfo {
                    rel_x: pet_rx,
                    rel_y: pet_ry,
                    over_cat,
                    alt_override,
                }));
            }
        }
    });
}
