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

/// How long to wait after synthesizing the paste before restoring the user's
/// previous clipboard. Must comfortably exceed how long a busy app takes to
/// actually READ the clipboard after receiving Ctrl+V — otherwise it reads the
/// restored value and pastes the wrong text. See `inject_clipboard`.
const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(1200);

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

    // Restore the user's previous clipboard on a BACKGROUND thread after a
    // generous delay.
    //
    // The paste we just synthesized is processed ASYNCHRONOUSLY by the target
    // app — Ctrl+V only queues the keystroke; the app reads the clipboard
    // whenever it gets around to it. This used to restore after 50ms, which
    // lost the race constantly: a busy app (and everything is busy right after
    // a transcribe just pegged every CPU core) would read the clipboard AFTER
    // we'd already put the old contents back, and paste the PREVIOUS clipboard
    // — often text from a completely different app. Waiting ~1.2s makes that
    // essentially impossible, and doing it off-thread keeps injection snappy.
    //
    // The still-ours check stays: if anything else wrote to the clipboard in
    // the meantime, we leave it alone rather than clobber it.
    if let Some(prior_text) = prior {
        let ours = text.to_string();
        std::thread::Builder::new()
            .name("murmr-clipboard-restore".into())
            .spawn(move || {
                thread::sleep(CLIPBOARD_RESTORE_DELAY);
                let Ok(mut cb) = Clipboard::new() else { return };
                if cb.get_text().ok().as_deref() == Some(ours.as_str()) {
                    let _ = cb.set_text(prior_text);
                }
            })
            .ok();
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
