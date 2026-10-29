//! Command mode: select text, press the command hotkey, speak an instruction
//! ("make this shorter"), and the selection is replaced with Claude's rewrite.
//! With nothing selected, the instruction writes new text at the cursor.
//!
//! A child module of `controller` (so it can use the controller's private
//! state) kept in its own file to stay clear of the dictation pipeline.
//!
//! Safety: every failure (no key, nothing heard, focus moved, copy failed,
//! timeout, API error) ends in a HUD notice and leaves the selection and the
//! clipboard as they were. Text is only pasted after a successful rewrite.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};

use super::{
    current_recording_session, has_speech, next_recording_session, Controller, Status,
    VAD_RMS_THRESHOLD,
};
use crate::audio;
use crate::audio_duck;
use crate::focus;
use crate::hotkey;
use crate::injector;
use crate::llm::{self, anthropic::AnthropicProvider, LlmError, TransformContext};
use crate::perf_log;

/// How long a HUD notice stays up.
const NOTICE_DURATION: Duration = Duration::from_millis(2800);
/// How long to wait for the user to let go of the hotkey's modifiers
/// before synthesizing Ctrl/Cmd+C.
const MODIFIER_RELEASE_WAIT: Duration = Duration::from_millis(1000);

impl Controller {
    /// Start recording a command. Returns false (and shows a notice) if
    /// command mode can't run, e.g. no API key saved.
    pub(super) fn start_command_recording(&self) -> bool {
        match llm::keychain::get_key() {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => {
                perf_log::append("[cmd] command hotkey pressed but no API key saved");
                self.show_hud();
                self.sounds.play_error_beep();
                notice(&self.app, "Command mode needs an Anthropic API key (Preferences → AI cleanup).");
                return false;
            }
        }
        let session = next_recording_session();
        if let Err(e) = self.start_recording() {
            perf_log::append(&format!("[cmd] start_recording failed: {e}"));
            self.sounds.play_error_beep();
            self.show_hud();
            notice(&self.app, &format!("Couldn't start recording: {e}"));
            return false;
        }
        // Open the TLS connection while the user is still talking.
        llm::anthropic::prewarm();
        let target = focus::capture_foreground();
        perf_log::append(&format!(
            "[cmd] command recording started (session={session}) app={:?}",
            target.app_name
        ));
        *self.capture_target.lock() = Some(target);
        self.command_active.store(true, Ordering::Relaxed);
        hotkey::set_recording_active(true);
        self.emit(Status::CommandRecording);
        self.show_hud();
        self.sounds.play_start_click();
        self.spawn_max_duration_watchdog(session);
        self.reemit_command_after_show(session);
        true
    }

    /// Stop recording, transcribe the instruction, run it, paste the result.
    pub(super) fn complete_command(&self) {
        audio_duck::unduck();
        self.emit(Status::CommandThinking);

        let cap = match self.recorder.stop() {
            Ok(Some(c)) => c,
            Ok(None) => {
                self.emit(Status::Cancelled);
                self.hide_hud();
                return;
            }
            Err(e) => {
                perf_log::append(&format!("[cmd] stop failed: {e}"));
                notice(&self.app, "Recording failed — nothing was changed.");
                return;
            }
        };
        if !has_speech(&cap.samples, cap.sample_rate, VAD_RMS_THRESHOLD) {
            perf_log::append("[cmd] VAD rejected command audio");
            notice(&self.app, "Didn't catch a command.");
            return;
        }

        let transcriber = Arc::clone(&self.transcriber);
        let settings = self.settings.get();
        let sounds = Arc::clone(&self.sounds);
        let app = self.app.clone();
        let target = self.capture_target.lock().take().unwrap_or_default();
        let vocabulary = llm::cleanup::vocabulary(&self.db.list_dictionary(None).unwrap_or_default());

        let spawned = std::thread::Builder::new()
            .name("murmr-command".into())
            .spawn(move || {
                let started = Instant::now();
                let fail = |msg: &str| {
                    sounds.play_error_beep();
                    notice(&app, msg);
                };

                // 1. What did the user ask for?
                let spoken = audio::to_whisper_format(&cap.samples, cap.sample_rate, cap.channels)
                    .and_then(|s| transcriber.transcribe(&s, None, settings.accuracy_mode))
                    .map(|t| clean_instruction(&t));
                let spoken = match spoken {
                    Ok(s) if !s.is_empty() => s,
                    Ok(_) => return fail("Didn't catch a command."),
                    Err(e) => {
                        perf_log::append(&format!("[cmd] transcribe failed: {e}"));
                        return fail("Couldn't transcribe the command.");
                    }
                };
                let transcribe_ms = started.elapsed().as_millis();

                // 2. Key (re-checked: it may have been removed mid-recording).
                let key = match llm::keychain::get_key() {
                    Ok(Some(k)) => k,
                    _ => return fail(&error_message(&LlmError::MissingKey)),
                };

                // 3. Back in the window the user started in?
                if let Some(id) = target.window_id {
                    if focus::foreground_window_id() != Some(id) {
                        focus::refocus_window(id);
                        std::thread::sleep(Duration::from_millis(60));
                        if focus::foreground_window_id() != Some(id) {
                            perf_log::append("[cmd] focus changed — aborting");
                            return fail("Focus changed — nothing was changed.");
                        }
                    }
                }

                // 4. Read the selection (needs the user's modifiers up).
                let wait_start = Instant::now();
                while hotkey::any_modifier_held() && wait_start.elapsed() < MODIFIER_RELEASE_WAIT {
                    std::thread::sleep(Duration::from_millis(20));
                }
                if hotkey::any_modifier_held() {
                    return fail("Let go of the hotkey so Murmr can read the selection.");
                }
                let probe_done_at = Instant::now();
                let selection = match injector::copy_selection() {
                    Ok(sel) => sel,
                    Err(e) => {
                        perf_log::append(&format!("[cmd] copy_selection failed: {e}"));
                        return fail(&format!("Couldn't read the selection ({e}) — nothing was changed."));
                    }
                };

                // 5. Ask Claude.
                let llm_start = Instant::now();
                let ctx = TransformContext {
                    app_name: target.app_name.clone(),
                    vocabulary,
                };
                let result = llm::command_mode::run(
                    Arc::new(AnthropicProvider::new(key)),
                    selection.as_deref(),
                    &spoken,
                    ctx,
                );
                let llm_ms = llm_start.elapsed().as_millis();
                perf_log::append(&format!(
                    "[cmd] ran: selection_chars={} instruction_chars={} transcribe_ms={transcribe_ms} llm_ms={llm_ms} ok={}{}",
                    selection.as_deref().map(|s| s.chars().count()).unwrap_or(0),
                    spoken.chars().count(),
                    result.is_ok(),
                    result.as_ref().err().map(|e| format!(" reason={}", e.tag())).unwrap_or_default(),
                ));
                let out = match result {
                    Ok(t) => t,
                    Err(e) => return fail(&error_message(&e)),
                };

                // 6. Replace the selection (or insert at the cursor). Give
                // the clipboard time to settle after the probe first.
                if let Some(rest) =
                    injector::SELECTION_PROBE_SETTLE.checked_sub(probe_done_at.elapsed())
                {
                    std::thread::sleep(rest);
                }
                // The model call takes seconds: the user may have switched
                // apps. Never paste into a different window — hand the
                // result over on the clipboard instead.
                if let Some(id) = target.window_id {
                    if focus::foreground_window_id() != Some(id) {
                        focus::refocus_window(id);
                        std::thread::sleep(Duration::from_millis(60));
                        if focus::foreground_window_id() != Some(id) {
                            perf_log::append("[cmd] focus changed before paste — result put on clipboard");
                            let copied = arboard::Clipboard::new()
                                .and_then(|mut cb| cb.set_text(out.clone()))
                                .is_ok();
                            return fail(if copied {
                                "Focus changed — the result is on your clipboard instead."
                            } else {
                                "Focus changed — result not inserted."
                            });
                        }
                    }
                }
                let keystroke = settings.injection_mode == "keystroke";
                if let Err(e) = injector::inject_text(&out, keystroke) {
                    perf_log::append(&format!("[cmd] inject failed: {e}"));
                    return fail("Couldn't paste the result.");
                }
                perf_log::append(&format!("[cmd] done total_ms={}", started.elapsed().as_millis()));
                let _ = app.emit(
                    "murmr:status",
                    &Status::Injected {
                        text: out,
                        source_app: target.app_name.clone(),
                    },
                );
                if let Some(hud) = app.get_webview_window("hud") {
                    let _ = hud.hide();
                }
            });
        if spawned.is_err() {
            notice(&self.app, "Command failed — nothing was changed.");
        }
    }

    /// Same idea as `reemit_recording_after_show`, for the command pill.
    fn reemit_command_after_show(&self, session: u64) {
        let app = self.app.clone();
        let _ = std::thread::Builder::new()
            .name("murmr-hud-resync-cmd".into())
            .spawn(move || {
                for delay_ms in [120u64, 500u64] {
                    std::thread::sleep(Duration::from_millis(delay_ms));
                    if current_recording_session() != session || !hotkey::is_recording_active() {
                        return;
                    }
                    let _ = app.emit("murmr:status", Status::CommandRecording);
                }
            });
    }
}

/// Show a short HUD message, then hide the HUD (unless a new recording
/// started in the meantime).
fn notice(app: &AppHandle, message: &str) {
    let _ = app.emit(
        "murmr:status",
        &Status::CommandNotice {
            message: message.to_string(),
        },
    );
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("murmr-cmd-notice".into())
        .spawn(move || {
            std::thread::sleep(NOTICE_DURATION);
            if !hotkey::is_recording_active() {
                if let Some(hud) = app.get_webview_window("hud") {
                    let _ = hud.hide();
                }
            }
        });
}

/// Whisper output → instruction text: trim, drop bracketed non-speech tags
/// like "[BLANK_AUDIO]" or "(silence)".
pub(crate) fn clean_instruction(raw: &str) -> String {
    let re = regex::Regex::new(r"\[[^\]]*\]|\([^)]*\)").expect("static regex");
    re.replace_all(raw, " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// User-facing HUD text for an LLM error.
pub(crate) fn error_message(e: &LlmError) -> String {
    match e {
        LlmError::MissingKey => {
            "Command mode needs an Anthropic API key (Preferences → AI cleanup).".into()
        }
        LlmError::Timeout => "Claude took too long — nothing was changed.".into(),
        LlmError::Http { status: 401, .. } => "Anthropic rejected the API key.".into(),
        LlmError::Http { status: 429 | 529, .. } => "Anthropic is busy — try again.".into(),
        LlmError::Refusal => "Claude declined that request.".into(),
        LlmError::Network(_) => "Can't reach Anthropic — check your connection.".into(),
        LlmError::Rejected(why) if why == "no instruction heard" => "Didn't catch a command.".into(),
        _ => "Command failed — nothing was changed.".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instruction_cleanup() {
        assert_eq!(clean_instruction(" [BLANK_AUDIO] "), "");
        assert_eq!(clean_instruction(" Make this shorter. (coughs)"), "Make this shorter.");
    }

    #[test]
    fn error_messages_are_specific() {
        assert!(error_message(&LlmError::Timeout).contains("nothing was changed"));
        assert!(error_message(&LlmError::Http { status: 401, message: String::new() }).contains("API key"));
        assert!(error_message(&LlmError::MissingKey).contains("Preferences"));
    }
}
