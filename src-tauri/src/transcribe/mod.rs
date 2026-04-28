//! Whisper transcription wrapper + post-processing pipeline.

pub mod postprocess;

pub use postprocess::{build_initial_prompt, process, ProcessOutcome};

use std::ffi::{c_char, c_void, CStr};
use std::sync::Once;
use std::time::Instant;

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
    pub fn transcribe(
        &self,
        samples_16k_mono: &[f32],
        initial_prompt: Option<&str>,
    ) -> Result<String, String> {
        let t_start = Instant::now();
        let threads = num_cpus().min(8) as i32;
        let audio_seconds = samples_16k_mono.len() as f64 / 16_000.0;

        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| format!("failed to create Whisper state: {e}"))?;
        let t_state_ready = Instant::now();

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
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

        let trimmed = text.trim().to_string();

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
            "[transcribe] audio={audio_seconds:.2}s threads={threads} state={state_ms}ms full={full_ms}ms collect={collect_ms}ms total={total_ms}ms ratio={realtime_ratio:.2}x"
        ));

        if is_hallucination(&trimmed) {
            eprintln!("[whisper] discarded suspected hallucination: {trimmed:?}");
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
