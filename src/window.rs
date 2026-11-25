//! The pet window: a borderless, per-pixel transparent (layered) Win32 window, its frame loop,
//! mouse handling (drag, right-click menu, Ctrl + wheel scaling, banner buttons) and the
//! handling of every [`AppEvent`] sent by the background threads.
//!
//! Everything here runs on the UI thread. The window state lives in a thread-local so the
//! window procedure can reach it; it is never borrowed while a modal menu is open.

use crate::app::{self, MAX_SCALE, MIN_SCALE};
use crate::banner::Banner;
use crate::cats::{self, CatSkinConfig};
use crate::config::AppSettings;
use crate::engine::{now_ms, Engine, KeyboardOrientation};
use crate::events::{self, AppEvent, WindowCommand, WM_APP_EVENT};
use crate::input_hook::InputEvent;
use crate::keyboard_layout::KeyboardLayoutId;
use crate::menus::{self, MenuOutcome};
use crate::render::{Canvas, LineJoin};
use crate::tray::Tray;
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use windows_sys::Win32::Foundation::{
    CloseHandle, HANDLE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WAIT_OBJECT_0, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, MonitorFromPoint,
    ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION,
    DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, MONITOR_DEFAULTTONULL, GetMonitorInfoW, MonitorFromWindow, MONITORINFO,
    MONITOR_DEFAULTTONEAREST,
};
use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Threading::{
    CreateWaitableTimerExW, CreateWaitableTimerW, SetWaitableTimer, CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
    INFINITE, TIMER_ALL_ACCESS,
};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, ReleaseCapture, SetCapture, VK_MENU};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, CreateWindowExW, DefWindowProcW, DestroyCursor, DestroyWindow, DispatchMessageW, GetCursorPos,
    GetSystemMetrics, GetWindowRect, IsWindowVisible, KillTimer, LoadCursorW, LoadIconW, MsgWaitForMultipleObjectsEx,
    PeekMessageW, PostQuitMessage, RegisterClassExW, SetCursor, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
    UpdateLayeredWindow, HCURSOR, HTCLIENT, HWND_NOTOPMOST, HWND_TOPMOST, ICONINFO, IDC_ARROW, IDC_SIZEALL,
    MA_NOACTIVATE, MSG, MWMO_INPUTAVAILABLE, PM_REMOVE, QS_ALLINPUT, SM_CXSCREEN, SM_CYSCREEN, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA, WM_CAPTURECHANGED, WM_CLOSE,
    WM_DESTROY, WM_DPICHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_QUIT, WM_RBUTTONUP, WM_SETCURSOR, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};

/// Timer id used to keep animating while the right-click menu is open.
const MENU_ANIMATION_TIMER: usize = 1;
/// Timer id used to keep animating while the tray menu is open. It is started just before the
/// tray menu opens and stopped by the next regular frame, which only runs once the menu closed.
const TRAY_MENU_TIMER: usize = 2;
/// True while [`TRAY_MENU_TIMER`] is running.
static TRAY_MENU_TIMER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Frame time while something moves (60 fps).
const ACTIVE_FRAME_MS: f64 = 1000.0 / 60.0;
/// Frame time while the pet only breathes (30 fps).
const IDLE_FRAME_MS: f64 = 1000.0 / 30.0;
/// How often the loop looks again while the window is hidden.
const HIDDEN_FRAME_MS: f64 = 250.0;
/// Scale change per wheel notch with Ctrl held.
const WHEEL_SCALE_STEP: f64 = 0.05;
/// How long after the last Ctrl + wheel notch the new scale is saved.
const SCALE_COMMIT_DELAY_MS: f64 = 400.0;
/// `CF_UNICODETEXT` clipboard format.
const CLIPBOARD_UNICODE_TEXT: u32 = 13;
/// `MK_CONTROL` flag in the wheel message.
const WHEEL_CONTROL_FLAG: usize = 0x0008;

/// Converts text to a NUL-terminated UTF-16 buffer for Win32.
///
/// @param text The text.
/// @return The wide string.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads the signed x coordinate from a mouse message.
///
/// @param lparam The message parameter.
/// @return x in client pixels.
fn mouse_x(lparam: LPARAM) -> i32 {
    (lparam & 0xffff) as u16 as i16 as i32
}

/// Reads the signed y coordinate from a mouse message.
///
/// @param lparam The message parameter.
/// @return y in client pixels.
fn mouse_y(lparam: LPARAM) -> i32 {
    ((lparam >> 16) & 0xffff) as u16 as i16 as i32
}

/// Which mouse pointer the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CursorKind {
    Arrow,
    Petting,
    Dragging,
}

/// An in-progress window drag.
#[derive(Clone, Copy, Debug)]
struct DragState {
    /// Cursor position minus window position when the drag started.
    grab_offset_x: i32,
    grab_offset_y: i32,
}

/// A Ctrl + wheel scale change that has not been saved yet.
#[derive(Clone, Copy, Debug)]
struct ScalePreview {
    scale: f64,
    commit_at: f64,
}

/// A 32-bit top-down DIB section that `UpdateLayeredWindow` reads from.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl Surface {
    /// Creates a surface of the given size.
    ///
    /// @param width Width in physical pixels.
    /// @param height Height in physical pixels.
    /// @return The surface, or None when GDI fails.
    fn create(width: i32, height: i32) -> Option<Surface> {
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            if dc.is_null() {
                return None;
            }
            let mut info: BITMAPINFO = std::mem::zeroed();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            };
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(dc);
                return None;
            }
            let previous = SelectObject(dc, bitmap as HGDIOBJ);
            Some(Surface { dc, bitmap, previous, bits: bits as *mut u8, width, height })
        }
    }

    /// Copies premultiplied RGBA pixels into the surface as premultiplied BGRA.
    ///
    /// @param rgba Pixels of exactly `width × height`.
    fn copy_from_rgba(&mut self, rgba: &[u8]) {
        let length = (self.width as usize) * (self.height as usize) * 4;
        if rgba.len() < length {
            return;
        }
        let target = unsafe { std::slice::from_raw_parts_mut(self.bits, length) };
        for (destination, source) in target.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
            destination[0] = source[2];
            destination[1] = source[1];
            destination[2] = source[0];
            destination[3] = source[3];
        }
    }
}

impl Drop for Surface {
    /// Frees the GDI objects.
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap as HGDIOBJ);
            DeleteDC(self.dc);
        }
    }
}

/// Everything the window needs between messages.
struct WindowState {
    hwnd: HWND,
    engine: Engine,
    canvas: Canvas,
    surface: Option<Surface>,
    banner: Banner,
    tray: Option<Tray>,
    settings: AppSettings,
    cats: Vec<CatSkinConfig>,
    device_scale: f32,
    cursor_kind: CursorKind,
    over_cat: bool,
    alt_override: bool,
    drag: Option<DragState>,
    scale_preview: Option<ScalePreview>,
    petting_cursor: HCURSOR,
    arrow_cursor: HCURSOR,
    dragging_cursor: HCURSOR,
    quitting: bool,
}

thread_local! {
    static WINDOW: RefCell<Option<WindowState>> = const { RefCell::new(None) };
}

/// Runs a function with the window state, unless it is already in use (re-entrant message).
///
/// @param action The function to run.
/// @return The function's result, or None when the state was busy or missing.
fn with_state<R>(action: impl FnOnce(&mut WindowState) -> R) -> Option<R> {
    WINDOW.with(|cell| match cell.try_borrow_mut() {
        Ok(mut guard) => guard.as_mut().map(action),
        Err(_) => None,
    })
}

/// Public handle to the pet window, returned by [`PetWindow::create`].
pub struct PetWindow {
    hwnd: HWND,
    timer: HANDLE,
}

impl PetWindow {
    /// Creates the (still hidden) pet window for the given settings.
    ///
    /// @param settings The settings at start-up.
    /// @return The window, or an error message.
    pub fn create(settings: &AppSettings) -> Result<PetWindow, String> {
        unsafe {
            SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let class_name = wide("KuroCatPetWindow");
        let arrow_cursor = unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) };
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(window_procedure),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: unsafe { LoadIconW(instance, 1 as _) },
            hCursor: arrow_cursor,
            hbrBackground: std::ptr::null_mut(),
            lpszMenuName: std::ptr::null(),
            lpszClassName: class_name.as_ptr(),
            hIconSm: std::ptr::null_mut(),
        };
        if unsafe { RegisterClassExW(&class) } == 0 {
            return Err("RegisterClassExW failed".to_string());
        }

        let system_scale = (unsafe { GetDpiForSystem() }.max(96) as f64) / 96.0;
        let (logical_width, logical_height) = app::window_dimensions(&settings.keyboard_layout, settings.scale);
        let width = (logical_width * system_scale).round() as i32;
        let height = (logical_height * system_scale).round() as i32;
        let (x, y) = initial_position(settings, width, height);

        let mut ex_style = WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        if settings.always_on_top {
            ex_style |= WS_EX_TOPMOST;
        }
        let title = wide("KuroCat");
        let hwnd = unsafe {
            CreateWindowExW(
                ex_style,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                x,
                y,
                width,
                height,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err("CreateWindowExW failed".to_string());
        }
        events::set_main_window(hwnd as isize);

        let timer = create_frame_timer();
        let device_scale = window_device_scale(hwnd);
        let mut state = WindowState {
            hwnd,
            engine: Engine::new(),
            canvas: Canvas::new(1, 1),
            surface: None,
            banner: Banner::new(),
            tray: None,
            settings: settings.clone(),
            cats: cats::all_cats(),
            device_scale,
            cursor_kind: CursorKind::Arrow,
            over_cat: false,
            alt_override: false,
            drag: None,
            scale_preview: None,
            petting_cursor: create_petting_cursor(device_scale).unwrap_or(arrow_cursor),
            arrow_cursor,
            dragging_cursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_SIZEALL) },
            quitting: false,
        };
        state.apply_settings(settings.clone(), true);
        // Match the window to the real DPI of the monitor it landed on. The saved position is the
        // top-left corner, so keep that corner and do not save, or the pet would creep on every start.
        state.set_window_size(&settings.keyboard_layout, settings.scale, false);
        WINDOW.with(|cell| *cell.borrow_mut() = Some(state));

        Ok(PetWindow { hwnd, timer })
    }

    /// Shows the window and runs the message and frame loop until the user quits.
    ///
    /// @param tray The tray icon, if it could be created.
    pub fn run(&mut self, tray: Option<Tray>) {
        with_state(|state| {
            state.tray = tray;
            state.render();
        });
        tray_icon::TrayIconEvent::set_event_handler(Some(on_tray_icon_event));
        unsafe {
            ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }

        let mut next_frame_at = now_ms();
        loop {
            let now = now_ms();
            if now >= next_frame_at {
                if TRAY_MENU_TIMER_RUNNING.swap(false, Ordering::SeqCst) {
                    unsafe {
                        KillTimer(self.hwnd, TRAY_MENU_TIMER);
                    }
                }
                let frame_ms = with_state(|state| state.frame()).unwrap_or(ACTIVE_FRAME_MS);
                next_frame_at = now + frame_ms;
                if next_frame_at < now_ms() {
                    next_frame_at = now_ms() + 1.0;
                }
            }
            arm_frame_timer(self.timer, next_frame_at - now_ms());

            let handles = [self.timer];
            let wait_count = if self.timer.is_null() { 0 } else { 1 };
            let timeout = if self.timer.is_null() {
                ((next_frame_at - now_ms()).max(1.0)) as u32
            } else {
                INFINITE
            };
            let result = unsafe {
                MsgWaitForMultipleObjectsEx(wait_count, handles.as_ptr(), timeout, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
            };
            if result != WAIT_OBJECT_0 || wait_count == 0 {
                if !pump_messages() {
                    break;
                }
            }
        }

        WINDOW.with(|cell| *cell.borrow_mut() = None);
        if !self.timer.is_null() {
            unsafe {
                CloseHandle(self.timer);
            }
        }
    }
}

/// Handles all waiting window messages.
///
/// @return False when the application should quit.
fn pump_messages() -> bool {
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if message.message == WM_QUIT {
                return false;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    true
}

/// Creates the high-resolution timer that wakes the frame loop.
///
/// @return The timer handle, or null when no timer could be created (the loop then uses timeouts).
fn create_frame_timer() -> HANDLE {
    unsafe {
        let timer = CreateWaitableTimerExW(
            std::ptr::null(),
            std::ptr::null(),
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS,
        );
        if !timer.is_null() {
            return timer;
        }
        CreateWaitableTimerW(std::ptr::null(), 1, std::ptr::null())
    }
}

/// Sets the frame timer to fire once after a delay.
///
/// @param timer The timer handle (ignored when null).
/// @param delay_ms Delay in milliseconds.
fn arm_frame_timer(timer: HANDLE, delay_ms: f64) {
    if timer.is_null() {
        return;
    }
    // Negative due time = relative, in 100-nanosecond units.
    let due_time: i64 = -((delay_ms.max(0.5) * 10_000.0) as i64);
    unsafe {
        SetWaitableTimer(timer, &due_time, 0, None, std::ptr::null(), 0);
    }
}

/// Returns the physical pixels per logical pixel of the monitor the window is on.
///
/// @param hwnd The window.
/// @return 1.0 at 96 DPI, 1.5 at 144 DPI and so on.
fn window_device_scale(hwnd: HWND) -> f32 {
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f32 / 96.0
    }
}

/// Keeps a window rectangle inside the work area of the monitor the window is on. Monitors left
/// of or above the primary one have negative coordinates, so this never clamps to 0.
///
/// @param hwnd The window.
/// @param x Wanted left edge in physical pixels.
/// @param y Wanted top edge in physical pixels.
/// @param width Window width in physical pixels.
/// @param height Window height in physical pixels.
/// @return The (x, y) to use.
fn clamp_to_work_area(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
    let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_null() || unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return (x, y);
    }
    let work = info.rcWork;
    let clamped_x = x.min(work.right - width).max(work.left);
    let clamped_y = y.min(work.bottom - height).max(work.top);
    (clamped_x, clamped_y)
}

/// Default window position: near the bottom-right corner of the primary screen.
///
/// @param width Window width in physical pixels.
/// @param height Window height in physical pixels.
/// @return (x, y) in physical screen pixels.
fn default_bottom_right_position(width: i32, height: i32) -> (i32, i32) {
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    ((screen_width - width - 28).max(20), (screen_height - height - 68).max(20))
}

/// Picks the start position: the saved one when it is still on a monitor, else the default.
///
/// @param settings The settings.
/// @param width Window width in physical pixels.
/// @param height Window height in physical pixels.
/// @return (x, y) in physical screen pixels.
fn initial_position(settings: &AppSettings, width: i32, height: i32) -> (i32, i32) {
    if let (Some(x), Some(y)) = (settings.x, settings.y) {
        let centre = POINT { x: x + width / 2, y: y + height / 2 };
        let monitor = unsafe { MonitorFromPoint(centre, MONITOR_DEFAULTTONULL) };
        if !monitor.is_null() {
            return (x, y);
        }
    }
    default_bottom_right_position(width, height)
}

/// Copies text to the clipboard.
///
/// @param hwnd The window that owns the clipboard while copying.
/// @param text The text.
/// @return True on success.
fn copy_text_to_clipboard(hwnd: HWND, text: &str) -> bool {
    let wide_text = wide(text);
    let byte_count = wide_text.len() * 2;
    unsafe {
        if OpenClipboard(hwnd) == 0 {
            return false;
        }
        EmptyClipboard();
        let memory = GlobalAlloc(GMEM_MOVEABLE, byte_count);
        if memory.is_null() {
            CloseClipboard();
            return false;
        }
        let target = GlobalLock(memory) as *mut u16;
        if target.is_null() {
            CloseClipboard();
            return false;
        }
        std::ptr::copy_nonoverlapping(wide_text.as_ptr(), target, wide_text.len());
        GlobalUnlock(memory);
        let stored = !SetClipboardData(CLIPBOARD_UNICODE_TEXT, memory as HANDLE).is_null();
        CloseClipboard();
        stored
    }
}

/// Draws the open-hand petting cursor (the same shape as the old CSS cursor) into a Win32 cursor.
///
/// @param device_scale Physical pixels per logical pixel.
/// @return The cursor, or None when it could not be created.
fn create_petting_cursor(device_scale: f32) -> Option<HCURSOR> {
    let size = (26.0 * device_scale).round().max(16.0) as u32;
    let mut canvas = Canvas::new(size, size);
    let unit = size as f32 / 24.0;
    canvas.scale(unit, unit);
    canvas.begin_path();
    // Index finger.
    canvas.move_to(9.0, 11.0);
    canvas.line_to(9.0, 5.5);
    canvas.arc(10.5, 5.5, 1.5, std::f32::consts::PI, std::f32::consts::TAU, false);
    canvas.line_to(12.0, 10.0);
    // Middle finger.
    canvas.move_to(12.0, 4.5);
    canvas.arc(13.5, 4.5, 1.5, std::f32::consts::PI, std::f32::consts::TAU, false);
    canvas.line_to(15.0, 10.0);
    // Ring finger.
    canvas.move_to(15.0, 6.0);
    canvas.arc(16.5, 6.0, 1.5, std::f32::consts::PI, std::f32::consts::TAU, false);
    canvas.line_to(18.0, 11.0);
    // Little finger, palm and thumb.
    canvas.move_to(18.0, 8.5);
    canvas.arc(19.5, 8.5, 1.5, std::f32::consts::PI, std::f32::consts::TAU, false);
    canvas.line_to(21.0, 14.0);
    canvas.arc(14.0, 14.0, 7.0, 0.0, std::f32::consts::FRAC_PI_2, false);
    canvas.line_to(12.5, 21.0);
    canvas.arc(12.5, 14.5, 6.5, std::f32::consts::FRAC_PI_2, 2.555, false);
    canvas.line_to(4.5, 14.0);
    canvas.arc(5.7, 13.1, 1.5, 2.498, 5.640, false);
    canvas.line_to(7.0, 14.0);
    canvas.line_to(7.0, 7.5);
    canvas.arc(8.0, 8.618, 1.5, 3.983, 5.442, false);
    canvas.close_path();
    canvas.set_fill_color("#16171a");
    canvas.fill();
    canvas.set_stroke_color("#ffffff");
    canvas.set_line_width(1.65);
    canvas.set_line_join(LineJoin::Round);
    canvas.stroke();

    let hotspot = (12.0 * size as f32 / 26.0).round() as u32;
    unsafe {
        let mut surface = Surface::create(size as i32, size as i32)?;
        surface.copy_from_rgba(canvas.pixels());
        // The colour bitmap must be detached from its DC before it is used in an icon.
        SelectObject(surface.dc, surface.previous);
        let mask_bits = vec![0u8; ((size as usize + 15) / 16) * 2 * size as usize];
        let mask = CreateBitmap(size as i32, size as i32, 1, 1, mask_bits.as_ptr() as *const _);
        let info = ICONINFO {
            fIcon: 0,
            xHotspot: hotspot,
            yHotspot: hotspot,
            hbmMask: mask,
            hbmColor: surface.bitmap,
        };
        let cursor = CreateIconIndirect(&info);
        DeleteObject(mask as HGDIOBJ);
        SelectObject(surface.dc, surface.bitmap as HGDIOBJ);
        if cursor.is_null() {
            None
        } else {
            Some(cursor as HCURSOR)
        }
    }
}

impl WindowState {
    /// Runs one frame: handles queued events, menu and tray events, advances the animation and
    /// draws.
    ///
    /// @return How many milliseconds until the next frame.
    fn frame(&mut self) -> f64 {
        self.drain_events();
        self.drain_menu_events();
        if self.quitting {
            return ACTIVE_FRAME_MS;
        }

        // Hidden (or paused for anti-cheat): nothing to draw. Events still wake the loop.
        if unsafe { IsWindowVisible(self.hwnd) } == 0 {
            return HIDDEN_FRAME_MS;
        }
        // WM_DPICHANGED is lost when it arrives while the window state is busy (for example
        // during a drag across monitors), so compare the DPI here too.
        if self.drag.is_none() && window_device_scale(self.hwnd) != self.device_scale {
            self.refresh_for_dpi();
        }

        let now = now_ms();
        if let Some(preview) = self.scale_preview {
            if now >= preview.commit_at {
                self.scale_preview = None;
                app::apply_scale_and_save(preview.scale);
            }
        }
        self.banner.tick(now);
        self.engine.tick(now);
        self.render();

        let busy = self.drag.is_some() || self.scale_preview.is_some() || self.banner.is_animating();
        if busy || !self.engine.is_idle(now) {
            ACTIVE_FRAME_MS
        } else {
            IDLE_FRAME_MS
        }
    }

    /// Draws the pet and the banner and pushes the pixels to the layered window.
    fn render(&mut self) {
        let mut window_rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe {
            GetWindowRect(self.hwnd, &mut window_rect);
        }
        let width = (window_rect.right - window_rect.left).max(1);
        let height = (window_rect.bottom - window_rect.top).max(1);
        if self.canvas.width() != width as u32 || self.canvas.height() != height as u32 {
            self.canvas.resize(width as u32, height as u32);
        }
        let surface_matches = match &self.surface {
            Some(surface) => surface.width == width && surface.height == height,
            None => false,
        };
        if !surface_matches {
            self.surface = Surface::create(width, height);
        }

        self.engine.render(&mut self.canvas, self.device_scale);
        let logical_width = width as f32 / self.device_scale;
        self.banner.draw(&mut self.canvas, logical_width, self.device_scale);

        let Some(surface) = self.surface.as_mut() else {
            return;
        };
        surface.copy_from_rgba(self.canvas.pixels());
        let size = SIZE { cx: width, cy: height };
        let source = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            let screen = GetDC(std::ptr::null_mut());
            UpdateLayeredWindow(
                self.hwnd,
                screen,
                std::ptr::null(),
                &size,
                surface.dc,
                &source,
                0,
                &blend,
                ULW_ALPHA,
            );
            ReleaseDC(std::ptr::null_mut(), screen);
        }
    }

    /// Handles every event queued by the background threads.
    fn drain_events(&mut self) {
        for event in events::drain() {
            self.handle_event(event);
        }
    }

    /// Handles one event from a background thread.
    ///
    /// @param event The event.
    fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(input) => self.handle_input(input),
            AppEvent::Privacy(active) => self.engine.set_privacy_mode(active),
            AppEvent::Cursor(info) => {
                self.over_cat = info.over_cat;
                self.alt_override = info.alt_override;
                self.engine.on_window_cursor(info.rel_x as f32, info.rel_y as f32, info.over_cat);
                self.update_cursor_kind();
            }
            AppEvent::Settings(settings) => self.apply_settings(settings, false),
            AppEvent::Cats(cat_list) => {
                if !cat_list.is_empty() {
                    self.cats = cat_list;
                }
                if let Some(tray) = self.tray.as_mut() {
                    tray.rebuild_cats(&self.cats, &self.settings.cat_breed);
                }
                self.apply_appearance();
            }
            AppEvent::Toast(text) => self.banner.show_message(&text),
            AppEvent::TrayIcon(icon_event) => {
                if let Some(tray) = self.tray.as_ref() {
                    tray.handle_icon_event(&icon_event);
                }
            }
            AppEvent::Window(command) => self.handle_window_command(command),
            AppEvent::Quit => {
                self.quitting = true;
                unsafe {
                    DestroyWindow(self.hwnd);
                }
            }
        }
    }

    /// Passes a global keyboard or mouse event to the engine.
    ///
    /// @param input The event.
    fn handle_input(&mut self, input: InputEvent) {
        match input {
            InputEvent::KeyDown { vk, repeat, .. } => self.engine.on_key_down(vk, repeat),
            InputEvent::KeyUp { vk, .. } => self.engine.on_key_up(vk),
            InputEvent::MouseMove { nx, ny, .. } => self.engine.on_global_mouse_move(nx as f32, ny as f32),
            InputEvent::MouseDown { button } => self.engine.on_mouse_down(&button),
            InputEvent::MouseUp { button } => self.engine.on_mouse_up(&button),
            InputEvent::MouseWheel { .. } => self.engine.on_mouse_wheel(),
            InputEvent::PrivacyTyping => self.engine.on_privacy_typing(),
        }
    }

    /// Handles menu clicks from the right-click menu and the tray, and tray icon clicks.
    fn drain_menu_events(&mut self) {
        while let Ok(menu_event) = muda::MenuEvent::receiver().try_recv() {
            let id = menu_event.id().0.clone();
            if let Some(outcome) = menus::handle_context_menu_id(&id) {
                self.handle_menu_outcome(outcome);
            } else if let Some(tray) = self.tray.as_ref() {
                tray.handle_menu_event(&id);
            }
        }
        // Menu actions send events of their own (settings, toasts…): handle them in this frame.
        self.drain_events();
    }

    /// Finishes a context-menu action that needs the window.
    ///
    /// @param outcome What is left to do.
    fn handle_menu_outcome(&mut self, outcome: MenuOutcome) {
        match outcome {
            MenuOutcome::Done => {}
            MenuOutcome::CopyMcpConfig => {
                let copied = copy_text_to_clipboard(self.hwnd, &app::mcp_config_json());
                let message = if copied {
                    "✓ MCP config copied to the clipboard"
                } else {
                    "⚠ Could not open the clipboard"
                };
                self.banner.show_message(message);
            }
        }
    }

    /// Applies new settings to the engine, the tray and the window.
    ///
    /// @param settings The new settings.
    /// @param first True at start-up (no notices are shown).
    fn apply_settings(&mut self, settings: AppSettings, first: bool) {
        let lock_changed = settings.click_through_locked != self.settings.click_through_locked;
        self.settings = settings;

        self.engine.show_key_bubbles = self.settings.show_key_bubbles;
        self.apply_appearance();
        self.engine.set_keyboard_config(
            KeyboardLayoutId::from_setting(&self.settings.keyboard_layout),
            KeyboardOrientation::from_setting(&self.settings.keyboard_orientation),
        );
        if self.scale_preview.is_none() {
            self.engine.set_render_scale(self.settings.scale as f32);
        }
        crate::audio::set_sound_enabled(self.settings.sound_enabled);
        if let Some(tray) = self.tray.as_ref() {
            tray.sync(&self.settings);
        }

        if lock_changed && !first {
            if self.settings.click_through_locked {
                self.banner.show_message("🔒 Click-through locked (hold Alt to click, or unlock from the tray)");
            } else {
                self.banner.show_message("Click-through unlocked: drag or right-click the cat");
            }
        }
        self.update_cursor_kind();
    }

    /// Gives the engine the active cat.
    fn apply_appearance(&mut self) {
        let breed = self.settings.cat_breed.clone();
        let active = self.cats.iter().find(|cat| cat.id == breed);
        match active {
            Some(cat) => self.engine.set_cat_appearance(&cat.id, Some(cat)),
            None => self.engine.set_cat_appearance("black", None),
        }
    }

    /// Runs a window command sent from another thread.
    ///
    /// @param command The command.
    fn handle_window_command(&mut self, command: WindowCommand) {
        match command {
            WindowCommand::Show => self.show(),
            WindowCommand::Hide => unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            },
            WindowCommand::ToggleVisible => {
                if unsafe { IsWindowVisible(self.hwnd) } != 0 {
                    unsafe {
                        ShowWindow(self.hwnd, SW_HIDE);
                    }
                } else {
                    self.show();
                }
            }
            WindowCommand::SetAlwaysOnTop(on_top) => self.set_topmost(on_top),
            WindowCommand::ReassertTopmost => {
                if self.settings.always_on_top {
                    self.set_topmost(true);
                }
            }
            WindowCommand::Resize { layout, scale } => {
                if self.scale_preview.is_none() {
                    self.resize_window(&layout, scale);
                }
            }
            WindowCommand::ResetPosition => self.reset_position(),
        }
    }

    /// Shows the window without activating it and puts it back on top.
    fn show(&mut self) {
        self.render();
        unsafe {
            ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
        }
        if self.settings.always_on_top {
            self.set_topmost(true);
        }
    }

    /// Puts the window into (or out of) the topmost band without activating it.
    ///
    /// @param on_top True for topmost.
    fn set_topmost(&self, on_top: bool) {
        let insert_after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
        unsafe {
            SetWindowPos(self.hwnd, insert_after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    /// Resizes the window for a layout and scale, keeping its bottom-right corner in place, and
    /// saves the new position.
    ///
    /// @param layout "65" or "108".
    /// @param scale The pet scale.
    fn resize_window(&mut self, layout: &str, scale: f64) {
        self.set_window_size(layout, scale, true);
    }

    /// Sizes the window for a layout and scale at the DPI of its monitor, keeping it on the
    /// monitor's work area.
    ///
    /// @param layout "65" or "108".
    /// @param scale The pet scale.
    /// @param keep_bottom_right True to keep the bottom-right corner in place and save the new
    ///        position; false to keep the top-left corner and save nothing.
    fn set_window_size(&mut self, layout: &str, scale: f64, keep_bottom_right: bool) {
        self.device_scale = window_device_scale(self.hwnd);
        let (logical_width, logical_height) = app::window_dimensions(layout, scale);
        let width = (logical_width * self.device_scale as f64).round() as i32;
        let height = (logical_height * self.device_scale as f64).round() as i32;
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe {
            GetWindowRect(self.hwnd, &mut rect);
        }
        if rect.right - rect.left == width && rect.bottom - rect.top == height {
            return;
        }
        let (wanted_x, wanted_y) = if keep_bottom_right {
            (rect.right - width, rect.bottom - height)
        } else {
            (rect.left, rect.top)
        };
        let (x, y) = clamp_to_work_area(self.hwnd, wanted_x, wanted_y, width, height);
        unsafe {
            SetWindowPos(self.hwnd, std::ptr::null_mut(), x, y, width, height, SWP_NOZORDER | SWP_NOACTIVATE);
        }
        if keep_bottom_right && self.settings.x.is_some() {
            app::save_window_position(x, y);
        }
    }

    /// Moves the window back to the default bottom-right corner and saves that position.
    fn reset_position(&mut self) {
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe {
            GetWindowRect(self.hwnd, &mut rect);
        }
        let (x, y) = default_bottom_right_position(rect.right - rect.left, rect.bottom - rect.top);
        unsafe {
            SetWindowPos(self.hwnd, std::ptr::null_mut(), x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
        app::save_window_position(x, y);
    }

    /// Reports whether the user may interact with the pet right now (not locked, or Alt held).
    ///
    /// @return True when clicks should drag or open the menu.
    fn interaction_allowed(&self) -> bool {
        let alt_held = unsafe { GetAsyncKeyState(VK_MENU as i32) } < 0;
        !self.settings.click_through_locked || self.alt_override || alt_held
    }

    /// Picks the pointer: grabbing while dragging, a hand over the cat, else the arrow.
    fn update_cursor_kind(&mut self) {
        self.cursor_kind = if self.drag.is_some() {
            CursorKind::Dragging
        } else if self.over_cat && (!self.settings.click_through_locked || self.alt_override) {
            CursorKind::Petting
        } else {
            CursorKind::Arrow
        };
    }

    /// @return The Win32 cursor for the current pointer kind.
    fn current_cursor(&self) -> HCURSOR {
        match self.cursor_kind {
            CursorKind::Arrow => self.arrow_cursor,
            CursorKind::Petting => self.petting_cursor,
            CursorKind::Dragging => self.dragging_cursor,
        }
    }

    /// Converts client pixels to logical window pixels.
    ///
    /// @param lparam The mouse message parameter.
    /// @return (x, y) in logical pixels.
    fn logical_point(&self, lparam: LPARAM) -> (f32, f32) {
        (mouse_x(lparam) as f32 / self.device_scale, mouse_y(lparam) as f32 / self.device_scale)
    }

    /// Left button pressed: banner click areas take priority, otherwise a drag starts.
    ///
    /// @param lparam The mouse message parameter.
    fn on_left_button_down(&mut self, lparam: LPARAM) {
        let (x, y) = self.logical_point(lparam);
        if self.banner.contains(x, y) {
            return;
        }
        if !self.interaction_allowed() {
            return;
        }
        let mut cursor = POINT { x: 0, y: 0 };
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe {
            GetCursorPos(&mut cursor);
            GetWindowRect(self.hwnd, &mut rect);
            SetCapture(self.hwnd);
        }
        self.drag = Some(DragState {
            grab_offset_x: cursor.x - rect.left,
            grab_offset_y: cursor.y - rect.top,
        });
        app::state().dragging.store(true, Ordering::Relaxed);
        self.update_cursor_kind();
        unsafe {
            SetCursor(self.current_cursor());
        }
    }

    /// Mouse moved over the window: moves the window while dragging, else updates banner hover.
    ///
    /// @param lparam The mouse message parameter.
    fn on_mouse_move(&mut self, lparam: LPARAM) {
        if let Some(drag) = self.drag {
            let mut cursor = POINT { x: 0, y: 0 };
            unsafe {
                GetCursorPos(&mut cursor);
                SetWindowPos(
                    self.hwnd,
                    std::ptr::null_mut(),
                    cursor.x - drag.grab_offset_x,
                    cursor.y - drag.grab_offset_y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            return;
        }
        let (x, y) = self.logical_point(lparam);
        self.banner.update_hover(x, y);
    }

    /// Left button released: ends a drag (saving the position) or clicks a banner button.
    ///
    /// @param lparam The mouse message parameter.
    fn on_left_button_up(&mut self, lparam: LPARAM) {
        if self.drag.is_some() {
            unsafe {
                ReleaseCapture();
            }
            self.finish_drag();
            return;
        }
        let (x, y) = self.logical_point(lparam);
        self.banner.handle_click(x, y);
    }

    /// Ends a drag and saves the window position.
    fn finish_drag(&mut self) {
        if self.drag.take().is_none() {
            return;
        }
        app::state().dragging.store(false, Ordering::Relaxed);
        let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        unsafe {
            GetWindowRect(self.hwnd, &mut rect);
        }
        app::save_window_position(rect.left, rect.top);
        self.update_cursor_kind();
    }

    /// Ctrl + wheel over the pet: previews a new scale now and saves it shortly after.
    ///
    /// @param wparam The wheel message parameter.
    fn on_mouse_wheel(&mut self, wparam: WPARAM) {
        if (wparam & WHEEL_CONTROL_FLAG) == 0 || !self.interaction_allowed() {
            return;
        }
        let delta = ((wparam >> 16) & 0xffff) as u16 as i16;
        if delta == 0 {
            return;
        }
        let current = match self.scale_preview {
            Some(preview) => preview.scale,
            None => self.settings.scale,
        };
        let step = if delta > 0 { WHEEL_SCALE_STEP } else { -WHEEL_SCALE_STEP };
        let snapped = ((current + step) * 20.0).round() / 20.0;
        let next = snapped.clamp(MIN_SCALE, MAX_SCALE);
        self.scale_preview = Some(ScalePreview { scale: next, commit_at: now_ms() + SCALE_COMMIT_DELAY_MS });
        self.engine.set_render_scale(next as f32);
        let layout = self.settings.keyboard_layout.clone();
        self.resize_window(&layout, next);
        self.banner.show_message(&format!("Size {}%", (next * 100.0).round() as i32));
    }

    /// The monitor DPI changed (the window moved to another monitor or the user changed scaling).
    ///
    /// @param lparam Pointer to the suggested new window rectangle.
    fn on_dpi_changed(&mut self, lparam: LPARAM) {
        let suggested = unsafe { &*(lparam as *const RECT) };
        self.device_scale = window_device_scale(self.hwnd);
        unsafe {
            SetWindowPos(
                self.hwnd,
                std::ptr::null_mut(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        self.refresh_for_dpi();
    }

    /// Rebuilds what depends on the monitor DPI: the petting cursor and the window size.
    fn refresh_for_dpi(&mut self) {
        self.device_scale = window_device_scale(self.hwnd);
        if let Some(cursor) = create_petting_cursor(self.device_scale) {
            let old_cursor = self.petting_cursor;
            self.petting_cursor = cursor;
            if old_cursor != self.arrow_cursor {
                unsafe {
                    SetCursor(self.current_cursor());
                    DestroyCursor(old_cursor);
                }
            }
        }
        let layout = self.settings.keyboard_layout.clone();
        let scale = self.settings.scale;
        self.resize_window(&layout, scale);
    }
}

/// Opens the right-click menu (blocks until it closes). The pet keeps animating through a
/// timer while the menu's modal loop runs.
///
/// @param hwnd The pet window.
fn open_context_menu(hwnd: HWND) {
    let prepared = with_state(|state| {
        if state.banner.is_visible() {
            let mut cursor = POINT { x: 0, y: 0 };
            let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            unsafe {
                GetCursorPos(&mut cursor);
                GetWindowRect(hwnd, &mut rect);
            }
            let x = (cursor.x - rect.left) as f32 / state.device_scale;
            let y = (cursor.y - rect.top) as f32 / state.device_scale;
            if state.banner.contains(x, y) {
                return None;
            }
        }
        if !state.interaction_allowed() {
            return None;
        }
        Some(menus::build_context_menu(&state.settings, &state.cats))
    });
    let Some(Some(menu)) = prepared else {
        return;
    };

    app::state().menu_open.store(true, Ordering::Relaxed);
    unsafe {
        SetTimer(hwnd, MENU_ANIMATION_TIMER, ACTIVE_FRAME_MS as u32, None);
    }
    menus::show_context_menu(&menu, hwnd as isize);
    unsafe {
        KillTimer(hwnd, MENU_ANIMATION_TIMER);
    }
    app::state().menu_open.store(false, Ordering::Relaxed);
}

/// Receives tray icon events on the UI thread. A right-button press opens the tray menu right
/// after this returns, and its modal loop blocks the frame loop, so a timer keeps the pet
/// animating until the menu closes.
///
/// @param event The tray icon event.
fn on_tray_icon_event(event: tray_icon::TrayIconEvent) {
    let opens_menu = matches!(
        event,
        tray_icon::TrayIconEvent::Click {
            button: tray_icon::MouseButton::Right,
            button_state: tray_icon::MouseButtonState::Down,
            ..
        }
    );
    if opens_menu {
        let hwnd = events::main_window() as HWND;
        if !hwnd.is_null() {
            TRAY_MENU_TIMER_RUNNING.store(true, Ordering::SeqCst);
            unsafe {
                SetTimer(hwnd, TRAY_MENU_TIMER, ACTIVE_FRAME_MS as u32, None);
            }
        }
    }
    events::send(AppEvent::TrayIcon(event));
}

/// The window procedure of the pet window.
///
/// @param hwnd The window.
/// @param message The message id.
/// @param wparam First message parameter.
/// @param lparam Second message parameter.
/// @return The message result.
unsafe extern "system" fn window_procedure(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_APP_EVENT => {
            with_state(|state| state.drain_events());
            0
        }
        WM_TIMER => {
            if wparam == MENU_ANIMATION_TIMER || wparam == TRAY_MENU_TIMER {
                with_state(|state| state.frame());
            }
            0
        }
        WM_SETCURSOR => {
            if (lparam & 0xffff) as u32 == HTCLIENT {
                if let Some(cursor) = with_state(|state| state.current_cursor()) {
                    SetCursor(cursor);
                    return 1;
                }
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_LBUTTONDOWN => {
            with_state(|state| state.on_left_button_down(lparam));
            0
        }
        WM_MOUSEMOVE => {
            with_state(|state| state.on_mouse_move(lparam));
            0
        }
        WM_LBUTTONUP => {
            with_state(|state| state.on_left_button_up(lparam));
            0
        }
        WM_CAPTURECHANGED => {
            with_state(|state| state.finish_drag());
            0
        }
        WM_RBUTTONUP => {
            open_context_menu(hwnd);
            0
        }
        WM_MOUSEWHEEL => {
            with_state(|state| state.on_mouse_wheel(wparam));
            0
        }
        WM_DPICHANGED => {
            with_state(|state| state.on_dpi_changed(lparam));
            0
        }
        WM_CLOSE => 0,
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}
