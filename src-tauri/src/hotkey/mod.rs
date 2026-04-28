//! Global keyboard hotkey listener.
//!
//! Uses `rdev::grab` (low-level platform keyboard hook with suppression).
//! Bound keys (dictation, repeat, cancel) are CONSUMED — they don't reach
//! the focused app — so binding to e.g. `~` or `K` doesn't paint the
//! character into whatever input has focus on every press.
//!
//! Why not `rdev::listen`? It's observation-only — events still reach the
//! focused app. Why not `tauri-plugin-global-shortcut`? It doesn't fire
//! on bare modifier keys (Right Ctrl, etc.), which is the canonical Murmr
//! ergonomic.
//!
//! ## Runtime configuration
//!
//! Bindings live in a global `Arc<RwLock<HotkeyConfig>>` populated at
//! startup from `Settings`. The Settings page calls `update_config(...)`
//! to swap them live without restarting the listener thread (rdev's grab
//! blocks forever and can't be woken).

use std::sync::Arc;

use crossbeam_channel::Sender;
use parking_lot::RwLock;
use rdev::{grab, Event, EventType, Key};

#[derive(Debug, Clone, Copy)]
pub struct HotkeyConfig {
    pub dictation: Key,
    /// Standalone re-paste key. `None` disables the re-paste shortcut
    /// entirely (the user can still re-paste from the History page).
    pub repeat: Option<Key>,
    pub cancel: Key,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            dictation: Key::ControlRight,
            repeat: None,
            cancel: Key::Escape,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    DictationDown,
    DictationUp,
    EscDown,
    /// User pressed the standalone re-paste key. The controller re-injects
    /// the most recent transcription.
    RepeatLast,
}

// --- Global config + helpers -----------------------------------------------

static CONFIG: parking_lot::Mutex<Option<Arc<RwLock<HotkeyConfig>>>> =
    parking_lot::Mutex::new(None);

fn config_handle() -> Arc<RwLock<HotkeyConfig>> {
    let mut guard = CONFIG.lock();
    if let Some(c) = guard.as_ref() {
        return Arc::clone(c);
    }
    let cfg = Arc::new(RwLock::new(HotkeyConfig::default()));
    *guard = Some(Arc::clone(&cfg));
    cfg
}

/// Hot-swap the listener's bound keys. Called from the IPC layer after the
/// user saves a new key in Settings.
pub fn update_config(new_config: HotkeyConfig) {
    let handle = config_handle();
    *handle.write() = new_config;
}

/// Parse a key name from settings (matches `format!("{:?}", Key::*)`) into
/// an rdev::Key. Returns `None` on unknown / empty names so callers can
/// keep the previous binding (or treat as "disabled").
pub fn parse_key(name: &str) -> Option<Key> {
    match name {
        // Modifiers
        "Alt" => Some(Key::Alt),
        "AltGr" => Some(Key::AltGr),
        "ControlLeft" => Some(Key::ControlLeft),
        "ControlRight" => Some(Key::ControlRight),
        "ShiftLeft" => Some(Key::ShiftLeft),
        "ShiftRight" => Some(Key::ShiftRight),
        "MetaLeft" => Some(Key::MetaLeft),
        "MetaRight" => Some(Key::MetaRight),
        "CapsLock" => Some(Key::CapsLock),
        // Navigation + control
        "Backspace" => Some(Key::Backspace),
        "Delete" => Some(Key::Delete),
        "DownArrow" => Some(Key::DownArrow),
        "End" => Some(Key::End),
        "Escape" => Some(Key::Escape),
        "Home" => Some(Key::Home),
        "Insert" => Some(Key::Insert),
        "LeftArrow" => Some(Key::LeftArrow),
        "PageDown" => Some(Key::PageDown),
        "PageUp" => Some(Key::PageUp),
        "Return" => Some(Key::Return),
        "RightArrow" => Some(Key::RightArrow),
        "Space" => Some(Key::Space),
        "Tab" => Some(Key::Tab),
        "UpArrow" => Some(Key::UpArrow),
        "PrintScreen" => Some(Key::PrintScreen),
        "ScrollLock" => Some(Key::ScrollLock),
        "Pause" => Some(Key::Pause),
        "NumLock" => Some(Key::NumLock),
        "Function" => Some(Key::Function),
        // F-keys
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        // Top-row numbers (rdev names them Num0..Num9, NOT Digit0..)
        "Num0" => Some(Key::Num0),
        "Num1" => Some(Key::Num1),
        "Num2" => Some(Key::Num2),
        "Num3" => Some(Key::Num3),
        "Num4" => Some(Key::Num4),
        "Num5" => Some(Key::Num5),
        "Num6" => Some(Key::Num6),
        "Num7" => Some(Key::Num7),
        "Num8" => Some(Key::Num8),
        "Num9" => Some(Key::Num9),
        // Letters
        "KeyA" => Some(Key::KeyA),
        "KeyB" => Some(Key::KeyB),
        "KeyC" => Some(Key::KeyC),
        "KeyD" => Some(Key::KeyD),
        "KeyE" => Some(Key::KeyE),
        "KeyF" => Some(Key::KeyF),
        "KeyG" => Some(Key::KeyG),
        "KeyH" => Some(Key::KeyH),
        "KeyI" => Some(Key::KeyI),
        "KeyJ" => Some(Key::KeyJ),
        "KeyK" => Some(Key::KeyK),
        "KeyL" => Some(Key::KeyL),
        "KeyM" => Some(Key::KeyM),
        "KeyN" => Some(Key::KeyN),
        "KeyO" => Some(Key::KeyO),
        "KeyP" => Some(Key::KeyP),
        "KeyQ" => Some(Key::KeyQ),
        "KeyR" => Some(Key::KeyR),
        "KeyS" => Some(Key::KeyS),
        "KeyT" => Some(Key::KeyT),
        "KeyU" => Some(Key::KeyU),
        "KeyV" => Some(Key::KeyV),
        "KeyW" => Some(Key::KeyW),
        "KeyX" => Some(Key::KeyX),
        "KeyY" => Some(Key::KeyY),
        "KeyZ" => Some(Key::KeyZ),
        // Symbols on the main row
        "BackQuote" => Some(Key::BackQuote),
        "Minus" => Some(Key::Minus),
        "Equal" => Some(Key::Equal),
        "LeftBracket" => Some(Key::LeftBracket),
        "RightBracket" => Some(Key::RightBracket),
        "BackSlash" => Some(Key::BackSlash),
        "SemiColon" => Some(Key::SemiColon),
        "Quote" => Some(Key::Quote),
        "Comma" => Some(Key::Comma),
        "Dot" => Some(Key::Dot),
        "Slash" => Some(Key::Slash),
        "IntlBackslash" => Some(Key::IntlBackslash),
        // Numpad
        "Kp0" => Some(Key::Kp0),
        "Kp1" => Some(Key::Kp1),
        "Kp2" => Some(Key::Kp2),
        "Kp3" => Some(Key::Kp3),
        "Kp4" => Some(Key::Kp4),
        "Kp5" => Some(Key::Kp5),
        "Kp6" => Some(Key::Kp6),
        "Kp7" => Some(Key::Kp7),
        "Kp8" => Some(Key::Kp8),
        "Kp9" => Some(Key::Kp9),
        "KpDelete" => Some(Key::KpDelete),
        "KpReturn" => Some(Key::KpReturn),
        "KpMinus" => Some(Key::KpMinus),
        "KpPlus" => Some(Key::KpPlus),
        "KpMultiply" => Some(Key::KpMultiply),
        "KpDivide" => Some(Key::KpDivide),
        _ => None,
    }
}

/// Build a `HotkeyConfig` from raw setting strings. Empty `repeat` =
/// disabled. Falls back to defaults for fields that don't parse.
pub fn config_from_strings(dictation: &str, repeat: &str, cancel: &str) -> HotkeyConfig {
    let defaults = HotkeyConfig::default();
    HotkeyConfig {
        dictation: parse_key(dictation).unwrap_or(defaults.dictation),
        repeat: parse_key(repeat),
        cancel: parse_key(cancel).unwrap_or(defaults.cancel),
    }
}

// --- Listener thread -------------------------------------------------------

/// Spawn a dedicated OS-thread that runs `rdev::grab` and forwards hotkey
/// events to the supplied sender. `rdev::grab` blocks forever, so it must
/// own its own thread.
///
/// Cost: `grab` runs in the OS keyboard hook hot path (every key the user
/// types). Our callback is ~microseconds (one RwLock read + a channel
/// send), so latency added to normal typing is unmeasurable.
pub fn spawn(tx: Sender<HotkeyEvent>, initial_config: HotkeyConfig) {
    *config_handle().write() = initial_config;

    let cfg_for_cb = config_handle();

    std::thread::Builder::new()
        .name("murmr-hotkey".into())
        .spawn(move || {
            let result = grab(move |event: Event| -> Option<Event> {
                let cfg = *cfg_for_cb.read();

                // Decide if this event is one of ours, AND whether to
                // suppress it (so it doesn't ALSO reach the focused app).
                let (msg, suppress) = match event.event_type {
                    EventType::KeyPress(k) if k == cfg.dictation => {
                        (Some(HotkeyEvent::DictationDown), true)
                    }
                    EventType::KeyRelease(k) if k == cfg.dictation => {
                        // Suppress the release too so e.g. a held `~`
                        // doesn't end with the OS thinking it should
                        // commit the character.
                        (Some(HotkeyEvent::DictationUp), true)
                    }
                    EventType::KeyPress(k) if Some(k) == cfg.repeat => {
                        (Some(HotkeyEvent::RepeatLast), true)
                    }
                    EventType::KeyRelease(k) if Some(k) == cfg.repeat => {
                        // Suppress the release for symmetry.
                        (None, true)
                    }
                    EventType::KeyPress(k) if k == cfg.cancel => {
                        (Some(HotkeyEvent::EscDown), true)
                    }
                    EventType::KeyRelease(k) if k == cfg.cancel => {
                        (None, true)
                    }
                    _ => (None, false),
                };

                if let Some(ev) = msg {
                    let _ = tx.send(ev);
                }
                if suppress {
                    None
                } else {
                    Some(event)
                }
            });
            if let Err(e) = result {
                eprintln!("[hotkey] rdev grab error: {e:?}");
            }
        })
        .expect("failed to spawn hotkey thread");
}
