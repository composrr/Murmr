//! Global keyboard hotkey listener with chord support.
//!
//! Each binding is a *chord*: zero or more modifiers (Ctrl/Shift/Alt/Meta)
//! plus exactly one main key. So you can bind dictation to a bare key
//! (`F8`, `CapsLock`, `~`), a modifier-letter combo (`Ctrl+Shift+V`), or
//! anything in between.
//!
//! Match policy is **strict** — `Shift+V` does NOT fire when the user
//! presses `Ctrl+Shift+V`, because the extra Ctrl modifier disqualifies it.
//! Lenient matching invites accidental triggers from ordinary app shortcuts.
//!
//! We use `rdev::grab` so the main key (and only the main key) is consumed —
//! modifiers always pass through normally so things like Ctrl-selection in
//! the focused app keep working.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;
use parking_lot::RwLock;
use rdev::{grab, Event, EventType, Key};

// ---------------------------------------------------------------------------
// Chord type — modifiers + main key
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifierSet {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl ModifierSet {
    fn empty(&self) -> bool {
        !(self.ctrl || self.shift || self.alt || self.meta)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Chord {
    pub modifiers: ModifierSet,
    pub key: Key,
}

#[derive(Debug, Clone, Copy)]
pub struct HotkeyConfig {
    pub dictation: Chord,
    /// `None` disables the re-paste shortcut entirely.
    pub repeat: Option<Chord>,
    pub cancel: Chord,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            dictation: Chord {
                modifiers: ModifierSet::default(),
                key: Key::ControlRight,
            },
            repeat: None,
            cancel: Chord {
                modifiers: ModifierSet::default(),
                key: Key::Escape,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    DictationDown,
    DictationUp,
    EscDown,
    RepeatLast,
}

// ---------------------------------------------------------------------------
// String <-> Chord serialization
// ---------------------------------------------------------------------------

/// Parse a chord like "Ctrl+Shift+KeyV", "F8", or "ControlRight".
/// Modifiers (in any order): "Ctrl", "Shift", "Alt", "Meta".
/// Returns None if the string has no valid main key (modifier-only chords
/// aren't allowed — except ControlRight etc which ARE main keys despite
/// being a modifier physically).
pub fn parse_chord(s: &str) -> Option<Chord> {
    let mut modifiers = ModifierSet::default();
    let mut main_key: Option<Key> = None;

    for raw in s.split('+') {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        match part {
            "Ctrl" => modifiers.ctrl = true,
            "Shift" => modifiers.shift = true,
            "Alt" => modifiers.alt = true,
            "Meta" => modifiers.meta = true,
            other => {
                if main_key.is_some() {
                    return None; // two non-modifier keys — invalid
                }
                main_key = parse_key(other);
            }
        }
    }

    main_key.map(|k| Chord {
        modifiers,
        key: k,
    })
}

/// Build a `HotkeyConfig` from raw setting strings, falling back to the
/// defaults for any field that doesn't parse. Empty `repeat` = disabled.
pub fn config_from_strings(dictation: &str, repeat: &str, cancel: &str) -> HotkeyConfig {
    let defaults = HotkeyConfig::default();
    HotkeyConfig {
        dictation: parse_chord(dictation).unwrap_or(defaults.dictation),
        repeat: parse_chord(repeat),
        cancel: parse_chord(cancel).unwrap_or(defaults.cancel),
    }
}

/// Parse a key name from rdev's Debug spelling.
fn parse_key(name: &str) -> Option<Key> {
    match name {
        // Modifiers (also valid as main keys for bare-modifier hotkeys)
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
        // Digits
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
        // Symbols
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

// ---------------------------------------------------------------------------
// Global config + helpers
// ---------------------------------------------------------------------------

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

pub fn update_config(new_config: HotkeyConfig) {
    let handle = config_handle();
    *handle.write() = new_config;
}

// ---------------------------------------------------------------------------
// Listener
// ---------------------------------------------------------------------------

#[derive(Default, Clone)]
struct ModifierState {
    shift: Arc<AtomicBool>,
    ctrl: Arc<AtomicBool>,
    alt: Arc<AtomicBool>,
    meta: Arc<AtomicBool>,
}

impl ModifierState {
    fn snapshot(&self) -> ModifierSet {
        ModifierSet {
            ctrl: self.ctrl.load(Ordering::Relaxed),
            shift: self.shift.load(Ordering::Relaxed),
            alt: self.alt.load(Ordering::Relaxed),
            meta: self.meta.load(Ordering::Relaxed),
        }
    }

    fn apply(&self, ev: &EventType) {
        let (key, pressed) = match ev {
            EventType::KeyPress(k) => (*k, true),
            EventType::KeyRelease(k) => (*k, false),
            _ => return,
        };
        match key {
            Key::ShiftLeft | Key::ShiftRight => self.shift.store(pressed, Ordering::Relaxed),
            Key::ControlLeft | Key::ControlRight => self.ctrl.store(pressed, Ordering::Relaxed),
            Key::Alt | Key::AltGr => self.alt.store(pressed, Ordering::Relaxed),
            Key::MetaLeft | Key::MetaRight => self.meta.store(pressed, Ordering::Relaxed),
            _ => {}
        }
    }
}

/// Spawn the OS keyboard hook thread. Bindings come from `initial_config`
/// and can be hot-swapped via `update_config(...)` without restarting.
///
/// Suppression rule: only the *main key* of a matched chord is consumed.
/// Modifiers always pass through normally — otherwise binding to
/// `Ctrl+Shift+V` would break every app's normal Ctrl handling.
pub fn spawn(tx: Sender<HotkeyEvent>, initial_config: HotkeyConfig) {
    *config_handle().write() = initial_config;

    let modifiers = ModifierState::default();
    let mods_for_cb = modifiers.clone();
    let cfg_for_cb = config_handle();

    std::thread::Builder::new()
        .name("murmr-hotkey".into())
        .spawn(move || {
            let result = grab(move |event: Event| -> Option<Event> {
                let cfg = *cfg_for_cb.read();

                // Snapshot modifier state PRIOR to applying this event so
                // chords with bare-modifier main keys (e.g. main = ControlRight,
                // expected modifiers = Ctrl) don't self-trigger when the user
                // presses ControlRight (which IS a Ctrl press but we want to
                // see "modifiers held BEFORE this press" for the match).
                let prior_mods = mods_for_cb.snapshot();
                mods_for_cb.apply(&event.event_type);

                // Helper: does this event press the chord's main key with
                // EXACTLY the right modifiers held?
                let matches_chord = |chord: &Chord, k: Key| -> bool {
                    k == chord.key && prior_mods == chord.modifiers
                };

                let (msg, suppress) = match event.event_type {
                    EventType::KeyPress(k) => {
                        if matches_chord(&cfg.dictation, k) {
                            (Some(HotkeyEvent::DictationDown), true)
                        } else if cfg
                            .repeat
                            .as_ref()
                            .map(|c| matches_chord(c, k))
                            .unwrap_or(false)
                        {
                            (Some(HotkeyEvent::RepeatLast), true)
                        } else if matches_chord(&cfg.cancel, k) {
                            (Some(HotkeyEvent::EscDown), true)
                        } else {
                            (None, false)
                        }
                    }
                    EventType::KeyRelease(k) => {
                        // Mirror the press-side suppression for the
                        // dictation key release (so push-to-talk's release
                        // closes the recording cleanly), and suppress the
                        // release of any other matched-on-press key for
                        // symmetry with the press.
                        if k == cfg.dictation.key {
                            (Some(HotkeyEvent::DictationUp), true)
                        } else if cfg.repeat.as_ref().map(|c| k == c.key).unwrap_or(false)
                            || k == cfg.cancel.key
                        {
                            (None, true)
                        } else {
                            (None, false)
                        }
                    }
                    _ => (None, false),
                };

                if let Some(ev) = msg {
                    let _ = tx.send(ev);
                }
                if suppress { None } else { Some(event) }
            });
            if let Err(e) = result {
                eprintln!("[hotkey] rdev grab error: {e:?}");
            }
        })
        .expect("failed to spawn hotkey thread");
}
