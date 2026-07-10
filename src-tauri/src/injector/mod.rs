//! Text injection with two paths, selectable via `Settings.injection_mode`.
//!
//! - **Clipboard** (default): snapshot the current clipboard, set ours,
//!   simulate the platform paste shortcut, then restore. Fast, preserves
//!   formatting, and works in almost every app.
//! - **Per-keystroke**: synthesize the text as Unicode keystrokes
//!   (SendInput on Windows, CGEvent on macOS via `enigo`). Slower, but the
//!   reliable fallback for apps that block programmatic Ctrl/Cmd+V
//!   (Photoshop, Ableton, some secure fields) where the clipboard path
//!   silently no-ops.
//!
//! Both paths return `Result<(), String>`; the controller promotes any
//! `Err` to a visible `Status::Error` so a failed injection is never silent.
//!
//! Race protection (clipboard path): per plan §13 #7, we wait briefly after
//! pasting and only restore if the clipboard still holds *our* text —
//! otherwise some other app wrote between our set and our restore, and
//! clobbering it would lose data.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

#[cfg(target_os = "macos")]
const MOD_KEY: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const MOD_KEY: Key = Key::Control;

/// Inject `text` into the focused app. When `keystroke_mode` is true, type it
/// character-by-character instead of pasting — the fallback for apps that
/// ignore a synthesized paste.
pub fn inject_text(text: &str, keystroke_mode: bool) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    if keystroke_mode {
        inject_keystroke(text)
    } else {
        inject_clipboard(text)
    }
}

fn inject_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("clipboard open: {e}"))?;
    let prior = clipboard.get_text().ok();

    clipboard
        .set_text(text.to_string())
        .map_err(|e| format!("clipboard set: {e}"))?;

    // Tiny pause so the OS picks up the new clipboard contents before paste.
    thread::sleep(Duration::from_millis(20));

    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("enigo init: {e}"))?;
    enigo
        .key(MOD_KEY, Press)
        .map_err(|e| format!("enigo Ctrl down: {e}"))?;
    // macOS 26 crashes on Key::Unicode: enigo's char→keycode translation calls
    // TISGetInputSourceProperty off the main thread; dispatch_assert_queue
    // fatally aborts. Key::Other passes raw keycode to CGEventCreateKeyboardEvent
    // with no TSM lookup. V on macOS HID keymap is 9.
    #[cfg(target_os = "macos")]
    let v_key = Key::Other(9);
    #[cfg(not(target_os = "macos"))]
    let v_key = Key::Unicode('v');
    let v_result = enigo.key(v_key, Click);
    let _ = enigo.key(MOD_KEY, Release);
    v_result.map_err(|e| format!("enigo V: {e}"))?;

    // Wait for the paste to consume the clipboard, then restore — but only
    // if no one else has overwritten our text in the meantime.
    thread::sleep(Duration::from_millis(50));
    if let Some(prior_text) = prior {
        let still_ours = clipboard.get_text().ok().as_deref() == Some(text);
        if still_ours {
            let _ = clipboard.set_text(prior_text);
        }
    }

    Ok(())
}

/// Type `text` as synthesized keystrokes. Newlines become Return presses so
/// voice "new line"/"new paragraph" still work in this mode. Leaves the
/// clipboard untouched.
fn inject_keystroke(text: &str) -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("enigo init: {e}"))?;
    let mut first = true;
    for segment in text.split('\n') {
        if !first {
            enigo
                .key(Key::Return, Click)
                .map_err(|e| format!("enigo Return: {e}"))?;
        }
        first = false;
        if !segment.is_empty() {
            enigo
                .text(segment)
                .map_err(|e| format!("enigo type: {e}"))?;
        }
    }
    Ok(())
}
