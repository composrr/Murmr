//! Detect what's focused on the user's desktop so we can position the HUD
//! intelligently. Win32 today; macOS (AXUIElement) lands when we get to a
//! Mac build.

#[cfg(windows)]
mod windows;

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

#[cfg(not(windows))]
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
