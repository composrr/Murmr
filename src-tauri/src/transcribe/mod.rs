//! Whisper transcription wrapper + post-processing pipeline.

pub mod postprocess;

pub use postprocess::{build_initial_prompt, process, ProcessOutcome};

use std::ffi::{c_char, c_void, CStr};
use std::sync::{Once, OnceLock};
use std::time::Instant;

use regex::Regex;
use whisper_rs::{
    whisper_rs_sys::{ggml_log_level, ggml_log_set, whisper_log_set, whisper_print_system_info},
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters,
};

use crate::perf_log;

// ---------------------------------------------------------------------------
// ggml + whisper log capture
// ---------------------------------------------------------------------------

/// Pipe ggml + whisper's startup logs (CPU feature detection, backend info,
/// model load summary, n_threads) into our perf log so we can see what's
/// actually happening when transcription is slow in production builds.
///
/// Note that ggml and whisper.cpp expose two SEPARATE log callbacks. Hooking
/// `ggml_log_set` only catches CPU/backend init; `whisper_log_set` is what
/// emits model-load and per-call info.
pub fn install_log_hook() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| unsafe {
        ggml_log_set(Some(perf_log_trampoline), std::ptr::null_mut());
        whisper_log_set(Some(perf_log_trampoline_whisper), std::ptr::null_mut());
    });
}

unsafe extern "C" fn perf_log_trampoline(
    _level: ggml_log_level,
    text: *const c_char,
    _user_data: *mut c_void,
) {
    if text.is_null() {
        return;
    }
    let cs = unsafe { CStr::from_ptr(text) };
    let s = cs.to_string_lossy();
    let trimmed = s.trim_end_matches('\n').trim();
    if !trimmed.is_empty() {
        perf_log::append(&format!("[ggml] {trimmed}"));
    }
}

unsafe extern "C" fn perf_log_trampoline_whisper(
    _level: ggml_log_level,
    text: *const c_char,
    _user_data: *mut c_void,
) {
    if text.is_null() {
        return;
    }
    let cs = unsafe { CStr::from_ptr(text) };
    let s = cs.to_string_lossy();
    let trimmed = s.trim_end_matches('\n').trim();
    if !trimmed.is_empty() {
        perf_log::append(&format!("[whisper] {trimmed}"));
    }
}

/// One-shot dump of whisper.cpp's `system_info` string — the SIMD feature
/// flags (AVX/AVX2/F16C/FMA), thread count whisper expects, and any backend
/// info. Called once at startup so we can correlate slow runs with missing
/// CPU features.
pub fn whisper_system_info() -> String {
    unsafe {
        let raw = whisper_print_system_info();
        if raw.is_null() {
            return "(null)".into();
        }
        CStr::from_ptr(raw).to_string_lossy().into_owned()
    }
}

/// Common Whisper hallucinations on near-silence. If the entire output
/// matches one of these (case-insensitive), we treat the result as garbage
/// and return an empty string so the controller skips injection.
const HALLUCINATIONS: &[&str] = &[
    "you",
    "thank you",
    "thank you.",
    "thanks",
    "thanks.",
    "thanks for watching",
    "thanks for watching.",
    "thanks for watching!",
    "thank you for watching",
    "thank you for watching.",
    "thank you for watching!",
    "bye",
    "bye.",
    "bye!",
    "okay",
    "okay.",
    "ok",
    "ok.",
    "hmm",
    "hmm.",
    "...",
    "..",
    ".",
    "♪",
    "[music]",
    "[silence]",
    "(silence)",
];

fn is_hallucination(text: &str) -> bool {
    let trimmed = text.trim().to_lowercase();
    HALLUCINATIONS.contains(&trimmed.as_str())
}

/// Regex matching Whisper's bracketed NON-SPEECH annotations — `[BLANK_AUDIO]`,
/// `[silence]`, `(music)`, `[applause]`, etc. These are transcription
/// artifacts the model emits for silent / non-speech spans, NOT words the user
/// spoke. Deliberately narrow (a fixed vocabulary of known tags) so ordinary
/// parentheticals the user actually dictates — "(see below)", "(John)" — are
/// left untouched.
fn nonspeech_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)[\[(]\s*(?:blank[\s_]*audio|silence|silent|music|sound|no[\s_]*speech|inaudible|applause|laughter|cough(?:ing)?|sniff(?:ing)?|breath(?:ing)?|sigh(?:s|ing)?|wind|static|noise|beep(?:ing)?|typing|clicking|footsteps|background(?:\s+noise)?|pause)\s*[\])]",
        )
        .expect("nonspeech annotation regex is valid")
    })
}

/// Strip Whisper's bracketed non-speech annotations wherever they appear, so a
/// mid-sentence pause never types "[BLANK_AUDIO]" into the user's document.
/// Whitespace left behind is collapsed. If the whole output was one of these
/// tags, the result is empty → the controller treats it as no-speech (a brief
/// HUD hint, nothing injected).
fn strip_nonspeech_annotations(text: &str) -> String {
    let replaced = nonspeech_re().replace_all(text, " ");
    let mut out = String::with_capacity(replaced.len());
    let mut prev_space = false;
    for c in replaced.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    // Tidy stray spaces before punctuation that a removed tag may have left.
    out.trim()
        .replace(" ,", ",")
        .replace(" .", ".")
        .replace(" !", "!")
        .replace(" ?", "?")
        .trim()
        .to_string()
}

pub struct Transcriber {
    ctx: WhisperContext,
}

impl Transcriber {
    pub fn new(model_path: &str) -> Result<Self, String> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| format!("failed to load Whisper model at {model_path}: {e}"))?;
        Ok(Self { ctx })
    }

    /// Transcribe 16kHz mono f32 samples. `initial_prompt` biases Whisper's
    /// vocabulary — pass dictionary Words here so proper nouns survive.
    /// `accuracy_mode` switches greedy decoding for beam search: noticeably
    /// better on jargon/accents at the cost of some speed.
    pub fn transcribe(
        &self,
        samples_16k_mono: &[f32],
        initial_prompt: Option<&str>,
        accuracy_mode: bool,
    ) -> Result<String, String> {
        let t_start = Instant::now();
        let threads = num_cpus().min(8) as i32;
        let audio_seconds = samples_16k_mono.len() as f64 / 16_000.0;

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("failed to create Whisper state: {e}"))?;
        let t_state_ready = Instant::now();

        // Beam search explores multiple hypotheses per step — slower but more
        // robust on hard audio (accents, technical terms). Greedy best_of:1 is
        // the fast default.
        let strategy = if accuracy_mode {
            SamplingStrategy::BeamSearch { beam_size: 5, patience: 1.0 }
        } else {
            SamplingStrategy::Greedy { best_of: 1 }
        };
        let mut params = FullParams::new(strategy);
        params.set_n_threads(threads);
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        if let Some(prompt) = initial_prompt {
            params.set_initial_prompt(prompt);
        }

        state
            .full(params, samples_16k_mono)
            .map_err(|e| format!("Whisper full() failed: {e}"))?;
        let t_full_done = Instant::now();

        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_str_lossy().map_err(|e| format!("segment text: {e}"))?);
        }
        let t_collect_done = Instant::now();

        // Remove Whisper's non-speech annotations (`[BLANK_AUDIO]`, etc.)
        // before anything else sees the text — including a mid-sentence pause
        // that would otherwise litter the transcript.
        let trimmed = strip_nonspeech_annotations(&text);

        // One-line stats per transcribe so the user can share `<app_data>/perf.log`
        // when investigating speed.
        let state_ms = t_state_ready.duration_since(t_start).as_millis();
        let full_ms = t_full_done.duration_since(t_state_ready).as_millis();
        let collect_ms = t_collect_done.duration_since(t_full_done).as_millis();
        let total_ms = t_collect_done.duration_since(t_start).as_millis();
        let realtime_ratio = if audio_seconds > 0.0 {
            (total_ms as f64) / 1000.0 / audio_seconds
        } else {
            0.0
        };
        perf_log::append(&format!(
            "[transcribe] audio={audio_seconds:.2}s threads={threads} accuracy={accuracy_mode} state={state_ms}ms full={full_ms}ms collect={collect_ms}ms total={total_ms}ms ratio={realtime_ratio:.2}x"
        ));

        // Empty after stripping (pure silence / blank audio) or a known
        // whole-output hallucination → inject nothing. The controller turns an
        // empty result into the brief "Didn't catch that" HUD hint.
        if trimmed.is_empty() || is_hallucination(&trimmed) {
            if !text.trim().is_empty() {
                perf_log::append(&format!(
                    "[whisper] dropped non-speech/hallucination output: {:?}",
                    text.trim()
                ));
            }
            return Ok(String::new());
        }
        Ok(trimmed)
    }
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
