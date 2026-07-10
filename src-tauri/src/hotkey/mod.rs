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
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use parking_lot::{Mutex, RwLock};
use rdev::{grab, Event, EventType, Key};

/// How long we wait after a bare-modifier dictation key press before
/// committing to "this is a dictation hold" vs "this is a modifier in a
/// combo like Ctrl+V". Anything shorter and we can't reliably catch the
/// follow-up keystroke; anything longer and push-to-talk feels laggy at
/// the start of the recording.
const BARE_MODIFIER_COMMIT_DELAY: Duration = Duration::from_millis(80);

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

/// Human-readable chord rendering for `perf.log` so we can see the
/// FULL chord (modifiers + main key) rather than just the main key —
/// e.g. "Ctrl+MetaLeft" instead of just "MetaLeft", or "<bare> KeyV"
/// vs "Ctrl+Shift+KeyV". Saves a lot of guessing when a user reports
/// "my hotkey is weird."
fn format_chord(chord: &Chord) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if chord.modifiers.ctrl {
        parts.push("Ctrl");
    }
    if chord.modifiers.shift {
        parts.push("Shift");
    }
    if chord.modifiers.alt {
        parts.push("Alt");
    }
    if chord.modifiers.meta {
        parts.push("Meta");
    }
    let main = format!("{:?}", chord.key);
    if parts.is_empty() {
        format!("<bare> {main}")
    } else {
        format!("{}+{main}", parts.join("+"))
    }
}

/// True when this key is a modifier-only physical key (any Ctrl/Shift/Alt/Meta).
/// These keys never type a character on their own, so when bound as a bare
/// dictation hotkey we can pass them through to the OS — apps need to see
/// them so combos like Ctrl+V still work.
fn is_modifier_key(k: Key) -> bool {
    matches!(
        k,
        Key::ControlLeft
            | Key::ControlRight
            | Key::ShiftLeft
            | Key::ShiftRight
            | Key::Alt
            | Key::AltGr
            | Key::MetaLeft
            | Key::MetaRight
    )
}

/// True when the chord's MAIN KEY is itself a modifier — either a bare
/// modifier chord (no chord prefix, e.g. `ControlRight`) OR a chord whose
/// main key happens to be a modifier (e.g. `Ctrl+MetaLeft`).
///
/// Both cases need the deferred-commit + pass-through treatment so they
/// don't break combo shortcuts in the focused app. Without this, binding
/// dictation to `Ctrl+MetaLeft` would have Murmr eat the Win press the
/// instant Ctrl is held — breaking every Win+Ctrl+X system shortcut
/// (new virtual desktop, switch desktop, etc).
fn chord_main_key_is_modifier(chord: &Chord) -> bool {
    is_modifier_key(chord.key)
}

/// True when the chord is two-or-more modifiers (e.g. `Ctrl+MetaLeft`,
/// `Alt+Shift`). For these we use ORDER-INDEPENDENT matching — the
/// chord fires as soon as ALL required modifier keys are simultaneously
/// held, regardless of which one the user pressed first. The user's
/// hand isn't always going to land on Ctrl before Win; for a modifier-
/// only chord there's no meaningful "main key" to anchor the press
/// order against.
///
/// Bare modifier chords (just `ControlRight` alone, no prefix) stay
/// order-dependent so they keep matching the SPECIFIC L/R key the user
/// bound, not "any ctrl press."
fn chord_is_multi_modifier(chord: &Chord) -> bool {
    !chord.modifiers.empty() && is_modifier_key(chord.key)
}

/// True when every modifier required by `chord` (its `.modifiers`
/// prefix AND its `.key` if that key is itself a modifier) is currently
/// held according to `mods`. Order-independent. Returns false if the
/// chord's main key isn't a modifier (callers should check
/// `chord_is_multi_modifier` first).
fn chord_modifier_state_satisfied(chord: &Chord, mods: &ModifierSet) -> bool {
    if !is_modifier_key(chord.key) {
        return false;
    }
    // All prefix modifiers must be held.
    let required = &chord.modifiers;
    let prefix_held = (!required.ctrl || mods.ctrl)
        && (!required.shift || mods.shift)
        && (!required.alt || mods.alt)
        && (!required.meta || mods.meta);
    // Main-key modifier must also be held.
    let main_held = match chord.key {
        Key::ControlLeft | Key::ControlRight => mods.ctrl,
        Key::ShiftLeft | Key::ShiftRight => mods.shift,
        Key::Alt | Key::AltGr => mods.alt,
        Key::MetaLeft | Key::MetaRight => mods.meta,
        _ => false,
    };
    prefix_held && main_held
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
    /// `None` disables the edit-last shortcut entirely.
    pub edit_last: Option<Chord>,
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
            edit_last: None,
            cancel: Chord {
                modifiers: ModifierSet::default(),
                key: Key::Escape,
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum HotkeyEvent {
    /// `pressed_at` is the moment the user physically pressed the key, NOT
    /// when the controller received the event. The two diverge by ~80ms for
    /// bare-modifier hotkeys (deferred commit) — without using the original
    /// timestamp the controller's tap-vs-hold threshold ends up effectively
    /// 80ms higher than the user configured.
    DictationDown { pressed_at: Instant },
    DictationUp,
    EscDown,
    RepeatLast,
    /// Pop the last transcript into an editable HUD bubble.
    EditLast,
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
pub fn config_from_strings(
    dictation: &str,
    repeat: &str,
    edit_last: &str,
    cancel: &str,
) -> HotkeyConfig {
    let defaults = HotkeyConfig::default();
    HotkeyConfig {
        dictation: parse_chord(dictation).unwrap_or(defaults.dictation),
        repeat: parse_chord(repeat),
        edit_last: parse_chord(edit_last),
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

/// Set by the controller while a recording is in flight (HoldUncertain or
/// Toggled). The hotkey thread reads this to decide whether to suppress
/// the cancel key. We only steal Escape from the focused app while there's
/// actually something to cancel — otherwise Escape behaves normally
/// (closes menus, dismisses dialogs, etc.).
static RECORDING_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_recording_active(active: bool) {
    RECORDING_ACTIVE.store(active, Ordering::Relaxed);
}

/// Public mirror so the HUD can ask "am I supposed to be showing right
/// now?" on mount and self-heal if it missed the original Status emit
/// (cold-mount race, WebView2 wake-from-suspend, etc).
pub fn is_recording_active() -> bool {
    RECORDING_ACTIVE.load(Ordering::Relaxed)
}

/// When true (and we detect a fullscreen app is focused on the
/// foreground monitor), the hotkey thread silently skips its dictation
/// match — pass key events through to the OS untouched. Default true,
/// flipped at startup from `settings.pause_during_fullscreen` and
/// re-pushed on every save_settings.
static PAUSE_DURING_FULLSCREEN: AtomicBool = AtomicBool::new(true);

pub fn set_pause_during_fullscreen(pause: bool) {
    PAUSE_DURING_FULLSCREEN.store(pause, Ordering::Relaxed);
}

fn pause_during_fullscreen_enabled() -> bool {
    PAUSE_DURING_FULLSCREEN.load(Ordering::Relaxed)
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

/// Pending state for a bare-modifier dictation press. We delay firing
/// `DictationDown` until either:
///   - The commit timer elapses with no other key pressed → real dictation.
///   - Any other non-modifier key is pressed → it was a combo (Ctrl+V),
///     drop the pending state and never fire.
/// `pressed_at` doubles as a unique ID so a stale timer thread can detect
/// "this press was cancelled / superseded" by checking the timestamp.
#[derive(Default)]
struct PendingPress {
    pressed_at: Option<Instant>,
    committed: bool,
}

/// Spawn the OS keyboard hook thread. Bindings come from `initial_config`
/// and can be hot-swapped via `update_config(...)` without restarting.
///
/// Suppression rule: only the *main key* of a matched chord is consumed.
/// Modifiers always pass through normally — otherwise binding to
/// `Ctrl+Shift+V` would break every app's normal Ctrl handling.
///
/// **Hotkeys whose MAIN KEY is a modifier (Ctrl/Shift/Alt/Meta) get
/// special handling**. This covers both bare-modifier chords (e.g.
/// `ControlRight` alone) AND chords where the main key happens to be a
/// modifier (e.g. `Ctrl+MetaLeft`). For these we don't suppress the
/// press, AND we defer firing `DictationDown` by ~80ms so combos like
/// `Ctrl+V` (where the user's holding our dictation modifier briefly as
/// part of a shortcut) or `Win+Ctrl+D` (new virtual desktop) don't
/// trigger an unwanted recording. Any non-modifier key pressed inside
/// that 80ms window cancels the pending dictation. Push-to-talk loses
/// 80ms at the start of the recording — a negligible cost for keeping
/// system shortcuts working.
pub fn spawn(tx: Sender<HotkeyEvent>, initial_config: HotkeyConfig) {
    crate::perf_log::append(&format!(
        "[hotkey] installing OS keyboard hook (dictation={}, cancel={}, repeat={})",
        format_chord(&initial_config.dictation),
        format_chord(&initial_config.cancel),
        initial_config
            .repeat
            .as_ref()
            .map(format_chord)
            .unwrap_or_else(|| "<none>".into()),
    ));
    *config_handle().write() = initial_config;

    let modifiers = ModifierState::default();
    let mods_for_cb = modifiers.clone();
    let cfg_for_cb = config_handle();
    let pending = Arc::new(Mutex::new(PendingPress::default()));
    let pending_for_cb = pending.clone();
    let tx_for_cb = tx.clone();
    let first_event_seen = Arc::new(AtomicBool::new(false));
    let first_event_for_cb = first_event_seen.clone();
    // Keys whose PRESS we suppressed (consumed as a hotkey). We suppress the
    // matching RELEASE only for these, so key-down/up stay paired for the
    // focused app. The old code suppressed the release of the repeat/cancel
    // chord's *main key* with no modifier check — e.g. repeat = Ctrl+Shift+V
    // meant every `V` key-up got eaten, so a plain Ctrl+V (down passes, up
    // swallowed) left apps like Photoshop/Ableton seeing V stuck-down and
    // refusing further pastes. Pairing press↔release fixes that.
    let suppressed = Arc::new(Mutex::new(std::collections::HashSet::<Key>::new()));
    let suppressed_for_cb = suppressed.clone();
    // Tracks whether the dictation chord is CURRENTLY fully held. Only
    // meaningful for multi-modifier dictation chords (e.g. Ctrl+Win) —
    // we use it to detect held/released transitions on ANY modifier
    // press or release, so the chord matches order-independently. For
    // single-key / modifier+non-modifier chords this stays unused.
    let chord_held = Arc::new(AtomicBool::new(false));
    let chord_held_for_cb = chord_held.clone();

    std::thread::Builder::new()
        .name("murmr-hotkey".into())
        .spawn(move || {
            let result = grab(move |event: Event| -> Option<Event> {
                // One-shot sanity log so we can tell from perf.log whether
                // the OS hook is actually delivering events (if this never
                // fires, another app's hook may have refused us, or we're
                // running on a session without keyboard access — RDP idle,
                // locked workstation, etc).
                if !first_event_for_cb.swap(true, Ordering::Relaxed) {
                    crate::perf_log::append(
                        "[hotkey] first keyboard event received — hook is live",
                    );
                }
                let cfg = *cfg_for_cb.read();
                let dict_main_is_modifier = chord_main_key_is_modifier(&cfg.dictation);
                let dict_is_multi_mod = chord_is_multi_modifier(&cfg.dictation);

                // Snapshot modifier state PRIOR to applying this event so
                // chords with bare-modifier main keys (e.g. main = ControlRight,
                // expected modifiers = Ctrl) don't self-trigger when the user
                // presses ControlRight (which IS a Ctrl press but we want to
                // see "modifiers held BEFORE this press" for the match).
                let prior_mods = mods_for_cb.snapshot();
                mods_for_cb.apply(&event.event_type);

                // For multi-modifier dictation chords, compute the held/
                // released TRANSITION on every event so the chord matches
                // order-independently (Ctrl-then-Win works the same as
                // Win-then-Ctrl). For other chord shapes (bare modifier,
                // modifier+non-modifier) we use the original press-time
                // match logic below.
                let (dict_chord_just_held, dict_chord_just_released) = if dict_is_multi_mod {
                    let now_held = chord_modifier_state_satisfied(
                        &cfg.dictation,
                        &mods_for_cb.snapshot(),
                    );
                    let was_held = chord_held_for_cb.swap(now_held, Ordering::Relaxed);
                    (!was_held && now_held, was_held && !now_held)
                } else {
                    (false, false)
                };

                // Helper: does this event press the chord's main key with
                // EXACTLY the right modifiers held? (Order-DEPENDENT — used
                // for chords that aren't multi-modifier.)
                let matches_chord = |chord: &Chord, k: Key| -> bool {
                    k == chord.key && prior_mods == chord.modifiers
                };

                let (msg, suppress) = match event.event_type {
                    EventType::KeyPress(k) => {
                        // If a bare-modifier dictation press is currently
                        // pending (not yet committed) and this is some OTHER
                        // non-modifier key, the user is doing a combo.
                        // Cancel the pending press so DictationDown never
                        // fires. (Modifier-on-modifier presses don't count
                        // — pressing Shift while holding Ctrl is fine.)
                        if dict_main_is_modifier
                            && k != cfg.dictation.key
                            && !is_modifier_key(k)
                        {
                            let mut pp = pending_for_cb.lock();
                            if pp.pressed_at.is_some() && !pp.committed {
                                pp.pressed_at = None;
                            }
                        }

                        // Was the dictation chord triggered by THIS event?
                        //   - multi-modifier chords: chord just became fully held
                        //     (regardless of press order)
                        //   - everything else: exact main-key press with the
                        //     required modifiers already held (existing logic)
                        let dict_pressed = if dict_is_multi_mod {
                            dict_chord_just_held
                        } else {
                            matches_chord(&cfg.dictation, k)
                        };

                        // Fullscreen gate: when enabled (default), pass
                        // dictation-chord presses through to the OS while
                        // a fullscreen app is focused. Prevents the stuck-
                        // state bug from fullscreen games eating the key
                        // release, accidental in-game triggers, and the
                        // worst-case anti-cheat optics of Murmr's hotkey
                        // firing inside a competitive game. The hook
                        // itself stays installed — alt-tab and dictation
                        // resumes instantly.
                        if dict_pressed
                            && pause_during_fullscreen_enabled()
                            && crate::focus::is_foreground_fullscreen()
                        {
                            crate::perf_log::append(
                                "[hotkey] dictation press suppressed — fullscreen app focused (pause_during_fullscreen=true)",
                            );
                            return Some(event);
                        }

                        if dict_pressed {
                            if dict_main_is_modifier {
                                // Defer the commit. Spawn a one-shot timer
                                // that fires DictationDown after the delay
                                // IF the press is still pending (i.e.
                                // hasn't been cancelled by a combo or by
                                // an early release). Pass the modifier
                                // through so the focused app sees it.
                                //
                                // We capture press_at NOW (the actual
                                // user-perceived press time) and forward
                                // it on the eventual DictationDown so the
                                // controller's tap-vs-hold threshold is
                                // measured against the real press, not the
                                // delayed-commit moment.
                                let press_at = Instant::now();
                                {
                                    let mut pp = pending_for_cb.lock();
                                    pp.pressed_at = Some(press_at);
                                    pp.committed = false;
                                }
                                let pp_for_thread = pending_for_cb.clone();
                                let tx_for_thread = tx_for_cb.clone();
                                let _ = std::thread::Builder::new()
                                    .name("murmr-hotkey-defer".into())
                                    .spawn(move || {
                                        std::thread::sleep(BARE_MODIFIER_COMMIT_DELAY);
                                        let mut pp = pp_for_thread.lock();
                                        // `pressed_at` is our unique ID:
                                        // if a newer press has overwritten
                                        // it, OR the press got cancelled
                                        // (cleared to None), or the main
                                        // thread already committed, this
                                        // timer is stale.
                                        if pp.pressed_at == Some(press_at) && !pp.committed {
                                            pp.committed = true;
                                            drop(pp);
                                            let _ = tx_for_thread.send(
                                                HotkeyEvent::DictationDown { pressed_at: press_at },
                                            );
                                        } else {
                                            crate::perf_log::append(
                                                "[hotkey] bare-modifier dictation cancelled before commit (combo or early release)",
                                            );
                                        }
                                    });
                                (None, false)
                            } else {
                                (
                                    Some(HotkeyEvent::DictationDown { pressed_at: Instant::now() }),
                                    true,
                                )
                            }
                        } else if cfg
                            .repeat
                            .as_ref()
                            .map(|c| matches_chord(c, k))
                            .unwrap_or(false)
                        {
                            (Some(HotkeyEvent::RepeatLast), true)
                        } else if cfg
                            .edit_last
                            .as_ref()
                            .map(|c| matches_chord(c, k))
                            .unwrap_or(false)
                        {
                            (Some(HotkeyEvent::EditLast), true)
                        } else if matches_chord(&cfg.cancel, k) {
                            // Cancel key (default Escape) — only suppress
                            // and fire EscDown when there's an actual
                            // recording to cancel. Otherwise Escape just
                            // passes through to the focused app so menus,
                            // dialogs, and modals still dismiss normally.
                            if is_recording_active() {
                                (Some(HotkeyEvent::EscDown), true)
                            } else {
                                (None, false)
                            }
                        } else {
                            (None, false)
                        }
                    }
                    EventType::KeyRelease(k) => {
                        // Consume this key from the suppressed-press set (if
                        // present) up front — its value tells us whether we
                        // ate the matching press and therefore must eat the
                        // release too, keeping key-down/up paired.
                        let press_was_suppressed = suppressed_for_cb.lock().remove(&k);

                        // For multi-mod dictation chords: the release event
                        // is "any required modifier no longer held."
                        // dict_chord_just_released captured the transition
                        // above (true on the event that brought chord_held
                        // from true to false). For everything else, the
                        // release event is "the chord's main key was
                        // released."
                        let dict_released = if dict_is_multi_mod {
                            dict_chord_just_released
                        } else {
                            k == cfg.dictation.key
                        };

                        if dict_released {
                            if dict_main_is_modifier {
                                // Bare-modifier OR multi-modifier release.
                                // Only fire DictationUp if the press
                                // actually committed (passed the 80ms
                                // threshold without a combo cancelling
                                // it).
                                let was_committed = {
                                    let mut pp = pending_for_cb.lock();
                                    let c = pp.committed;
                                    pp.pressed_at = None;
                                    pp.committed = false;
                                    c
                                };
                                if was_committed {
                                    (Some(HotkeyEvent::DictationUp), false)
                                } else {
                                    (None, false)
                                }
                            } else {
                                // Mirror the press-side suppression for the
                                // dictation key release (so push-to-talk's
                                // release closes the recording cleanly).
                                (Some(HotkeyEvent::DictationUp), true)
                            }
                        } else if press_was_suppressed {
                            // We consumed this key's press as a hotkey
                            // (repeat / cancel-while-recording). Consume its
                            // release too so the focused app never sees a
                            // dangling key-up. Crucially this is keyed to
                            // THIS key's actual suppressed press — so a key
                            // whose press passed through (e.g. plain Ctrl+V
                            // when repeat is Ctrl+Shift+V) also has its
                            // release pass through.
                            (None, true)
                        } else {
                            (None, false)
                        }
                    }
                    _ => (None, false),
                };

                // Record a suppressed PRESS so its matching release above can
                // be suppressed too (and only then). Modifiers passed through
                // for pass-through chords aren't suppressed, so they never
                // land here.
                if suppress {
                    if let EventType::KeyPress(k) = event.event_type {
                        suppressed_for_cb.lock().insert(k);
                    }
                }

                if let Some(ev) = msg {
                    let _ = tx.send(ev);
                }
                if suppress { None } else { Some(event) }
            });
            if let Err(e) = result {
                eprintln!("[hotkey] rdev grab error: {e:?}");
                crate::perf_log::append(&format!("[hotkey] rdev grab returned error: {e:?}"));
            }
            // The grab loop only exits when the OS uninstalls our hook
            // (extremely rare during normal operation) or rdev errors.
            // Either way, log it — hotkeys will silently stop working
            // afterwards, which would otherwise be very confusing.
            crate::perf_log::append("[hotkey] grab() returned — keyboard hook is no longer active");
        })
        .expect("failed to spawn hotkey thread");
}
