//! Detect what's focused on the user's desktop so we can position the HUD
//! intelligently. Win32 UIA on Windows; AXUIElement on macOS.

#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone, Copy)]
pub struct ScreenRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Bounding rect of the currently focused accessible element via UI
/// Automation. Works for Chrome / Electron / VS Code / WinUI / modern
/// Office — basically anything with a11y exposed (~all modern apps).
#[cfg(windows)]
pub fn uia_focused_element_rect() -> Option<ScreenRect> {
    windows::uia_focused_element_rect()
}

#[cfg(target_os = "macos")]
pub fn uia_focused_element_rect() -> Option<ScreenRect> {
    macos::ax_focused_element_rect()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn uia_focused_element_rect() -> Option<ScreenRect> {
    None
}

/// Legacy Win32 caret rect for old EDIT/RICHEDIT controls.
#[cfg(windows)]
pub fn focused_caret_screen_rect() -> Option<ScreenRect> {
    windows::focused_caret_screen_rect()
}

#[cfg(not(windows))]
pub fn focused_caret_screen_rect() -> Option<ScreenRect> {
    None
}

/// Bounding rect of the foreground window — final fallback if neither UIA
/// nor the legacy caret give us anything.
#[cfg(windows)]
pub fn focused_window_screen_rect() -> Option<ScreenRect> {
    windows::focused_window_screen_rect()
}

#[cfg(not(windows))]
pub fn focused_window_screen_rect() -> Option<ScreenRect> {
    None
}

/// True when the currently-focused window covers an entire monitor's
/// bounds — i.e. it's a fullscreen game, video player, or
/// presentation. Used by the hotkey thread to optionally pause
/// dictation while a fullscreen app is focused (so we don't fight
/// the OS for the key release, accidentally trigger in-game, or
/// look like macroing to anti-cheat software).
///
/// Cheap to call — one GetForegroundWindow + one MonitorFromWindow +
/// one GetMonitorInfo on Windows. Returns false on platforms where
/// we don't have a native implementation.
#[cfg(windows)]
pub fn is_foreground_fullscreen() -> bool {
    windows::is_foreground_fullscreen()
}

#[cfg(not(windows))]
pub fn is_foreground_fullscreen() -> bool {
    false
}

// ---------------------------------------------------------------------------
// Foreground app / window identity (for wrong-window paste protection + the
// Insights "where you dictate" breakdown).
// ---------------------------------------------------------------------------

/// Executable base name of the foreground app (e.g. "Code.exe"). None on
/// platforms without a native implementation.
#[cfg(windows)]
pub fn foreground_app() -> Option<String> {
    windows::foreground_app()
}

#[cfg(not(windows))]
pub fn foreground_app() -> Option<String> {
    None
}

/// Opaque Send-able id of the foreground window, compared later to detect
/// focus changes during transcription. None where unsupported.
#[cfg(windows)]
pub fn foreground_window_id() -> Option<isize> {
    windows::foreground_window_id()
}

#[cfg(not(windows))]
pub fn foreground_window_id() -> Option<isize> {
    None
}

/// Best-effort restore of foreground focus to a previously-captured window.
#[cfg(windows)]
pub fn refocus_window(id: isize) -> bool {
    windows::refocus_window(id)
}

#[cfg(not(windows))]
pub fn refocus_window(_id: isize) -> bool {
    false
}

// ---------------------------------------------------------------------------
// Edit-last target (where edited text is re-injected)
// ---------------------------------------------------------------------------

/// Opaque handle to the place edited text should go back into: the
/// foreground HWND on Windows, the frontmost app's PID on macOS.
#[cfg(windows)]
pub fn capture_edit_target() -> Option<isize> {
    windows::foreground_window_id()
}

#[cfg(target_os = "macos")]
pub fn capture_edit_target() -> Option<isize> {
    macos::frontmost_app_pid()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn capture_edit_target() -> Option<isize> {
    None
}

/// True if the captured edit target still exists (window open / app running).
#[cfg(windows)]
pub fn edit_target_alive(id: isize) -> bool {
    windows::window_exists(id)
}

#[cfg(target_os = "macos")]
pub fn edit_target_alive(id: isize) -> bool {
    macos::app_running(id)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn edit_target_alive(_id: isize) -> bool {
    true
}

/// Give focus back to the captured edit target and confirm it took.
#[cfg(windows)]
pub fn restore_edit_target(id: isize, timeout: std::time::Duration) -> bool {
    windows::refocus_window_verified(id, timeout)
}

#[cfg(target_os = "macos")]
pub fn restore_edit_target(id: isize, timeout: std::time::Duration) -> bool {
    macos::activate_app_verified(id, timeout)
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn restore_edit_target(_id: isize, _timeout: std::time::Duration) -> bool {
    false
}

/// A capture of the window the user was focused on when dictation started.
#[derive(Debug, Clone, Default)]
pub struct CaptureTarget {
    pub window_id: Option<isize>,
    pub app_name: Option<String>,
}

/// Snapshot the current foreground target (window id + app name).
pub fn capture_foreground() -> CaptureTarget {
    CaptureTarget {
        window_id: foreground_window_id(),
        app_name: foreground_app(),
    }
}
