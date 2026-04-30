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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::Receiver;
use parking_lot::Mutex;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition};

use crate::audio::{self, Recorder};
use crate::audio_duck;
use crate::db::Db;
use crate::focus;
use crate::hotkey::HotkeyEvent;
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
    Error { message: String },
}

pub struct Controller {
    recorder: Arc<Recorder>,
    transcriber: Arc<Transcriber>,
    db: Arc<Db>,
    settings: Arc<SettingsStore>,
    sounds: Arc<SoundPlayer>,
    app: AppHandle,
    /// Last text we successfully injected. Used by the re-paste hotkey.
    last_injected: Arc<Mutex<Option<String>>>,
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
        let seed = db.recent_transcriptions(1).ok().and_then(|v| v.into_iter().next()).map(|t| t.text);
        Self {
            recorder: Arc::new(Recorder::new()),
            transcriber,
            db,
            settings,
            sounds,
            app,
            last_injected: Arc::new(Mutex::new(seed)),
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
                (HotkeyEvent::DictationDown, RecState::Idle) => {
                    if let Err(e) = self.start_recording() {
                        self.sounds.play_error_beep();
                        self.emit(Status::Error {
                            message: format!("failed to start recording: {e}"),
                        });
                        continue;
                    }
                    state = RecState::HoldUncertain;
                    press_at = Some(Instant::now());
                    self.emit(Status::Recording);
                    self.show_hud();
                    self.sounds.play_start_click();
                }

                (HotkeyEvent::DictationDown, RecState::Toggled) => {
                    state = RecState::Idle;
                    press_at = None;
                    self.complete_recording();
                }

                (HotkeyEvent::DictationUp, RecState::HoldUncertain) => {
                    let elapsed = press_at.map(|t| t.elapsed()).unwrap_or_default();
                    let tap_threshold_ms =
                        self.settings.get().tap_threshold_ms.max(MIN_TAP_THRESHOLD_MS);
                    let tap_threshold = Duration::from_millis(tap_threshold_ms as u64);
                    if elapsed >= tap_threshold {
                        state = RecState::Idle;
                        press_at = None;
                        self.complete_recording();
                    } else {
                        state = RecState::Toggled;
                    }
                }
                (HotkeyEvent::DictationUp, _) => {}

                (HotkeyEvent::EscDown, RecState::HoldUncertain | RecState::Toggled) => {
                    let _ = self.recorder.cancel();
                    audio_duck::unduck();
                    state = RecState::Idle;
                    press_at = None;
                    self.emit(Status::Cancelled);
                    self.hide_hud();
                }

                (HotkeyEvent::EscDown, RecState::Idle) => {}
                (HotkeyEvent::DictationDown, RecState::HoldUncertain) => {}

                // Re-paste the most recent transcription. Ignored mid-recording
                // (would interfere with the active dictation flow).
                (HotkeyEvent::RepeatLast, RecState::Idle) => {
                    self.repaste_last();
                }
                (HotkeyEvent::RepeatLast, _) => {}
            }
        }
    }

    fn repaste_last(&self) {
        let text = match self.last_injected.lock().clone() {
            Some(t) if !t.is_empty() => t,
            _ => {
                self.emit(Status::Cancelled);
                return;
            }
        };
        let app = self.app.clone();
        std::thread::Builder::new()
            .name("murmr-repaste".into())
            .spawn(move || {
                if let Err(e) = injector::inject_text(&text) {
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
                self.emit(Status::Cancelled);
                self.hide_hud();
                return;
            }
            Err(e) => {
                self.emit(Status::Error {
                    message: format!("stop failed: {e}"),
                });
                self.hide_hud();
                return;
            }
        };

        // Energy-based VAD — bail out before invoking Whisper on silence.
        if !has_speech(&cap.samples, cap.sample_rate, VAD_RMS_THRESHOLD) {
            // Log the peak chunk RMS so users hitting silent failures can
            // see whether their mic is producing audio that's just below
            // threshold — informs the next round of threshold tuning.
            let peak = peak_chunk_rms(&cap.samples, cap.sample_rate);
            perf_log::append(&format!(
                "[ctrl] VAD rejected: {} samples, peak chunk RMS {:.4} (threshold {:.4})",
                cap.samples.len(),
                peak,
                VAD_RMS_THRESHOLD
            ));
            self.emit(Status::Cancelled);
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

                let result = (|| -> Result<String, String> {
                    let samples_16k =
                        audio::to_whisper_format(&cap.samples, cap.sample_rate, cap.channels)?;
                    transcriber.transcribe(&samples_16k, initial_prompt.as_deref())
                })();

                let result = result.map(|raw| transcribe::process(&raw, &settings, &dictionary));

                // Extract text + stripped fillers so the rest of the match
                // arms only deal with `String` like before.
                let (result, stripped_fillers) = match result {
                    Ok(outcome) => (Ok(outcome.text), outcome.stripped_fillers),
                    Err(e) => (Err(e), Default::default()),
                };

                match result {
                    Ok(text) if text.is_empty() => {
                        let _ = app.emit("murmr:status", &Status::Cancelled);
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
                        if practice_mode.load(Ordering::Relaxed) {
                            sounds.play_complete_chime();
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

                        if let Err(e) = injector::inject_text(&text) {
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

                        let word_count = text.split_whitespace().count() as i64;
                        match db.insert_transcription(&text, word_count, duration_ms, None) {
                            Ok(_) => {
                                let now_ms = std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as i64)
                                    .unwrap_or(0);
                                if let Err(e) = db.bump_streak_day(now_ms) {
                                    eprintln!("[db] failed to bump streak day: {e}");
                                }
                                if !stripped_fillers.is_empty() {
                                    let entries: Vec<(String, i64)> =
                                        stripped_fillers.iter().map(|(k, v)| (k.clone(), *v)).collect();
                                    if let Err(e) = db.bump_fillers(&entries) {
                                        eprintln!("[db] failed to bump fillers: {e}");
                                    }
                                }
                                let _ = app.emit("murmr:transcription-saved", &());
                            }
                            Err(e) => eprintln!("[db] failed to write transcription: {e}"),
                        }

                        *last_injected.lock() = Some(text.clone());

                        sounds.play_complete_chime();
                        let _ = app.emit(
                            "murmr:status",
                            &Status::Injected {
                                text,
                                source_app: None,
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
        let Some(hud) = self.app.get_webview_window("hud") else {
            return;
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

        eprintln!("[hud] positioned via {}", placement.unwrap_or("(failed)"));

        let _ = hud.show();
        let _ = hud.set_always_on_top(true);
    }

    fn hide_hud(&self) {
        if let Some(hud) = self.app.get_webview_window("hud") {
            let _ = hud.hide();
        }
    }
}

/// Treat a Whisper result as suspicious — and skip the injection — when it
/// matches the last successfully-injected text byte-for-byte (or differs only
/// by the smart-space prefix). That's almost always Whisper falling back to a
/// trained-data echo on quiet/garbled input.
fn text_is_suspicious(candidate: &str, last_injected: &Mutex<Option<String>>) -> bool {
    let last = last_injected.lock();
    let Some(prev) = last.as_ref() else {
        return false;
    };
    let cand_norm = candidate.trim();
    let prev_norm = prev.trim();
    if cand_norm.is_empty() || prev_norm.is_empty() {
        return false;
    }
    cand_norm == prev_norm
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
