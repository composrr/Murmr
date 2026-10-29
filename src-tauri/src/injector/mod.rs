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

use arboard::{Clipboard, ImageData};
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

/// A snapshot of the user's clipboard taken before we overwrite it, so we can
/// put it back afterward. Covers text AND images — the previous version only
/// snapshotted text, so dictating while a photo (or other image) was on the
/// clipboard permanently replaced it with the transcript.
enum PriorClipboard {
    Text(String),
    Image(ImageData<'static>),
}

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

    // Snapshot whatever is currently on the clipboard so we can restore it.
    // Text first; if there's no text, check for an image (a copied photo /
    // screenshot). Without the image case, dictating while a photo is on the
    // clipboard would wipe it out and leave the transcript there for good.
    let prior: Option<PriorClipboard> = match clipboard.get_text().ok() {
        Some(t) => Some(PriorClipboard::Text(t)),
        None => clipboard.get_image().ok().map(PriorClipboard::Image),
    };

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
    if let Some(prior) = prior {
        let ours = text.to_string();
        std::thread::Builder::new()
            .name("murmr-clipboard-restore".into())
            .spawn(move || {
                thread::sleep(CLIPBOARD_RESTORE_DELAY);
                let Ok(mut cb) = Clipboard::new() else { return };
                // Only restore if the clipboard is still OUR transcript — i.e.
                // nothing else was copied in the meantime (in which case we
                // leave the user's newer content alone).
                if cb.get_text().ok().as_deref() == Some(ours.as_str()) {
                    match prior {
                        PriorClipboard::Text(t) => {
                            let _ = cb.set_text(t);
                        }
                        PriorClipboard::Image(img) => {
                            let _ = cb.set_image(img);
                        }
                    }
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

// ---- Command mode: read the current selection ----

/// How long to wait for the focused app to answer a synthesized copy.
/// Electron/browser apps can take a few hundred ms right after a transcribe.
const COPY_POLL_TIMEOUT: Duration = Duration::from_millis(600);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Minimum gap between [`copy_selection`] returning and the next paste.
/// Measured on Windows: when nothing was selected, a paste issued <0.5 s
/// after the probe's clipboard writes was silently dropped by the target
/// app (a WinForms TextBox), while 1.5 s always worked — something
/// (likely a clipboard listener such as Windows clipboard history) still
/// holds the clipboard. The LLM call usually takes longer than this anyway.
pub const SELECTION_PROBE_SETTLE: Duration = Duration::from_millis(1200);

/// Copy the focused app's current selection and return it as text, leaving
/// the user's clipboard exactly as it was.
///
/// - `Ok(Some(text))` — something was selected.
/// - `Ok(None)` — nothing selected (the app didn't change the clipboard).
/// - `Err(..)` — the selection isn't text (e.g. an image), or the clipboard
///   or keyboard could not be driven.
///
/// How: snapshot the clipboard (text or image), put a unique sentinel on it,
/// synthesize Ctrl/Cmd+C, and poll until the sentinel is replaced. The
/// prior clipboard is restored right away: unlike a paste, nothing reads our
/// value later, so there is no restore race to wait out.
pub fn copy_selection() -> Result<Option<String>, String> {
    let mut clipboard = Clipboard::new().map_err(|e| format!("clipboard open: {e}"))?;
    let prior: Option<PriorClipboard> = match clipboard.get_text().ok() {
        Some(t) => Some(PriorClipboard::Text(t)),
        None => clipboard.get_image().ok().map(PriorClipboard::Image),
    };

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let sentinel = format!("\u{2063}murmr-selection-probe-{nanos}");
    clipboard
        .set_text(sentinel.clone())
        .map_err(|e| format!("clipboard set: {e}"))?;
    thread::sleep(Duration::from_millis(20));

    let copy_result = synthesize_copy();

    // Poll for the app's answer (only if the keystroke went out).
    let mut outcome: Result<Option<String>, String> = Ok(None);
    if let Err(e) = copy_result {
        outcome = Err(e);
    } else {
        let started = std::time::Instant::now();
        while started.elapsed() < COPY_POLL_TIMEOUT {
            thread::sleep(COPY_POLL_INTERVAL);
            match clipboard.get_text() {
                Ok(t) if t == sentinel => continue,
                Ok(t) => {
                    outcome = Ok(if t.trim().is_empty() { None } else { Some(t) });
                    break;
                }
                // Clipboard changed to something that isn't text.
                Err(_) => {
                    outcome = Err("the selection isn't text".into());
                    break;
                }
            }
        }
    }

    // Put the user's clipboard back. Only if what's there is still the
    // sentinel or the copied selection — never clobber something newer.
    let current = clipboard.get_text().ok();
    let ours = current.as_deref() == Some(sentinel.as_str())
        || matches!((&outcome, current.as_deref()), (Ok(Some(sel)), Some(cur)) if sel == cur)
        || matches!(outcome, Err(_));
    if ours {
        match prior {
            Some(PriorClipboard::Text(t)) => {
                let _ = clipboard.set_text(t);
            }
            Some(PriorClipboard::Image(img)) => {
                let _ = clipboard.set_image(img);
            }
            None => {
                let _ = clipboard.clear();
            }
        }
    }
    outcome
}

fn synthesize_copy() -> Result<(), String> {
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| format!("enigo init: {e}"))?;
    enigo
        .key(MOD_KEY, Press)
        .map_err(|e| format!("enigo Ctrl down: {e}"))?;
    // Raw keycode on macOS for the same reason as the paste path (Key::Unicode
    // crashes on macOS 26). C on the macOS HID keymap is 8.
    #[cfg(target_os = "macos")]
    let c_key = Key::Other(8);
    #[cfg(not(target_os = "macos"))]
    let c_key = Key::Unicode('c');
    let c_result = enigo.key(c_key, Click);
    let _ = enigo.key(MOD_KEY, Release);
    c_result.map_err(|e| format!("enigo C: {e}"))
}

#[cfg(all(test, windows))]
mod command_tests {
    use super::*;

    /// Spawn a self-closing WinForms window with a focused TextBox. On close
    /// it writes the TextBox contents to `out` so the test can check them.
    fn spawn_textbox(text: &str, select_all: bool, out: &std::path::Path) -> std::process::Child {
        let sel = if select_all {
            "$t.SelectAll()"
        } else {
            "$t.SelectionStart = $t.TextLength; $t.SelectionLength = 0"
        };
        let out = out.display();
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $f = New-Object Windows.Forms.Form; $f.TopMost = $true; $f.Text = 'murmr-test'; \
             $t = New-Object Windows.Forms.TextBox; $t.Text = '{text}'; $t.Width = 300; $f.Controls.Add($t); \
             $f.Add_Shown({{ $f.Activate(); $t.Focus(); {sel} }}); \
             $f.Add_FormClosing({{ [IO.File]::WriteAllText('{out}', $t.Text) }}); \
             $timer = New-Object Windows.Forms.Timer; $timer.Interval = 7500; $timer.Add_Tick({{ $f.Close() }}); $timer.Start(); \
             [void]$f.ShowDialog()"
        );
        std::process::Command::new("powershell")
            .args(["-NoProfile", "-STA", "-Command", &script])
            .spawn()
            .expect("spawn powershell")
    }

    /// Full command-mode text path minus Whisper/LLM: read the selection,
    /// paste a replacement (or insert at the cursor), clipboard restored.
    /// `cargo test --lib injector -- --ignored --nocapture --test-threads=1`
    #[test]
    #[ignore]
    fn live_copy_then_replace_in_textbox() {
        let dir = std::env::temp_dir();
        let cases = [
            (true, Some("hello murmr selection"), "REPLACED"),
            (false, None, "hello murmr selection INSERTED"),
        ];
        for (i, (select_all, expect_sel, expect_final)) in cases.into_iter().enumerate() {
            let out = dir.join(format!("murmr-cmd-test-{i}.txt"));
            let _ = std::fs::remove_file(&out);
            Clipboard::new().unwrap().set_text("prior clipboard value").unwrap();
            let mut child = spawn_textbox("hello murmr selection", select_all, &out);
            thread::sleep(Duration::from_millis(3500));
            let fg = crate::focus::capture_foreground().app_name;
            println!("foreground: {fg:?}");
            if fg.as_deref() != Some("powershell.exe") {
                // Never send keys to whatever the user has in front.
                let _ = child.kill();
                println!("test window not in front; skipping");
                return;
            }

            let got = copy_selection();
            println!("select_all={select_all} -> {got:?}");
            assert_eq!(got, Ok(expect_sel.map(str::to_string)));
            assert_eq!(Clipboard::new().unwrap().get_text().unwrap(), "prior clipboard value");

            if !select_all { thread::sleep(Duration::from_millis(400)); println!("cb before inject: {:?}", Clipboard::new().unwrap().get_text()); }
            thread::sleep(SELECTION_PROBE_SETTLE);
            if crate::focus::capture_foreground().app_name.as_deref() != Some("powershell.exe") {
                let _ = child.kill();
                println!("test window lost focus before paste; skipping");
                return;
            }
            let insert = if select_all { "REPLACED" } else { " INSERTED" };
            inject_text(insert, false).unwrap();
            thread::sleep(Duration::from_millis(1600)); // past the restore delay
            assert_eq!(Clipboard::new().unwrap().get_text().unwrap(), "prior clipboard value");

            let _ = child.wait();
            let final_text = std::fs::read_to_string(&out).unwrap_or_default();
            println!("textbox after: {final_text:?}");
            assert_eq!(final_text, expect_final);
        }
    }
}
