//! Global keyboard hotkey listener.
//!
//! Uses `rdev` (low-level platform keyboard hook) because the standard
//! tauri-plugin-global-shortcut family doesn't fire on bare modifier keys —
//! see plan §13 #5. We hook just the keys we care about and forward them to
//! the controller as `HotkeyEvent`s; the controller owns the tap-vs-hold
//! state machine and the "what to do next" decisions.
//!
//! ## Runtime configuration
//!
//! Bindings are stored in a global `Arc<RwLock<HotkeyConfig>>` populated at
//! startup from `Settings`. The Settings page calls
//! `update_config(...)` to swap them live without restarting the listener
//! thread (rdev's `listen()` blocks forever and can't be woken).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;
use parking_lot::RwLock;
use rdev::{grab, Event, EventType, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatModifier {
    Shift,
    Ctrl,
    Alt,
    Meta,
    None,
}

#[derive(Debug, Clone, Copy)]
pub struct HotkeyConfig {
    pub dictation: Key,
    pub repeat_mod: RepeatModifier,
    pub cancel: Key,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        // Windows: Right Ctrl avoids the menu-bar focus steal that Right Alt
        // causes; Esc to cancel; Shift+RightCtrl re-pastes.
        Self {
            dictation: Key::ControlRight,
            repeat_mod: RepeatModifier::Shift,
            cancel: Key::Escape,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    DictationDown,
    DictationUp,
    EscDown,
    /// Modifier was held when the dictation key was pressed — re-inject the
    /// most recent transcription instead of starting a new recording.
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
/// an rdev::Key. Returns `None` on unknown names so callers can keep the
/// previous binding instead of crashing.
///
/// Names follow rdev's `Debug` impl so it's a 1:1 round-trip with what
/// `format!("{:?}", key)` produces. Coverage:
///   - All modifier + nav + F-keys
///   - Letters (KeyA..KeyZ)
///   - Top-row numbers (Num0..Num9) + symbol keys
///   - Numpad (Kp0..Kp9, KpPlus, KpMinus, etc)
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
        // Letters (rdev names them KeyA..KeyZ)
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

pub fn parse_repeat_modifier(name: &str) -> RepeatModifier {
    match name {
        "Shift" => RepeatModifier::Shift,
        "Ctrl" => RepeatModifier::Ctrl,
        "Alt" => RepeatModifier::Alt,
        "Meta" => RepeatModifier::Meta,
        _ => RepeatModifier::None,
    }
}

/// Build a `HotkeyConfig` from raw setting strings, falling back to the
/// defaults for any field that doesn't parse.
pub fn config_from_strings(dictation: &str, repeat_mod: &str, cancel: &str) -> HotkeyConfig {
    let defaults = HotkeyConfig::default();
    HotkeyConfig {
        dictation: parse_key(dictation).unwrap_or(defaults.dictation),
        repeat_mod: parse_repeat_modifier(repeat_mod),
        cancel: parse_key(cancel).unwrap_or(defaults.cancel),
    }
}

// --- Listener thread -------------------------------------------------------

/// Spawn a dedicated OS-thread that runs `rdev::grab` and forwards hotkey
/// events to the supplied sender. `rdev::grab` blocks forever, so it must
/// own its own thread.
///
/// We use `grab` (not `listen`) specifically so the callback can return
/// `None` for the bound dictation/cancel keys — that consumes the keypress
/// before it reaches the focused app. Without this, binding to e.g. `~` or
/// `K` would type that character into whatever input has focus every time
/// the user starts dictation.
///
/// Cost: `grab` runs in the OS keyboard hook hot path (every key the user
/// types). Our callback is ~microseconds (a couple atomic loads + a
/// channel send), so latency added to normal typing is unmeasurable.
///
/// `initial_config` seeds the global config; subsequent updates flow through
/// `update_config(...)` and the listener picks them up on the next event
/// (RwLock read is sub-microsecond so this is fine in the hot path).
pub fn spawn(tx: Sender<HotkeyEvent>, initial_config: HotkeyConfig) {
    *config_handle().write() = initial_config;

    // Track every modifier key we might gate on. We watch all four families
    // (Shift / Ctrl / Alt / Meta) so the user can pick any of them as the
    // re-paste modifier without us having to spawn separate state machines.
    let modifier_state = ModifierState::default();
    let mods_for_cb = modifier_state.clone();
    let cfg_for_cb = config_handle();

    std::thread::Builder::new()
        .name("murmr-hotkey".into())
        .spawn(move || {
            let result = grab(move |event: Event| -> Option<Event> {
                let cfg = *cfg_for_cb.read();

                // Snapshot modifier state PRIOR to applying this event. Stops
                // the dictation key from self-triggering RepeatLast when the
                // user picks the same modifier family as both the dictation
                // key and the repeat modifier.
                let was_repeat_mod_held = mods_for_cb.matches(cfg.repeat_mod);

                // Now apply this event's effect on modifier state so the
                // *next* event sees the right snapshot.
                if let Some(slot) = modifier_slot(&event.event_type) {
                    let pressed = matches!(event.event_type, EventType::KeyPress(_));
                    mods_for_cb.set(slot, pressed);
                }

                // Decide if this event is one of ours, AND whether to
                // suppress it (so it doesn't ALSO reach the focused app).
                let (msg, suppress) = match event.event_type {
                    EventType::KeyPress(k) if k == cfg.dictation => {
                        let event = if was_repeat_mod_held {
                            HotkeyEvent::RepeatLast
                        } else {
                            HotkeyEvent::DictationDown
                        };
                        (Some(event), true)
                    }
                    EventType::KeyRelease(k) if k == cfg.dictation => {
                        // Suppress the release too so e.g. a held `~`
                        // doesn't end with the OS thinking it should
                        // commit the character.
                        (Some(HotkeyEvent::DictationUp), true)
                    }
                    EventType::KeyPress(k) if k == cfg.cancel => {
                        // Suppress cancel so e.g. binding to Escape during
                        // recording doesn't ALSO close the user's modal.
                        (Some(HotkeyEvent::EscDown), true)
                    }
                    EventType::KeyRelease(k) if k == cfg.cancel => {
                        // Suppress the release for symmetry with the press.
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

// --- Modifier tracking -----------------------------------------------------

#[derive(Default, Clone)]
struct ModifierState {
    shift: Arc<AtomicBool>,
    ctrl: Arc<AtomicBool>,
    alt: Arc<AtomicBool>,
    meta: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy)]
enum ModSlot {
    Shift,
    Ctrl,
    Alt,
    Meta,
}

impl ModifierState {
    fn set(&self, slot: ModSlot, pressed: bool) {
        let cell = match slot {
            ModSlot::Shift => &self.shift,
            ModSlot::Ctrl => &self.ctrl,
            ModSlot::Alt => &self.alt,
            ModSlot::Meta => &self.meta,
        };
        cell.store(pressed, Ordering::Relaxed);
    }

    fn matches(&self, modifier: RepeatModifier) -> bool {
        match modifier {
            RepeatModifier::Shift => self.shift.load(Ordering::Relaxed),
            RepeatModifier::Ctrl => self.ctrl.load(Ordering::Relaxed),
            RepeatModifier::Alt => self.alt.load(Ordering::Relaxed),
            RepeatModifier::Meta => self.meta.load(Ordering::Relaxed),
            RepeatModifier::None => false,
        }
    }
}

fn modifier_slot(event: &EventType) -> Option<ModSlot> {
    let key = match event {
        EventType::KeyPress(k) | EventType::KeyRelease(k) => *k,
        _ => return None,
    };
    match key {
        Key::ShiftLeft | Key::ShiftRight => Some(ModSlot::Shift),
        Key::ControlLeft | Key::ControlRight => Some(ModSlot::Ctrl),
        Key::Alt | Key::AltGr => Some(ModSlot::Alt),
        Key::MetaLeft | Key::MetaRight => Some(ModSlot::Meta),
        _ => None,
    }
}
