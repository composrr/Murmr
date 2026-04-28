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
use windows_sys::Win32::Foundation::{POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowRect, GetWindowThreadProcessId, GUITHREADINFO,
};

use super::ScreenRect;

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
