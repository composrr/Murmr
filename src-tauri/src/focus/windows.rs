//! Windows focus / caret discovery.
//!
//! Two strategies, in order of preference:
//!
//! 1. **UI Automation** (`uia_focused_element_rect`) — works for Chrome, VS
//!    Code, Slack, Electron, modern Office, WinUI 3 (incl. Win11 Notepad),
//!    and anything else that exposes accessibility (basically all modern
//!    apps). This is how Wispr Flow finds your input field.
//! 2. **Legacy GUI thread caret** (`focused_caret_screen_rect`) — narrow
//!    fallback for pre-UIA apps that still rely on the Win32 caret API.

use std::cell::RefCell;
use std::ptr;

use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows_sys::Win32::Foundation::{CloseHandle, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GetWindowThreadProcessId,
    SetForegroundWindow, GUITHREADINFO,
};

use super::ScreenRect;

// ---------------------------------------------------------------------------
// Foreground app / window identity — used to (a) keep injection targeted at
// the window the user started dictating in, and (b) label the Insights
// "where you dictate" breakdown.
// ---------------------------------------------------------------------------

/// Base name of the foreground window's executable (e.g. "Code.exe").
pub fn foreground_app() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 {
            return None;
        }
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 260];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        if ok == 0 || size == 0 {
            return None;
        }
        let full = String::from_utf16_lossy(&buf[..size as usize]);
        let base = full.rsplit(['\\', '/']).next().unwrap_or(&full);
        if base.is_empty() {
            None
        } else {
            Some(base.to_string())
        }
    }
}

/// Opaque id (the HWND as an integer, so it's `Send`) of the foreground
/// window. Compared later to detect whether focus moved during transcription.
pub fn foreground_window_id() -> Option<isize> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            None
        } else {
            Some(hwnd as isize)
        }
    }
}

/// Best-effort restore of foreground focus to a window captured earlier.
/// Returns true if the OS accepted the request (subject to Windows'
/// foreground-lock rules — not guaranteed to actually foreground it).
pub fn refocus_window(id: isize) -> bool {
    unsafe { SetForegroundWindow(id as _) != 0 }
}

/// True if the captured window still exists.
pub fn window_exists(id: isize) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
    unsafe { IsWindow(id as _) != 0 }
}

/// Restore foreground to `id` and confirm it actually took. Used by the
/// edit-last flow, which must call this while Murmr's HUD still owns the
/// foreground: Windows only lets the current foreground process hand focus
/// to another window, so once the HUD is hidden a plain
/// `SetForegroundWindow` can silently fail (taskbar flash, no focus).
///
/// Tries `SetForegroundWindow` first, then the `AttachThreadInput` fallback,
/// polling `GetForegroundWindow` until it matches or `timeout` passes.
pub fn refocus_window_verified(id: isize, timeout: std::time::Duration) -> bool {
    use std::time::Instant;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        BringWindowToTop, IsIconic, IsWindow, ShowWindow, SW_RESTORE,
    };

    let wait_for = |hwnd: isize, budget: std::time::Duration| -> bool {
        let deadline = Instant::now() + budget;
        loop {
            if unsafe { GetForegroundWindow() } as isize == hwnd {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    };

    unsafe {
        let hwnd = id as _;
        if IsWindow(hwnd) == 0 {
            return false;
        }
        if GetForegroundWindow() as isize == id {
            return true;
        }
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd);
        if wait_for(id, timeout / 2) {
            return true;
        }

        // Fallback: share the foreground thread's input state so Windows
        // treats our request as coming from the foreground.
        let fg = GetForegroundWindow();
        let fg_thread = if fg.is_null() {
            0
        } else {
            GetWindowThreadProcessId(fg, ptr::null_mut())
        };
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        BringWindowToTop(hwnd);
        SetForegroundWindow(hwnd);
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
        wait_for(id, timeout / 2)
    }
}

// ---------------------------------------------------------------------------
// UI Automation (preferred — covers virtually every modern app)
// ---------------------------------------------------------------------------

thread_local! {
    /// Cached IUIAutomation per controller thread. COM is single-threaded
    /// for STA so we hold one per thread; a single dictation hotkey + one
    /// controller thread means we only ever init COM once for the lifetime
    /// of the app.
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
}

fn ui_automation() -> Option<IUIAutomation> {
    UIA.with(|cell| {
        if cell.borrow().is_some() {
            return cell.borrow().clone();
        }
        unsafe {
            // OK if COM was already initialized on this thread by something
            // else — CoInitializeEx returns S_FALSE in that case.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let res = CoCreateInstance::<_, IUIAutomation>(
                &CUIAutomation,
                None,
                CLSCTX_INPROC_SERVER,
            );
            match res {
                Ok(a) => {
                    *cell.borrow_mut() = Some(a.clone());
                    Some(a)
                }
                Err(e) => {
                    eprintln!("[focus] UIA CoCreateInstance failed: {e:?}");
                    None
                }
            }
        }
    })
}

/// Bounding rect (screen coords) of the currently focused accessible element.
pub fn uia_focused_element_rect() -> Option<ScreenRect> {
    let auto = ui_automation()?;
    unsafe {
        let element = auto.GetFocusedElement().ok()?;
        let rc = element.CurrentBoundingRectangle().ok()?;
        let width = rc.right - rc.left;
        let height = rc.bottom - rc.top;
        if width <= 0 || height <= 0 {
            return None;
        }
        // Reject the whole-screen / whole-window case where UIA returns the
        // entire desktop or the foreground window itself (happens when no
        // specific control has focus). Heuristic: anything taller than ~600px
        // is almost never a single text field.
        if height > 600 {
            return None;
        }
        // Some focused elements (window-level focus on browsers) appear at
        // the desktop origin (0,0) with a window-sized rect — also skip.
        if rc.left == 0 && rc.top == 0 && width > 800 {
            return None;
        }
        Some(ScreenRect {
            x: rc.left,
            y: rc.top,
            width,
            height,
        })
    }
}

// ---------------------------------------------------------------------------
// Legacy Win32 caret (works for old EDIT/RICHEDIT controls)
// ---------------------------------------------------------------------------

pub fn focused_caret_screen_rect() -> Option<ScreenRect> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return None;
        }

        let tid = GetWindowThreadProcessId(fg, ptr::null_mut());
        if tid == 0 {
            return None;
        }

        let mut info: GUITHREADINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
        if GetGUIThreadInfo(tid, &mut info) == 0 {
            return None;
        }

        let hwnd_caret = info.hwndCaret;
        if hwnd_caret.is_null() {
            return None;
        }

        let rc = info.rcCaret;
        if rc.right == rc.left && rc.bottom == rc.top {
            return None;
        }

        let mut tl = POINT { x: rc.left, y: rc.top };
        let mut br = POINT { x: rc.right, y: rc.bottom };
        if ClientToScreen(hwnd_caret, &mut tl) == 0 {
            return None;
        }
        if ClientToScreen(hwnd_caret, &mut br) == 0 {
            return None;
        }

        Some(ScreenRect {
            x: tl.x,
            y: tl.y,
            width: (br.x - tl.x).max(1),
            height: (br.y - tl.y).max(1),
        })
    }
}

// ---------------------------------------------------------------------------
// Foreground window rect (final fallback)
// ---------------------------------------------------------------------------

pub fn focused_window_screen_rect() -> Option<ScreenRect> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return None;
        }
        let mut rc: RECT = std::mem::zeroed();
        if GetWindowRect(fg, &mut rc) == 0 {
            return None;
        }
        let width = rc.right - rc.left;
        let height = rc.bottom - rc.top;
        if width <= 0 || height <= 0 {
            return None;
        }
        Some(ScreenRect {
            x: rc.left,
            y: rc.top,
            width,
            height,
        })
    }
}

// ---------------------------------------------------------------------------
// Fullscreen detection (foreground window covers an entire monitor)
// ---------------------------------------------------------------------------

/// True when the foreground window's bounding rect matches its
/// monitor's full bounds (within a few pixels of tolerance). Catches
/// fullscreen games, video players, and presentations. False
/// positives (a maximized window with no taskbar that happens to
/// cover the whole monitor) are mostly harmless — they just suppress
/// dictation in cases where the user probably wanted it anyway.
///
/// Uses MonitorFromWindow + GetMonitorInfo so we don't depend on
/// Tauri's monitor enumeration (the hotkey thread doesn't have an
/// AppHandle to call available_monitors on).
pub fn is_foreground_fullscreen() -> bool {
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONULL,
    };
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() {
            return false;
        }
        let mut win_rc: RECT = std::mem::zeroed();
        if GetWindowRect(fg, &mut win_rc) == 0 {
            return false;
        }
        let mon: HMONITOR = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if mon.is_null() {
            return false;
        }
        let mut info: MONITORINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(mon, &mut info) == 0 {
            return false;
        }
        // Compare the foreground window's bounding rect to the
        // monitor's TOTAL bounds (rcMonitor, not rcWork — fullscreen
        // apps hide the taskbar so we want the full monitor area).
        // A few pixels of tolerance to absorb DPI rounding.
        let mon_rc = info.rcMonitor;
        let win_w = win_rc.right - win_rc.left;
        let win_h = win_rc.bottom - win_rc.top;
        let mon_w = mon_rc.right - mon_rc.left;
        let mon_h = mon_rc.bottom - mon_rc.top;
        (win_w - mon_w).abs() <= 4 && (win_h - mon_h).abs() <= 4
    }
}
