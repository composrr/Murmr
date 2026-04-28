//! Text injection via clipboard-paste.
//!
//! Phase 3 implements only the clipboard path: snapshot the current clipboard,
//! set ours, simulate the platform paste shortcut, then restore. Per-keystroke
//! fallback for paste-blocking apps lands in Phase 9 (Advanced settings).
//!
//! Race protection: per plan §13 #7, we wait briefly after pasting and only
//! restore if the clipboard still holds *our* text — otherwise some other app
//! wrote between our set and our restore, and clobbering it would lose data.

use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use enigo::{
    Direction::{Press, Release},
    Enigo, Key, Keyboard, Settings,
};

#[cfg(target_os = "macos")]
const MOD_KEY: Key = Key::Meta;
#[cfg(not(target_os = "macos"))]
const MOD_KEY: Key = Key::Control;

pub fn inject_text(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

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
    let v_result = enigo.key(Key::Unicode('v'), enigo::Direction::Click);
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
