//! Dictation orchestration.
//!
//! Runs in a dedicated OS thread that owns the recording state machine.
//! The hotkey thread sends `HotkeyEvent`s in; the controller drives the
//! Recorder, runs the transcribe + inject pipeline, emits status events
//! to the frontend, and shows / hides / positions the HUD window.
//!
//! State machine (per plan §6 #3-#9):
//!
//!   Idle ──Down──▶ HoldUncertain ──Up <250ms── ▶ Toggled
//!                              ╲ Up ≥250ms ─▶ stop+transcribe ▶ Idle
//!   Toggled ──Down──▶ stop+transcribe ▶ Idle
//!   any non-Idle ──Esc──▶ cancel ▶ Idle

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Monotonically increasing recording-session counter. Each new
/// `DictationDown` press bumps it. Background workers (the HUD
/// re-emit thread) capture the value at spawn time and compare to
/// the current value before firing — stale workers from a previous
/// session no-op, which prevents a delayed `Status::Recording`
/// from a recording that's already complete from clobbering the
/// HUD's view during a fresh dictation.
static RECORDING_SESSION: AtomicU64 = AtomicU64::new(0);

fn next_recording_session() -> u64 {
    RECORDING_SESSION.fetch_add(1, Ordering::Relaxed) + 1
}

fn current_recording_session() -> u64 {
    RECORDING_SESSION.load(Ordering::Relaxed)
}

/// The window the user was in when they pressed the edit-last hotkey — the
/// app the edited text should be re-injected into once they hit Insert.
/// Stashed here (rather than threaded through state) so the `insert_edited`
/// command can read it after the HUD has taken focus for editing.
static EDIT_TARGET: Mutex<Option<isize>> = Mutex::new(None);

pub fn set_edit_target(id: Option<isize>) {
    *EDIT_TARGET.lock() = id;
}

pub fn take_edit_target() -> Option<isize> {
    EDIT_TARGET.lock().take()
}

use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

use crate::audio::{self, Recorder};
use crate::audio_duck;
use crate::db::Db;
use crate::focus;
use crate::hotkey::{self, HotkeyEvent};
use crate::injector;
use crate::perf_log;
use crate::settings::SettingsStore;
use crate::sounds::SoundPlayer;
use crate::transcribe::{self, Transcriber};

/// Floor for the user-configurable tap-vs-hold threshold. Anything under
/// this is too short to reliably distinguish a deliberate tap from a brief
/// hold (KB latency + scheduler jitter eats most of the budget).
const MIN_TAP_THRESHOLD_MS: u32 = 80;

/// Pixel distance from the bottom of the screen's working area to the
/// bottom edge of the HUD window (used as fallback when no caret is found).
const HUD_BOTTOM_MARGIN_PX: i32 = 60;

/// Throttle RMS emit so we never exceed ~50 Hz, regardless of cpal block size.
const RMS_EMIT_INTERVAL: Duration = Duration::from_millis(20);

/// Energy-based VAD: speech-level RMS over a ~100 ms chunk. Plan §6 #9 — if
/// nothing in the captured audio crosses this, we skip Whisper to avoid
/// "Thanks for watching!" hallucinations from quiet rooms.
///
/// Mac builds use a much more permissive threshold. Built-in MacBook mics
/// record significantly quieter than the headset/desktop mics this was
/// originally tuned for — typical Mac speech-level RMS lands in the 0.001-0.01
/// range vs 0.02-0.1 on Windows. Whisper handles near-silent audio fine; the
/// occasional empty-room hallucination is a smaller cost than missing real
/// speech.
#[cfg(target_os = "macos")]
const VAD_RMS_THRESHOLD: f32 = 0.001;
#[cfg(not(target_os = "macos"))]
const VAD_RMS_THRESHOLD: f32 = 0.015;

/// Minimum number of speech chunks (out of all 100 ms chunks) that must be
/// above the RMS threshold before we treat the recording as containing real
/// speech. Catches the case where the user clears their throat once but
/// otherwise stays quiet.
const VAD_MIN_SPEECH_CHUNKS: usize = 3;

/// Maximum a recording can run before our watchdog assumes the
/// release event was lost (typical cause: a fullscreen game ate the
/// key release before our global hook saw it) and force-unducks
/// audio. Set high enough that no real dictation can hit it (normal
/// dictations finish in seconds, long ones in minutes) but short
/// enough that a stuck state self-recovers within one cup of coffee.
const MAX_RECORDING_DURATION: Duration = Duration::from_secs(10 * 60);

/// How recently an identical transcript must have been injected for us to
/// treat a repeat as a Whisper echo rather than the user genuinely saying
/// the same short phrase again. Beyond this window, "okay" / "yes" / "next"
/// spoken a second time is accepted normally.
const DUPLICATE_ECHO_WINDOW: Duration = Duration::from_secs(12);

/// Window during which a follow-up dictation into the SAME app gets a
/// leading space (smart spacing) so consecutive bursts don't butt together.
const SMART_SPACING_WINDOW: Duration = Duration::from_secs(8);

/// Recordings longer than this are transcribed in chunks instead of one
/// giant `whisper.full()` call, so a failure late in a long take doesn't
/// discard everything before it, and each chunk's text lands as it finishes.
const CHUNK_THRESHOLD_SECONDS: f64 = 45.0;
/// Target length of each transcription chunk for long recordings.
const CHUNK_TARGET_SECONDS: f64 = 30.0;

/// The most recent successful injection — text plus when and where it
/// happened. Drives re-paste, the time-gated duplicate check, and smart
/// spacing between consecutive dictations.
#[derive(Debug, Clone)]
pub struct LastInjection {
    pub text: String,
    /// When it was injected. `None` for the DB-seeded value at startup (a
    /// previous session) so a first dictation is never treated as a dup.
    pub at: Option<Instant>,
    /// Foreground app it was injected into, for smart spacing.
    pub app: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecState {
    Idle,
    HoldUncertain,
    Toggled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Status {
    Idle,
    Recording,
    Transcribing,
    Injected { text: String, source_app: Option<String> },
    Cancelled,
    /// We recorded but heard no usable speech (below the VAD threshold, or
    /// only a hallucination). Distinct from `Cancelled` so the HUD can show
    /// a "didn't catch that" hint instead of vanishing silently.
    NoSpeech,
    Error { message: String },
}

pub struct Controller {
    recorder: Arc<Recorder>,
    transcriber: Arc<Transcriber>,
    db: Arc<Db>,
    settings: Arc<SettingsStore>,
    sounds: Arc<SoundPlayer>,
    app: AppHandle,
    /// Last text we successfully injected (+ when/where). Used by the
    /// re-paste hotkey, the time-gated duplicate check, and smart spacing.
    last_injected: Arc<Mutex<Option<LastInjection>>>,
    /// Foreground window/app captured at record-start, so injection lands in
    /// the window the user was in when they began — not wherever focus
    /// happens to be when transcription finishes.
    capture_target: Arc<Mutex<Option<focus::CaptureTarget>>>,
    /// When true, transcriptions are emitted via `murmr:status` but NOT
    /// pasted into the focused field and NOT saved to the DB. Used by the
    /// onboarding wizard's Practice step.
    practice_mode: Arc<AtomicBool>,
}

impl Controller {
    pub fn new(
        transcriber: Arc<Transcriber>,
        db: Arc<Db>,
        settings: Arc<SettingsStore>,
        sounds: Arc<SoundPlayer>,
        app: AppHandle,
        practice_mode: Arc<AtomicBool>,
    ) -> Self {
        let seed = db
            .recent_transcriptions(1)
            .ok()
            .and_then(|v| v.into_iter().next())
            .map(|t| LastInjection {
                text: t.text,
                at: None,
                app: None,
            });
        Self {
            recorder: Arc::new(Recorder::new()),
            transcriber,
            db,
            settings,
            sounds,
            app,
            last_injected: Arc::new(Mutex::new(seed)),
            capture_target: Arc::new(Mutex::new(None)),
            practice_mode,
        }
    }

    pub fn spawn(self, rx: Receiver<HotkeyEvent>) {
        std::thread::Builder::new()
            .name("murmr-controller".into())
            .spawn(move || self.run(rx))
            .expect("failed to spawn controller thread");
    }

    fn run(self, rx: Receiver<HotkeyEvent>) {
        let mut state = RecState::Idle;
        let mut press_at: Option<Instant> = None;

        while let Ok(ev) = rx.recv() {
            // License gate removed in v0.1.23 — Murmr is free for anyone.
            match (ev, state) {
                (HotkeyEvent::DictationDown { pressed_at }, RecState::Idle) => {
                    let session = next_recording_session();
                    perf_log::append(&format!(
                        "[ctrl] DictationDown received → start_recording (session={session})"
                    ));
                    if let Err(e) = self.start_recording() {
                        perf_log::append(&format!("[ctrl] start_recording failed: {e}"));
                        self.sounds.play_error_beep();
                        self.emit(Status::Error {
                            message: format!("failed to start recording: {e}"),
                        });
                        continue;
                    }
                    // Capture the window/app the user is focused on RIGHT NOW,
                    // before transcription (which can take seconds and during
                    // which they may alt-tab). Injection later re-targets this
                    // window so text never lands in the wrong place, and the
                    // app name labels the Insights breakdown.
                    let target = focus::capture_foreground();
                    perf_log::append(&format!(
                        "[ctrl] captured foreground target: app={:?} id={:?}",
                        target.app_name, target.window_id
                    ));
                    *self.capture_target.lock() = Some(target);
                    state = RecState::HoldUncertain;
                    hotkey::set_recording_active(true);
                    // Use the user-perceived press time, not Now. For bare-
                    // modifier dictation the hotkey thread defers the
                    // event by ~80ms; without this the tap-vs-hold logic
                    // below sees every press as 80ms shorter than it
                    // actually was, dropping marginal taps into Toggled
                    // mode unexpectedly.
                    press_at = Some(pressed_at);
                    self.emit(Status::Recording);
                    self.show_hud();
                    self.sounds.play_start_click();
                    // Belt-and-suspenders watchdog: if NO release event
                    // fires within MAX_RECORDING duration (10 min — far
                    // longer than any real dictation), assume the key
                    // release got swallowed by a fullscreen game or
                    // similar exclusive-keyboard-grabber, and synthesize
                    // a DictationUp so the recording completes + audio
                    // unducks. Tagged with `session` so a stale watchdog
                    // from a prior recording can't fire into a fresh
                    // one. The recovery DictationDown-in-HoldUncertain
                    // arm below is the primary path when the user
                    // notices and presses again; this catches the case
                    // where they don't notice at all (walked away,
                    // alt-tabbed and didn't think to retry, etc).
                    self.spawn_max_duration_watchdog(session);
                    // Re-emit Status::Recording on a short delay tail —
                    // catches the cases where the HUD's React listener
                    // wasn't ready for the first emit (app just opened,
                    // WebView just woke from idle, etc). The HUD's
                    // recording-state reducer is idempotent — duplicate
                    // events are no-ops, so it's safe to fire several.
                    // The `session` arg keeps stale workers from a prior
                    // recording from firing into a fresh session.
                    self.reemit_recording_after_show(session);
                }

                (HotkeyEvent::DictationDown { .. }, RecState::Toggled) => {
                    perf_log::append("[ctrl] DictationDown in Toggled → stop + transcribe");
                    state = RecState::Idle;
                    hotkey::set_recording_active(false);
                    press_at = None;
                    // Stop sound fires on the user's action (the second tap),
                    // BEFORE transcription runs — keeps the audio feedback
                    // tied to the keypress, not delayed by Whisper.
                    self.sounds.play_complete_chime();
                    self.complete_recording();
                }

                (HotkeyEvent::DictationUp, RecState::HoldUncertain) => {
                    let elapsed = press_at.map(|t| t.elapsed()).unwrap_or_default();
                    let tap_threshold_ms =
                        self.settings.get().tap_threshold_ms.max(MIN_TAP_THRESHOLD_MS);
                    let tap_threshold = Duration::from_millis(tap_threshold_ms as u64);
                    perf_log::append(&format!(
                        "[ctrl] DictationUp in HoldUncertain: elapsed={}ms, tap_threshold={}ms → {}",
                        elapsed.as_millis(),
                        tap_threshold_ms,
                        if elapsed >= tap_threshold { "push-to-talk complete" } else { "Toggled" },
                    ));
                    if elapsed >= tap_threshold {
                        state = RecState::Idle;
                        hotkey::set_recording_active(false);
                        press_at = None;
                        // Push-to-talk release — fire stop sound immediately.
                        self.sounds.play_complete_chime();
                        self.complete_recording();
                    } else {
                        state = RecState::Toggled;
                        // recording_active stays true — Toggled means recording continues
                    }
                }
                (HotkeyEvent::DictationUp, _) => {}

                (HotkeyEvent::EscDown, RecState::HoldUncertain | RecState::Toggled) => {
                    perf_log::append("[ctrl] EscDown → cancel recording");
                    let _ = self.recorder.cancel();
                    audio_duck::unduck();
                    state = RecState::Idle;
                    hotkey::set_recording_active(false);
                    press_at = None;
                    self.emit(Status::Cancelled);
                    self.hide_hud();
                }

                (HotkeyEvent::EscDown, RecState::Idle) => {}

                // Recovery path: a second DictationDown arrives while we
                // think we're still in HoldUncertain from the LAST press.
                // In normal operation this never fires — the press handler
                // in hotkey/mod.rs only emits DictationDown ONCE per
                // chord-held period, not on auto-repeat. So if we see
                // one here, the previous release was lost — typically
                // because a fullscreen game (Apex Legends, etc) ate the
                // key release before our global hook saw it. Without this
                // arm the controller was stuck in HoldUncertain forever:
                // audio stayed ducked, subsequent presses silently
                // no-op'd, the user had to restart Murmr to recover.
                //
                // Treat the second press as "complete the orphaned
                // recording, then go back to Idle." User can press the
                // hotkey one more time to start a fresh recording.
                (HotkeyEvent::DictationDown { .. }, RecState::HoldUncertain) => {
                    perf_log::append(
                        "[ctrl] DictationDown in HoldUncertain → previous release lost, completing + recovering",
                    );
                    state = RecState::Idle;
                    hotkey::set_recording_active(false);
                    press_at = None;
                    self.sounds.play_complete_chime();
                    self.complete_recording();
                }

                // Re-paste the most recent transcription. Ignored mid-recording
                // (would interfere with the active dictation flow).
                (HotkeyEvent::RepeatLast, RecState::Idle) => {
                    self.repaste_last();
                }
                (HotkeyEvent::RepeatLast, _) => {}

                // Pop the last transcript into an editable HUD bubble.
                (HotkeyEvent::EditLast, RecState::Idle) => {
                    self.edit_last();
                }
                (HotkeyEvent::EditLast, _) => {}
            }
        }
    }

    fn repaste_last(&self) {
        let text = match self.last_injected.lock().as_ref() {
            Some(li) if !li.text.is_empty() => li.text.clone(),
            _ => {
                self.emit(Status::Cancelled);
                return;
            }
        };
        let app = self.app.clone();
        let keystroke = self.settings.get().injection_mode == "keystroke";
        std::thread::Builder::new()
            .name("murmr-repaste".into())
            .spawn(move || {
                if let Err(e) = injector::inject_text(&text, keystroke) {
                    let _ = app.emit(
                        "murmr:status",
                        &Status::Error {
                            message: format!("re-paste failed: {e}"),
                        },
                    );
                    return;
                }
                let _ = app.emit(
                    "murmr:status",
                    &Status::Injected {
                        text,
                        source_app: None,
                    },
                );
            })
            .expect("failed to spawn re-paste thread");
    }

    /// Pop the most recent transcript into an editable HUD bubble so a single
    /// mis-heard word can be fixed and re-injected. Triggered by the optional
    /// `edit_last_hotkey`.
    ///
    /// Focus dance: the HUD normally never takes focus (so injection targets
    /// the user's app). For editing we DO need it focused so the user can
    /// type — so we first stash the current foreground window (the app the
    /// edited text will go back into), then show + focus the HUD. On Insert,
    /// the `insert_edited` command restores focus to that window before
    /// injecting.
    fn edit_last(&self) {
        let text = self
            .last_injected
            .lock()
            .as_ref()
            .map(|li| li.text.clone())
            .unwrap_or_default();
        if text.is_empty() {
            self.emit(Status::Cancelled);
            return;
        }
        // Remember the app to re-inject into (current foreground — the HUD
        // hasn't taken focus yet).
        set_edit_target(focus::foreground_window_id());
        // Tell the HUD to render the editable bubble, then show + focus it.
        let _ = self.app.emit("murmr:edit-last", text);
        self.show_hud();
        if let Some(hud) = self.app.get_webview_window("hud") {
            let _ = hud.set_focus();
        }
    }

    fn start_recording(&self) -> Result<(), String> {
        // RMS emit closure: throttled, fires Tauri event so the HUD waveform
        // can react in real time.
        let app = self.app.clone();
        let last_emit = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(1)));
        let rms_cb: audio::RmsCallback = Arc::new(move |rms: f32| {
            let mut guard = last_emit.lock();
            if guard.elapsed() < RMS_EMIT_INTERVAL {
                return;
            }
            *guard = Instant::now();
            drop(guard);
            let _ = app.emit("murmr:audio-rms", rms);
        });

        // Mic disconnect / device error → surface to UI + play error beep.
        let app_err = self.app.clone();
        let sounds_err = Arc::clone(&self.sounds);
        let err_cb: audio::ErrorCallback = Arc::new(move |msg: String| {
            sounds_err.play_error_beep();
            let _ = app_err.emit(
                "murmr:status",
                &Status::Error {
                    message: format!("Microphone error: {msg}"),
                },
            );
        });

        let result = self
            .recorder
            .start_with_callbacks(Some(rms_cb), Some(err_cb));

        // Duck system audio AFTER successful recorder start. Doing it before
        // would mean a failed start (mic perms, device gone) leaves the
        // system at a permanently lower volume until the next recording.
        if result.is_ok() {
            let amount = self.settings.get().audio_duck_amount;
            audio_duck::duck(amount);
        }
        result
    }

    fn complete_recording(&self) {
        // Restore system audio volume FIRST so the chime/typing sounds the
        // user hears next aren't artificially quiet.
        audio_duck::unduck();

        self.emit(Status::Transcribing);

        let cap = match self.recorder.stop() {
            Ok(Some(c)) => c,
            Ok(None) => {
                perf_log::append("[ctrl] complete_recording: stop returned None (cancelled)");
                self.emit(Status::Cancelled);
                self.hide_hud();
                return;
            }
            Err(e) => {
                perf_log::append(&format!("[ctrl] complete_recording: stop failed: {e}"));
                self.emit(Status::Error {
                    message: format!("stop failed: {e}"),
                });
                self.hide_hud();
                return;
            }
        };

        // Always log a one-line summary of every recording so we can
        // diagnose missing-transcription reports without the user having
        // to repro under instrumentation. Includes duration, sample rate,
        // channels, peak chunk RMS, and the VAD threshold for comparison.
        let frames_total = cap.samples.len() as f64 / cap.channels.max(1) as f64;
        let duration_ms_dbg = ((frames_total / cap.sample_rate.max(1) as f64) * 1000.0) as i64;
        let peak = peak_chunk_rms(&cap.samples, cap.sample_rate);
        perf_log::append(&format!(
            "[ctrl] recording captured: {}ms, {} samples @ {}Hz x {}ch, peak chunk RMS {:.4} (VAD threshold {:.4})",
            duration_ms_dbg,
            cap.samples.len(),
            cap.sample_rate,
            cap.channels,
            peak,
            VAD_RMS_THRESHOLD,
        ));

        // Energy-based VAD — bail out before invoking Whisper on silence.
        // Emit NoSpeech (not Cancelled) so the HUD can tell the user we
        // didn't hear them, rather than just vanishing.
        if !has_speech(&cap.samples, cap.sample_rate, VAD_RMS_THRESHOLD) {
            perf_log::append(&format!(
                "[ctrl] VAD rejected: peak chunk RMS {:.4} below threshold {:.4} → no transcription",
                peak, VAD_RMS_THRESHOLD,
            ));
            self.emit(Status::NoSpeech);
            self.hide_hud();
            return;
        }

        let transcriber = Arc::clone(&self.transcriber);
        let db = Arc::clone(&self.db);
        let settings_store = Arc::clone(&self.settings);
        let sounds = Arc::clone(&self.sounds);
        let app = self.app.clone();
        let last_injected = Arc::clone(&self.last_injected);
        let practice_mode = Arc::clone(&self.practice_mode);
        // Take the window/app we captured at record-start; injection re-targets
        // it so text can't land in whatever window has focus when Whisper
        // finishes.
        let capture_target = self.capture_target.lock().take().unwrap_or_default();

        let frames = cap.samples.len() as f64 / cap.channels.max(1) as f64;
        let duration_ms = ((frames / cap.sample_rate.max(1) as f64) * 1000.0) as i64;

        // Pull the current settings + dictionary up-front so the transcribe
        // thread doesn't have to touch shared state mid-flight.
        let settings = settings_store.get();
        let dictionary = db.list_dictionary(None).unwrap_or_default();
        let initial_prompt = transcribe::build_initial_prompt(&dictionary);

        std::thread::Builder::new()
            .name("murmr-transcribe".into())
            .spawn(move || {
                let hide_hud = || {
                    if let Some(hud) = app.get_webview_window("hud") {
                        let _ = hud.hide();
                    }
                };

                perf_log::append("[ctrl] post-stop: starting resample + transcribe");
                let result = (|| -> Result<String, String> {
                    let samples_16k =
                        audio::to_whisper_format(&cap.samples, cap.sample_rate, cap.channels)?;
                    perf_log::append("[ctrl] resample done, invoking whisper");
                    transcribe_maybe_chunked(
                        &transcriber,
                        &samples_16k,
                        initial_prompt.as_deref(),
                        settings.accuracy_mode,
                    )
                })();
                perf_log::append(&format!(
                    "[ctrl] whisper returned: ok={}",
                    result.is_ok()
                ));

                let result = result.map(|raw| transcribe::process(&raw, &settings, &dictionary));
                perf_log::append("[ctrl] postprocess done");

                // Extract text + stripped fillers so the rest of the match
                // arms only deal with `String` like before.
                let (result, stripped_fillers) = match result {
                    Ok(outcome) => (Ok(outcome.text), outcome.stripped_fillers),
                    Err(e) => (Err(e), Default::default()),
                };

                match result {
                    Ok(text) if text.is_empty() => {
                        // Whisper produced nothing usable (empty or a discarded
                        // hallucination) — tell the user we didn't catch it.
                        let _ = app.emit("murmr:status", &Status::NoSpeech);
                        hide_hud();
                    }
                    Ok(text) if text_is_suspicious(&text, &last_injected) => {
                        eprintln!(
                            "[whisper] discarded result that looks like a duplicate of the last injection: {text:?}"
                        );
                        let _ = app.emit("murmr:status", &Status::Cancelled);
                        hide_hud();
                    }
                    Ok(text) => {
                        // Practice mode (onboarding wizard): emit the result
                        // for the UI to show, but don't paste anywhere and
                        // don't save to history.
                        // (No play_complete_chime here — fires on release in
                        // run() now, not after transcription.)
                        if practice_mode.load(Ordering::Relaxed) {
                            let _ = app.emit(
                                "murmr:status",
                                &Status::Injected {
                                    text,
                                    source_app: None,
                                },
                            );
                            hide_hud();
                            return;
                        }

                        // Verify we're still pointed at the window the user
                        // started in. If focus moved during transcription,
                        // try to restore it; if that fails, refuse to paste
                        // (dropping text into the wrong app is worse than not
                        // pasting) and keep it available for re-paste.
                        if let Some(target_id) = capture_target.window_id {
                            if focus::foreground_window_id() != Some(target_id) {
                                perf_log::append(
                                    "[ctrl] focus changed since record-start — attempting refocus",
                                );
                                focus::refocus_window(target_id);
                                std::thread::sleep(Duration::from_millis(60));
                                if focus::foreground_window_id() != Some(target_id) {
                                    perf_log::append(
                                        "[ctrl] refocus failed — refusing to paste into the wrong window",
                                    );
                                    // Keep the text for re-paste, but mark it
                                    // as NOT actually injected (at: None) so
                                    // the next dictation's smart-spacing
                                    // doesn't prepend a space off a paste that
                                    // never landed.
                                    *last_injected.lock() = Some(LastInjection {
                                        text: text.clone(),
                                        at: None,
                                        app: capture_target.app_name.clone(),
                                    });
                                    sounds.play_error_beep();
                                    let _ = app.emit(
                                        "murmr:status",
                                        &Status::Error {
                                            message: "Focus changed — text not inserted. Use re-paste to drop it where you want it.".into(),
                                        },
                                    );
                                    hide_hud();
                                    return;
                                }
                            }
                        }

                        // Smart spacing: if the previous dictation went into
                        // the SAME app moments ago, prefix a space so back-to-
                        // back bursts don't butt together ("…manifest.Next…").
                        let mut to_inject = text.clone();
                        if settings.smart_spacing {
                            let guard = last_injected.lock();
                            if let Some(li) = guard.as_ref() {
                                let recent = li
                                    .at
                                    .map(|t| t.elapsed() < SMART_SPACING_WINDOW)
                                    .unwrap_or(false);
                                let same_app =
                                    li.app.is_some() && li.app == capture_target.app_name;
                                let starts_wordy = to_inject
                                    .chars()
                                    .next()
                                    .map(|c| c.is_alphanumeric())
                                    .unwrap_or(false);
                                if recent && same_app && starts_wordy {
                                    to_inject.insert(0, ' ');
                                }
                            }
                        }

                        let keystroke = settings.injection_mode == "keystroke";
                        perf_log::append(&format!(
                            "[ctrl] starting inject_text (keystroke={keystroke})"
                        ));
                        if let Err(e) = injector::inject_text(&to_inject, keystroke) {
                            perf_log::append(&format!("[ctrl] inject_text failed: {e}"));
                            sounds.play_error_beep();
                            let _ = app.emit(
                                "murmr:status",
                                &Status::Error {
                                    message: format!("injection failed: {e}"),
                                },
                            );
                            hide_hud();
                            return;
                        }
                        perf_log::append("[ctrl] inject_text ok");

                        let word_count = text.split_whitespace().count() as i64;
                        perf_log::append("[ctrl] before insert_transcription");
                        match db.insert_transcription(
                            &text,
                            word_count,
                            duration_ms,
                            capture_target.app_name.as_deref(),
                        ) {
                            Ok(_) => {
                                perf_log::append("[ctrl] insert_transcription ok");
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as i64)
                                    .unwrap_or(0);
                                if let Err(e) = db.bump_streak_day(now_ms) {
                                    eprintln!("[db] failed to bump streak day: {e}");
                                }
                                perf_log::append("[ctrl] bump_streak_day ok");
                                if !stripped_fillers.is_empty() {
                                    let entries: Vec<(String, i64)> =
                                        stripped_fillers.iter().map(|(k, v)| (k.clone(), *v)).collect();
                                    if let Err(e) = db.bump_fillers(&entries) {
                                        eprintln!("[db] failed to bump fillers: {e}");
                                    }
                                    // Also append to the time-indexed
                                    // `filler_events` log so the Insights
                                    // page can ask "how many 'um's last
                                    // month vs the month before" — the
                                    // cumulative counts in `bump_fillers`
                                    // can't answer windowed questions.
                                    if let Err(e) = db.bump_filler_events(&entries) {
                                        eprintln!("[db] failed to bump filler_events: {e}");
                                    }
                                }
                                perf_log::append("[ctrl] fillers persisted");
                                let _ = app.emit("murmr:transcription-saved", &());
                                perf_log::append("[ctrl] transcription-saved event emitted");

                                // Background-fire any milestone notifications
                                // earned by this transcription. Spawns its
                                // own thread + handles the 4-second delay,
                                // fullscreen-detection, and the
                                // milestones_reached de-dup itself. Wrapped
                                // in catch_unwind defensively so a panic
                                // inside the notification flow (Tauri
                                // notification plugin, Win32 fullscreen
                                // detection, etc) can't take down Murmr —
                                // it just gets logged.
                                let app_for_notify = app.clone();
                                let db_for_notify = Arc::clone(&db);
                                let settings_for_notify = Arc::clone(&settings_store);
                                let notify_result = std::panic::catch_unwind(
                                    std::panic::AssertUnwindSafe(|| {
                                        crate::notifications::check_and_fire(
                                            app_for_notify,
                                            db_for_notify,
                                            settings_for_notify,
                                            word_count,
                                            duration_ms,
                                        );
                                    }),
                                );
                                if let Err(e) = notify_result {
                                    let msg = e
                                        .downcast_ref::<&str>()
                                        .copied()
                                        .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                                        .unwrap_or("<no message>");
                                    perf_log::append(&format!(
                                        "[ctrl] notifications::check_and_fire panicked: {msg}"
                                    ));
                                } else {
                                    perf_log::append("[ctrl] notifications scheduled");
                                }
                            }
                            Err(e) => {
                                perf_log::append(&format!(
                                    "[ctrl] insert_transcription failed: {e}"
                                ));
                                eprintln!("[db] failed to write transcription: {e}");
                            }
                        }

                        *last_injected.lock() = Some(LastInjection {
                            text: text.clone(),
                            at: Some(Instant::now()),
                            app: capture_target.app_name.clone(),
                        });

                        // Stop chime already fired on release in run() —
                        // don't double-play here.
                        let _ = app.emit(
                            "murmr:status",
                            &Status::Injected {
                                text,
                                source_app: capture_target.app_name.clone(),
                            },
                        );
                        hide_hud();
                    }
                    Err(e) => {
                        sounds.play_error_beep();
                        let _ = app.emit(
                            "murmr:status",
                            &Status::Error {
                                message: format!("transcription failed: {e}"),
                            },
                        );
                        hide_hud();
                    }
                }
            })
            .expect("failed to spawn transcription thread");
    }

    fn emit(&self, status: Status) {
        let _ = self.app.emit("murmr:status", &status);
    }

    /// Show the HUD. Try, in order:
    /// 1. UIA focused-element rect (Chrome, Electron, VS Code, WinUI, modern Office)
    /// 2. legacy Win32 caret rect (old EDIT/RICHEDIT controls)
    /// 3. bottom of the foreground window
    /// 4. bottom-center of the primary monitor
    fn show_hud(&self) {
        let hud = match self.app.get_webview_window("hud") {
            Some(h) => h,
            None => {
                // Tauri normally creates the HUD from tauri.conf.json at
                // startup, but on post-boot launches (auto-start via the
                // Windows Run key while the session is still warming up)
                // window creation races with WebView2 init and can fail
                // silently. Previously the user had to quit + relaunch
                // Murmr to recover. Now we re-create the HUD on the spot.
                perf_log::append(
                    "[hud] window missing at show_hud time — recreating via WebviewWindowBuilder",
                );
                match create_hud_window(&self.app) {
                    Ok(h) => h,
                    Err(e) => {
                        perf_log::append(&format!(
                            "[hud] recreation failed: {e} — dictation will proceed without a HUD this round",
                        ));
                        return;
                    }
                }
            }
        };

        let placement = focus::uia_focused_element_rect()
            .and_then(|rect| position_hud_below_field(&hud, rect).ok().map(|_| "uia"))
            .or_else(|| {
                focus::focused_caret_screen_rect()
                    .and_then(|rect| position_hud_near_caret(&hud, rect).ok().map(|_| "caret"))
            })
            .or_else(|| {
                focus::focused_window_screen_rect()
                    .and_then(|rect| position_hud_below_window(&hud, rect).ok().map(|_| "window"))
            })
            .or_else(|| position_hud_bottom_center(&hud).ok().map(|_| "screen"));

        match placement {
            Some(via) => perf_log::append(&format!("[hud] positioned via {via}")),
            None => perf_log::append(
                "[hud] all positioning strategies failed — HUD will appear at last known position",
            ),
        }

        // Defensive recovery: undo any window state that could leave the
        // HUD invisible from a prior session — minimized (post-boot we've
        // seen Windows restore the previous session's minimized state),
        // not on top, or hidden. Errors are logged but non-fatal.
        if let Err(e) = hud.unminimize() {
            perf_log::append(&format!("[hud] unminimize failed: {e}"));
        }
        if let Err(e) = hud.show() {
            perf_log::append(&format!("[hud] show failed: {e}"));
        }
        if let Err(e) = hud.set_always_on_top(true) {
            perf_log::append(&format!("[hud] set_always_on_top failed: {e}"));
        }

        // Streamer mode: exclude the HUD from screen capture so the recording
        // pill never shows up in OBS / a shared screen. Re-applied on every
        // show so toggling the setting takes effect on the next dictation.
        apply_capture_exclusion(&hud, self.settings.get().streamer_mode);

        // Verify the show actually took. If the window reports invisible
        // after we asked it to show, that's the post-boot bug — log it
        // and call show() once more. We deliberately do NOT call
        // set_focus here: the HUD must never steal focus from the user's
        // text field or text injection lands in the wrong window.
        match hud.is_visible() {
            Ok(true) => {}
            Ok(false) => {
                perf_log::append(
                    "[hud] is_visible == false after show() — retrying once",
                );
                if let Err(e) = hud.show() {
                    perf_log::append(&format!("[hud] retry show failed: {e}"));
                }
            }
            Err(e) => perf_log::append(&format!("[hud] is_visible query failed: {e}")),
        }

        // Bounds sanity check. If our positioning chain (or a prior
        // session's saved position) put the HUD outside every connected
        // monitor's working area — multi-monitor unplug, scale change,
        // UIA returning bogus coords for a fullscreen game's focus, etc.
        // — fall back to the guaranteed-visible bottom-center placement.
        // Without this, the HUD is "shown" but invisible to the user, which
        // they understandably read as "HUD missing."
        if !hud_within_any_monitor(&hud) {
            perf_log::append(
                "[hud] window position is outside every monitor — snapping to screen bottom-center",
            );
            if let Err(e) = position_hud_bottom_center(&hud) {
                perf_log::append(&format!("[hud] fallback positioning failed: {e}"));
            }
        }

        // Tell the HUD's React app "you've just been shown — re-query
        // current state and render the right view." This is a sharper
        // signal than the timed Status::Recording re-emits: even if
        // every prior emit was lost (cold launch, WebView suspended at
        // emit time, etc.) this event will fire after the WebView
        // resumes and drain its message queue, and the HUD's
        // `murmr:hud-shown` listener will trigger a self-heal that
        // calls `is_recording_active` and lands in the right view.
        let _ = self.app.emit("murmr:hud-shown", ());
    }

    fn hide_hud(&self) {
        if let Some(hud) = self.app.get_webview_window("hud") {
            let _ = hud.hide();
        }
    }

    /// Re-fires Status::Recording after a short delay, then again later.
    /// Mitigates the "I heard the sound but never saw the HUD" race
    /// where the HUD's React listener hadn't mounted yet (first
    /// dictation after launch) or had been suspended (long idle, then
    /// woke from WebView2's process freezer). Spawned on a worker
    /// thread so it doesn't block the controller's hot path.
    ///
    /// `session` is the recording session ID at spawn time; if it no
    /// longer matches the current session by the time the delay
    /// elapses, the user has moved on (this recording ended and a new
    /// one may already be in progress) and we silently skip the emit.
    /// Without this, a delayed re-emit from a previous recording could
    /// override the HUD's view AFTER a fresh dictation's
    /// `Status::Transcribing` arrived, leaving the pill in the wrong
    /// state.
    ///
    /// The HUD reducer is idempotent for in-session duplicates —
    /// receiving Status::Recording multiple times while already in
    /// `recording` view is a no-op.
    fn reemit_recording_after_show(&self, session: u64) {
        let app = self.app.clone();
        std::thread::Builder::new()
            .name("murmr-hud-resync".into())
            .spawn(move || {
                // Two delays cover the realistic window: ~120ms is
                // enough for a cold-mounted React app to finish
                // attaching listeners, 500ms covers slow disks / a
                // WebView coming out of deep suspend.
                for delay_ms in [120u64, 500u64] {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    if current_recording_session() != session
                        || !hotkey::is_recording_active()
                    {
                        // Either a newer session is in flight OR this
                        // session has ended (recording completed,
                        // cancelled, or Toggled→completed). Either way,
                        // skipping the emit avoids clobbering the HUD's
                        // current view with a stale Recording event.
                        return;
                    }
                    let _ = app.emit("murmr:status", Status::Recording);
                }
            })
            .ok();
    }

    /// Belt-and-suspenders cleanup for "release event got eaten by a
    /// fullscreen game / exclusive-keyboard-grab and the user walked
    /// away." Sleeps for the max-recording duration and, if our session
    /// is STILL marked active by then, force-unducks audio so the
    /// user's other apps don't sit at half-volume indefinitely. Does
    /// NOT touch controller state — the user's next hotkey press (or
    /// Esc) goes through the recovery arm above and restores the state
    /// machine cleanly; this just handles the audio side while we
    /// wait.
    ///
    /// `session` keeps stale watchdogs from a prior recording from
    /// firing into a fresh one — same pattern as the HUD re-emit.
    fn spawn_max_duration_watchdog(&self, session: u64) {
        std::thread::Builder::new()
            .name("murmr-max-duration".into())
            .spawn(move || {
                std::thread::sleep(MAX_RECORDING_DURATION);
                if current_recording_session() != session
                    || !hotkey::is_recording_active()
                {
                    return;
                }
                perf_log::append(&format!(
                    "[ctrl] max-duration watchdog (session={session}) — recording exceeded {}m, force-unducking audio (state machine recovers on next hotkey press)",
                    MAX_RECORDING_DURATION.as_secs() / 60,
                ));
                audio_duck::unduck();
            })
            .ok();
    }
}

/// Treat a Whisper result as suspicious — and skip the injection — when it
/// matches the last successfully-injected text byte-for-byte (or differs only
/// by the smart-space prefix). That's almost always Whisper falling back to a
/// trained-data echo on quiet/garbled input.
fn text_is_suspicious(candidate: &str, last_injected: &Mutex<Option<LastInjection>>) -> bool {
    let last = last_injected.lock();
    let Some(prev) = last.as_ref() else {
        return false;
    };
    // Only a RECENT identical injection is treated as a Whisper echo. The
    // same phrase said again after the echo window — or a match against the
    // DB-seeded value from a previous session (`at` is None) — is accepted
    // as intentional, so repeating "okay"/"yes"/"next" works.
    let recent = prev
        .at
        .map(|t| t.elapsed() < DUPLICATE_ECHO_WINDOW)
        .unwrap_or(false);
    if !recent {
        return false;
    }
    let cand_norm = candidate.trim();
    let prev_norm = prev.text.trim();
    if cand_norm.is_empty() || prev_norm.is_empty() {
        return false;
    }
    cand_norm == prev_norm
}

/// Transcribe, splitting very long recordings into sequential chunks so a
/// late failure doesn't discard everything and each piece completes on its
/// own. Short recordings take the original single-shot path.
fn transcribe_maybe_chunked(
    transcriber: &Transcriber,
    samples_16k: &[f32],
    initial_prompt: Option<&str>,
    accuracy: bool,
) -> Result<String, String> {
    let total_seconds = samples_16k.len() as f64 / 16_000.0;
    if total_seconds <= CHUNK_THRESHOLD_SECONDS {
        return transcriber.transcribe(samples_16k, initial_prompt, accuracy);
    }

    perf_log::append(&format!(
        "[ctrl] long recording ({total_seconds:.1}s) — transcribing in chunks"
    ));
    let boundaries = chunk_boundaries(samples_16k, 16_000, CHUNK_TARGET_SECONDS);
    let mut pieces: Vec<String> = Vec::new();
    let mut ok_count = 0usize;
    let mut last_err: Option<String> = None;
    let mut start = 0usize;
    for (i, end) in boundaries.iter().enumerate() {
        let seg = &samples_16k[start..*end];
        match transcriber.transcribe(seg, initial_prompt, accuracy) {
            Ok(t) => {
                ok_count += 1;
                let t = t.trim();
                if !t.is_empty() {
                    pieces.push(t.to_string());
                }
            }
            Err(e) => {
                // A failed chunk mustn't discard the whole recording — keep
                // the pieces we already have and log the gap.
                perf_log::append(&format!(
                    "[ctrl] chunk {i} failed: {e} — keeping {} prior chunk(s)",
                    pieces.len()
                ));
                last_err = Some(e);
            }
        }
        start = *end;
    }
    // If EVERY chunk errored, surface the failure rather than pretending the
    // audio was silent ("no speech").
    if ok_count == 0 {
        if let Some(e) = last_err {
            return Err(e);
        }
    }
    Ok(pieces.join(" "))
}

/// Compute chunk end-indices for a long recording, nudging each cut to the
/// quietest 100 ms within ±2 s of the nominal boundary so we split on a pause
/// rather than mid-word. The final boundary is always the sample count.
fn chunk_boundaries(samples: &[f32], sample_rate: usize, target_seconds: f64) -> Vec<usize> {
    let n = samples.len();
    let target = (target_seconds * sample_rate as f64) as usize;
    let search = 2 * sample_rate; // ±2 s
    let win = (sample_rate / 10).max(1); // 100 ms energy window
    let mut bounds = Vec::new();
    let mut pos = 0usize;
    while pos + target < n {
        let nominal = pos + target;
        let lo = nominal.saturating_sub(search).max(pos + win);
        let hi = (nominal + search).min(n.saturating_sub(win));
        let mut best = nominal.min(n);
        let mut best_rms = f32::MAX;
        let mut i = lo;
        while i < hi {
            let block = &samples[i..(i + win).min(n)];
            let rms =
                (block.iter().map(|&s| s * s).sum::<f32>() / block.len().max(1) as f32).sqrt();
            if rms < best_rms {
                best_rms = rms;
                best = i;
            }
            i += win;
        }
        if best <= pos {
            best = nominal.min(n);
        }
        bounds.push(best);
        pos = best;
    }
    bounds.push(n);
    bounds
}

/// Returns true if at least `VAD_MIN_SPEECH_CHUNKS` 100 ms chunks of the
/// captured audio have RMS above `threshold` — i.e., contains a meaningful
/// amount of real speech rather than a single throat-clear.
fn has_speech(samples: &[f32], sample_rate: u32, threshold: f32) -> bool {
    if samples.is_empty() {
        return false;
    }
    let chunk = (sample_rate as usize / 10).max(1); // 100 ms
    let speech_chunks = samples
        .chunks(chunk)
        .filter(|block| {
            let sum_sq: f32 = block.iter().map(|&s| s * s).sum();
            let rms = (sum_sq / block.len() as f32).sqrt();
            rms > threshold
        })
        .count();
    speech_chunks >= VAD_MIN_SPEECH_CHUNKS
}

/// Loudest 100ms-chunk RMS in the recording. Used in the VAD-bail log so
/// users hitting silent failures can see whether their mic is just below
/// threshold (informs whether to lower it further).
fn peak_chunk_rms(samples: &[f32], sample_rate: u32) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let chunk = (sample_rate as usize / 10).max(1);
    samples
        .chunks(chunk)
        .map(|b| (b.iter().map(|&s| s * s).sum::<f32>() / b.len() as f32).sqrt())
        .fold(0.0_f32, f32::max)
}

fn position_hud_bottom_center(hud: &tauri::WebviewWindow) -> Result<(), String> {
    let monitor = hud
        .current_monitor()
        .map_err(|e| e.to_string())?
        .or(hud.primary_monitor().map_err(|e| e.to_string())?)
        .ok_or_else(|| "no monitor available".to_string())?;
    let monitor_size = monitor.size();
    let monitor_pos = monitor.position();
    let scale = monitor.scale_factor();

    let win_size = hud.outer_size().map_err(|e| e.to_string())?;

    let x = monitor_pos.x + ((monitor_size.width as i32 - win_size.width as i32) / 2);
    let y = monitor_pos.y + monitor_size.height as i32
        - win_size.height as i32
        - (HUD_BOTTOM_MARGIN_PX as f64 * scale) as i32;

    hud.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Exclude (or re-include) the HUD window from screen capture. On Windows
/// this uses `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` so OBS /
/// Teams / any capture sees a hole where the pill is. Best-effort — logs and
/// moves on if the HWND isn't available or the OS is too old (pre-Win10 2004).
#[cfg(windows)]
fn apply_capture_exclusion(hud: &tauri::WebviewWindow, exclude: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
    };
    match hud.hwnd() {
        Ok(hwnd) => unsafe {
            let affinity = if exclude { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE };
            if SetWindowDisplayAffinity(hwnd.0 as _, affinity) == 0 {
                perf_log::append("[hud] SetWindowDisplayAffinity failed (pre-Win10 2004?)");
            }
        },
        Err(e) => perf_log::append(&format!("[hud] hwnd() unavailable for capture exclusion: {e}")),
    }
}

#[cfg(not(windows))]
fn apply_capture_exclusion(_hud: &tauri::WebviewWindow, _exclude: bool) {
    // macOS equivalent (NSWindow.sharingType = .none) would go here; the HUD
    // is a transparent overlay and is a lower priority on macOS.
}

/// Build the HUD window from scratch with the same config as
/// `tauri.conf.json`'s `[windows]` entry. Used when the initial
/// startup creation race lost — typically Windows auto-start where the
/// post-boot WebView2 init wasn't ready when Tauri's setup() ran.
///
/// Keep this in sync with the `"label": "hud"` block in tauri.conf.json.
/// We can't read that JSON back at runtime (it's compiled away), so the
/// two are mirrors of each other.
fn create_hud_window(app: &AppHandle) -> Result<WebviewWindow, String> {
    let win = WebviewWindowBuilder::new(app, "hud", WebviewUrl::App("hud.html".into()))
        .title("Murmr HUD")
        .inner_size(380.0, 76.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible(false)
        .focused(false)
        .shadow(false)
        // NOTE: do NOT set `additional_browser_args` here. WebView2 uses a
        // single shared browser environment per process — giving one window
        // different args than the others makes environment creation fail and
        // the webview never initializes (shipped as v0.1.65, reverted in
        // v0.1.66: the HUD stopped appearing entirely and a restart no longer
        // helped). If we ever need custom args they must be identical for
        // EVERY window in the app.
        .build()
        .map_err(|e| format!("WebviewWindowBuilder build failed: {e}"))?;
    perf_log::append("[hud] recreated successfully");
    Ok(win)
}

/// True when at least one corner of the HUD window overlaps a connected
/// monitor's physical bounds. Used as a sanity check before we conclude
/// the HUD was successfully shown — `show()` returning Ok and
/// `is_visible() == true` aren't sufficient if the window's position is
/// off the desktop entirely (multi-monitor unplug, UIA returning bad
/// coords for a fullscreen game's "focused element," etc).
fn hud_within_any_monitor(hud: &tauri::WebviewWindow) -> bool {
    let pos = match hud.outer_position() {
        Ok(p) => p,
        Err(e) => {
            perf_log::append(&format!("[hud] outer_position query failed: {e}"));
            return true; // can't verify — give the benefit of the doubt
        }
    };
    let size = match hud.outer_size() {
        Ok(s) => s,
        Err(e) => {
            perf_log::append(&format!("[hud] outer_size query failed: {e}"));
            return true;
        }
    };
    let monitors = match hud.available_monitors() {
        Ok(m) => m,
        Err(e) => {
            perf_log::append(&format!("[hud] available_monitors query failed: {e}"));
            return true;
        }
    };
    if monitors.is_empty() {
        return true; // headless / weird setup — don't second-guess
    }
    let win_left = pos.x;
    let win_top = pos.y;
    let win_right = pos.x + size.width as i32;
    let win_bottom = pos.y + size.height as i32;
    monitors.iter().any(|m| {
        let mpos = m.position();
        let msize = m.size();
        let mleft = mpos.x;
        let mtop = mpos.y;
        let mright = mpos.x + msize.width as i32;
        let mbottom = mpos.y + msize.height as i32;
        // AABB overlap test — any pixel of the window inside any pixel of
        // the monitor counts as "visible enough to find."
        win_left < mright && win_right > mleft && win_top < mbottom && win_bottom > mtop
    })
}

/// Inset above the bottom edge of the foreground window where the HUD floats
/// when no caret is available.
const FOREGROUND_BOTTOM_INSET_PX: i32 = 70;

/// Vertical gap between the focused field's bottom edge and the visible top
/// edge of the HUD pill (very small — the user wants it to almost touch).
const FIELD_GAP_PX: i32 = 4;

/// Pixel offset from the HUD window's top edge to where the pill actually
/// renders. Mirrors the `padding-top: 22px` in `hud.css`. Positioning math
/// shifts the WINDOW upward by this amount so the visible pill lands at
/// `field.bottom + FIELD_GAP_PX`.
const PILL_TOP_OFFSET_PX: i32 = 22;

/// Approximate height of the visible pill — used when we have to flip the
/// HUD above the field because there's no room below.
const PILL_HEIGHT_PX: i32 = 40;

/// Place the HUD just below the bottom edge of a focused element rect (from
/// UIA), horizontally centered on its midpoint. Falls upward if there isn't
/// room below.
fn position_hud_below_field(
    hud: &tauri::WebviewWindow,
    field: focus::ScreenRect,
) -> Result<(), String> {
    let win_size = hud.outer_size().map_err(|e| e.to_string())?;
    let hud_w = win_size.width as i32;

    let center_x = field.x + field.width / 2;
    let mut x = center_x - hud_w / 2;
    // Shift the window up so the pill (which sits PILL_TOP_OFFSET_PX inside
    // the window) lands at `field.bottom + FIELD_GAP_PX`.
    let mut y = field.y + field.height + FIELD_GAP_PX - PILL_TOP_OFFSET_PX;

    if let Some(monitor) = hud
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos = m.position();
                let size = m.size();
                center_x >= pos.x
                    && center_x < pos.x + size.width as i32
                    && field.y >= pos.y
                    && field.y < pos.y + size.height as i32
            })
        })
        .or_else(|| hud.primary_monitor().ok().flatten())
    {
        let m_pos = monitor.position();
        let m_size = monitor.size();
        let min_x = m_pos.x;
        let max_x = m_pos.x + m_size.width as i32 - hud_w;
        // Pill's visible bottom = window y + PILL_TOP_OFFSET + PILL_HEIGHT.
        // Allow the window to extend below the screen as long as the pill
        // itself stays visible.
        let max_pill_bottom = m_pos.y + m_size.height as i32 - 8;
        x = x.clamp(min_x, max_x);
        let pill_bottom = y + PILL_TOP_OFFSET_PX + PILL_HEIGHT_PX;
        if pill_bottom > max_pill_bottom {
            // Not enough room below the field — flip the pill above it.
            y = field.y - FIELD_GAP_PX - PILL_HEIGHT_PX - PILL_TOP_OFFSET_PX;
            y = y.max(m_pos.y);
        }
    }

    hud.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Place the HUD near the bottom of the foreground window, horizontally
/// centered on it. Used as the second-best fallback after caret positioning.
fn position_hud_below_window(
    hud: &tauri::WebviewWindow,
    win: focus::ScreenRect,
) -> Result<(), String> {
    let win_size = hud.outer_size().map_err(|e| e.to_string())?;
    let hud_w = win_size.width as i32;
    let hud_h = win_size.height as i32;

    let mut x = win.x + (win.width / 2) - (hud_w / 2);
    let mut y = win.y + win.height - hud_h - FOREGROUND_BOTTOM_INSET_PX;

    if let Some(monitor) = hud
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos = m.position();
                let size = m.size();
                let cx = win.x + win.width / 2;
                let cy = win.y + win.height / 2;
                cx >= pos.x
                    && cx < pos.x + size.width as i32
                    && cy >= pos.y
                    && cy < pos.y + size.height as i32
            })
        })
        .or_else(|| hud.primary_monitor().ok().flatten())
    {
        let m_pos = monitor.position();
        let m_size = monitor.size();
        let min_x = m_pos.x;
        let max_x = m_pos.x + m_size.width as i32 - hud_w;
        let max_y = m_pos.y + m_size.height as i32 - hud_h;
        x = x.clamp(min_x, max_x);
        if y > max_y {
            y = max_y;
        }
    }

    hud.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn position_hud_near_caret(
    hud: &tauri::WebviewWindow,
    caret: focus::ScreenRect,
) -> Result<(), String> {
    let win_size = hud.outer_size().map_err(|e| e.to_string())?;
    let win_w = win_size.width as i32;
    let win_h = win_size.height as i32;

    // Drop the HUD just below the caret, horizontally centered on it.
    let caret_center_x = caret.x + caret.width / 2;
    let mut x = caret_center_x - win_w / 2;
    let mut y = caret.y + caret.height + 14; // 14 px gap below the caret

    // Clamp into the monitor that contains the caret point.
    if let Some(monitor) = hud
        .available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos = m.position();
                let size = m.size();
                let mx0 = pos.x;
                let my0 = pos.y;
                let mx1 = pos.x + size.width as i32;
                let my1 = pos.y + size.height as i32;
                caret_center_x >= mx0 && caret_center_x < mx1 && caret.y >= my0 && caret.y < my1
            })
        })
        .or_else(|| hud.primary_monitor().ok().flatten())
    {
        let m_pos = monitor.position();
        let m_size = monitor.size();
        let min_x = m_pos.x;
        let max_x = m_pos.x + m_size.width as i32 - win_w;
        let max_y = m_pos.y + m_size.height as i32 - win_h;
        x = x.clamp(min_x, max_x);
        if y > max_y {
            // Not enough room below the caret — pop up above it instead.
            y = (caret.y - win_h - 14).max(m_pos.y);
        }
    }

    hud.set_position(PhysicalPosition::new(x, y))
        .map_err(|e| e.to_string())?;
    Ok(())
}
