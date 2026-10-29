//! Whisper transcription wrapper + post-processing pipeline.

pub mod postprocess;
// ---- Engine (GPU / hot-swap / VAD / model download) — session A ----
pub mod engine;
pub mod gpu;
pub mod models;
pub mod bench;
// ---- end engine block ----
// ---- Live preview ----
pub mod partial;
// ---- end live preview ----

pub use postprocess::{build_initial_prompt, process, ProcessOutcome};

use std::ffi::{c_char, c_void, CStr};
use std::path::Path;
use std::sync::{Once, OnceLock};
use std::time::Instant;

use parking_lot::Mutex;
use regex::Regex;
use whisper_rs::{
    whisper_rs_sys::{ggml_log_level, ggml_log_set, whisper_log_set, whisper_print_system_info},
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext,
    WhisperVadContextParams, WhisperVadParams,
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

/// Which compute device a loaded model runs on. Shown in Settings → Advanced
/// and written to perf.log.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct BackendInfo {
    /// "Vulkan" | "Metal" | "CPU"
    pub kind: String,
    /// GPU name as the driver reports it, e.g. "NVIDIA GeForce RTX 3080 Ti".
    pub device: Option<String>,
}

impl BackendInfo {
    pub fn cpu() -> Self {
        Self { kind: "CPU".into(), device: None }
    }
    pub fn is_gpu(&self) -> bool {
        self.kind != "CPU"
    }
    pub fn label(&self) -> String {
        match &self.device {
            Some(d) => format!("{} ({d})", self.kind),
            None => self.kind.clone(),
        }
    }
}

/// Whisper's prompt window is n_text_ctx/2 = 224 tokens and whisper.cpp keeps
/// only the LAST tokens when a prompt is longer. Stay under it with margin.
const PROMPT_TOKEN_BUDGET: usize = 200;

pub struct Transcriber {
    ctx: WhisperContext,
    backend: BackendInfo,
    model_name: String,
    /// Silero VAD, loaded once per model. `None` when the VAD model file is
    /// missing, failed to load, or VAD is switched off — transcription then
    /// runs on the full audio, exactly as before VAD existed.
    vad: Option<Mutex<WhisperVadContext>>,
}

impl Transcriber {
    /// Load a model. `use_gpu` asks whisper.cpp for the first GPU device
    /// (Vulkan on Windows, Metal on Apple Silicon); with no usable device it
    /// stays on CPU, and `backend()` reports what was actually used.
    pub fn new(model_path: &str, use_gpu: bool, vad_model: Option<&Path>) -> Result<Self, String> {
        // A model load initializes ggml's backend registry, which loads the
        // GPU driver even for a CPU model (unless drivers were hidden). Guard
        // it so a driver crash here can't become a crash loop on every launch.
        let _guard = gpu::driver_code_may_run().then(gpu::InflightGuard::begin);
        let gpu_dev = if use_gpu { gpu::first_gpu_device() } else { None };
        let mut cparams = WhisperContextParameters::default();
        cparams.use_gpu(gpu_dev.is_some());
        if let Some((_, index)) = &gpu_dev {
            cparams.gpu_device(*index);
        }
        // Flash attention is a clear win on GPU backends; keep the CPU path
        // exactly as it was.
        cparams.flash_attn(gpu_dev.is_some());
        let ctx = WhisperContext::new_with_params(model_path, cparams)
            .map_err(|e| format!("failed to load Whisper model at {model_path}: {e}"))?;
        drop(_guard);
        let backend = match gpu_dev {
            Some((dev, _)) => BackendInfo { kind: gpu::backend_kind().into(), device: Some(dev) },
            None => BackendInfo::cpu(),
        };
        let vad = vad_model.and_then(|p| {
            let mut vp = WhisperVadContextParams::default();
            vp.set_n_threads(2);
            vp.set_use_gpu(false); // tiny model; CPU beats a GPU round-trip
            match WhisperVadContext::new(&p.to_string_lossy(), vp) {
                Ok(v) => Some(Mutex::new(v)),
                Err(e) => {
                    perf_log::append(&format!("[vad] failed to load {p:?}: {e} — VAD off"));
                    None
                }
            }
        });
        let model_name = Path::new(model_path)
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| model_path.to_string());
        Ok(Self { ctx, backend, model_name, vad })
    }

    pub fn backend(&self) -> &BackendInfo {
        &self.backend
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn has_vad(&self) -> bool {
        self.vad.is_some()
    }

    /// Number of Whisper tokens in `text` (for prompt budgeting).
    fn token_count(&self, text: &str) -> usize {
        // max_tokens must be >= the real count: whisper-rs mis-handles the
        // "too many tokens" return. A token is never shorter than one byte.
        self.ctx
            .tokenize(text, text.len() + 8)
            .map(|t| t.len())
            .unwrap_or(text.len() / 3)
    }

    /// Build the initial prompt for the next chunk of a long recording:
    /// dictionary words (proper-noun bias) followed by the tail of the
    /// previous chunk's text (continuity: casing, punctuation style, the
    /// sentence that was cut mid-way). Kept within Whisper's prompt window so
    /// the dictionary words aren't truncated away.
    pub fn compose_prompt(&self, dictionary_prompt: Option<&str>, prev_text: &str) -> Option<String> {
        let dict = dictionary_prompt.map(str::trim).filter(|s| !s.is_empty());
        let prev = prev_text.trim();
        if prev.is_empty() || looks_repetitive(prev) {
            return dict.map(str::to_string);
        }
        let dict_tokens = dict.map(|d| self.token_count(d)).unwrap_or(0);
        let tail_budget = PROMPT_TOKEN_BUDGET.saturating_sub(dict_tokens).max(48);
        let words: Vec<&str> = prev.split_whitespace().collect();
        // ~1.3 tokens/word in English; start there and shrink until it fits.
        let mut take = ((tail_budget as f32) / 1.3) as usize;
        take = take.clamp(1, words.len());
        let tail = loop {
            let candidate = words[words.len() - take..].join(" ");
            if take <= 1 || self.token_count(&candidate) <= tail_budget {
                break candidate;
            }
            take = take * 4 / 5;
        };
        Some(match dict {
            Some(d) => format!("{d}. {tail}"),
            None => tail,
        })
    }

    /// Run Silero VAD and return audio with the silent stretches removed
    /// (speech segments joined by short gaps). `None` means "use the original
    /// audio": VAD unavailable, failed, found no speech (we never drop a
    /// dictation on VAD's word alone), or would barely trim anything.
    fn vad_filter(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let vad = self.vad.as_ref()?;
        let t0 = Instant::now();
        let mut params = WhisperVadParams::new();
        // Conservative settings for dictation: a lower threshold and generous
        // padding so soft word onsets/endings are never clipped.
        params.set_threshold(0.35);
        params.set_min_speech_duration(200);
        params.set_min_silence_duration(500);
        params.set_speech_pad(250);
        let segments = match vad.lock().segments_from_samples(params, samples) {
            Ok(s) => s,
            Err(e) => {
                perf_log::append(&format!("[vad] failed: {e} — using full audio"));
                return None;
            }
        };
        let n = samples.len();
        let gap = vec![0.0f32; 16_000 / 5]; // 200 ms between kept segments
        let mut out: Vec<f32> = Vec::with_capacity(n);
        let mut count = 0usize;
        for seg in segments {
            // Timestamps are centiseconds → 160 samples each at 16 kHz.
            let a = ((seg.start.max(0.0) as f64) * 160.0) as usize;
            let b = ((seg.end.max(0.0) as f64) * 160.0) as usize;
            let (a, b) = (a.min(n), b.min(n));
            if b <= a {
                continue;
            }
            if count > 0 {
                out.extend_from_slice(&gap);
            }
            out.extend_from_slice(&samples[a..b]);
            count += 1;
        }
        let ms = t0.elapsed().as_millis();
        if count == 0 {
            perf_log::append(&format!(
                "[vad] no speech segments found ({ms}ms) — using full audio (energy gate already passed)"
            ));
            return None;
        }
        let kept = out.len() as f64 / n.max(1) as f64;
        perf_log::append(&format!(
            "[vad] {count} segment(s), kept {:.0}% of {:.2}s in {ms}ms",
            kept * 100.0,
            n as f64 / 16_000.0
        ));
        // Not worth a different input if it trims under 5%.
        if kept > 0.95 {
            return None;
        }
        // Whisper misbehaves on sub-second input; pad very short results.
        if out.len() < 16_000 {
            out.resize(16_000, 0.0);
        }
        Some(out)
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
        // On GPU most work is off the CPU; a few threads cover the remaining
        // CPU ops and keep the machine responsive.
        let threads = if self.backend.is_gpu() { 4 } else { num_cpus().min(8) as i32 };
        let audio_seconds = samples_16k_mono.len() as f64 / 16_000.0;

        let filtered = self.vad_filter(samples_16k_mono);
        let samples: &[f32] = filtered.as_deref().unwrap_or(samples_16k_mono);
        let t_vad_done = Instant::now();

        // Crash guard: if the GPU driver takes the process down mid-decode,
        // the marker survives and the next launch falls back to CPU.
        let _gpu_guard = self.backend.is_gpu().then(gpu::InflightGuard::begin);

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
            .full(params, samples)
            .map_err(|e| format!("Whisper full() failed: {e}"))?;
        let t_full_done = Instant::now();

        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(&segment.to_str_lossy().map_err(|e| format!("segment text: {e}"))?);
        }
        let t_collect_done = Instant::now();
        drop(state);

        // Remove Whisper's non-speech annotations (`[BLANK_AUDIO]`, etc.)
        // before anything else sees the text — including a mid-sentence pause
        // that would otherwise litter the transcript.
        let trimmed = strip_nonspeech_annotations(&text);

        // One-line stats per transcribe so the user can share `<app_data>/perf.log`
        // when investigating speed.
        let vad_ms = t_vad_done.duration_since(t_start).as_millis();
        let state_ms = t_state_ready.duration_since(t_vad_done).as_millis();
        let full_ms = t_full_done.duration_since(t_state_ready).as_millis();
        let collect_ms = t_collect_done.duration_since(t_full_done).as_millis();
        let total_ms = t_collect_done.duration_since(t_start).as_millis();
        let realtime_ratio = if audio_seconds > 0.0 {
            (total_ms as f64) / 1000.0 / audio_seconds
        } else {
            0.0
        };
        perf_log::append(&format!(
            "[transcribe] model={} backend={} audio={audio_seconds:.2}s decoded={:.2}s threads={threads} accuracy={accuracy_mode} vad={vad_ms}ms state={state_ms}ms full={full_ms}ms collect={collect_ms}ms total={total_ms}ms ratio={realtime_ratio:.2}x",
            self.model_name,
            self.backend.kind,
            samples.len() as f64 / 16_000.0,
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

/// True when the text is dominated by a repeated phrase — Whisper's classic
/// failure loop ("thank you thank you thank you …"). Such text must not be
/// carried into the next chunk's prompt, or the loop spreads.
fn looks_repetitive(text: &str) -> bool {
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    for n in 1..=4 {
        if words.len() < n * 4 {
            break;
        }
        // Any phrase of n words repeated 4+ times back-to-back.
        let mut i = 0;
        while i + 2 * n <= words.len() {
            let mut run = 1;
            let mut j = i + n;
            while j + n <= words.len() && words[j..j + n] == words[i..i + n] {
                run += 1;
                j += n;
            }
            if run >= 4 {
                return true;
            }
            i += 1;
        }
    }
    false
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

#[cfg(test)]
mod engine_tests {
    use super::looks_repetitive;

    #[test]
    fn detects_loops() {
        assert!(looks_repetitive("thank you thank you thank you thank you thank you"));
        assert!(looks_repetitive("so so so so so so"));
        assert!(!looks_repetitive(
            "the quick brown fox jumps over the lazy dog and keeps on running"
        ));
        assert!(!looks_repetitive("short text"));
        assert!(!looks_repetitive("no, no, no. I said yes"));
    }
}
