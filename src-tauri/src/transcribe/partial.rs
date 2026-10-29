//! Fast, abortable "rough" decodes for the live preview.
//!
//! Differences from [`Transcriber::transcribe`] (the accurate final pass):
//! - one reusable `WhisperState` per preview session (no per-tick allocation);
//! - greedy, single segment, no timestamps, no temperature fallback retries;
//! - optional reduced encoder context (`audio_ctx`) for short windows — the
//!   main lever that makes CPU previews affordable;
//! - prompt passed as tokens (whisper-rs leaks a CString per
//!   `set_initial_prompt`, which would add up at one decode per second);
//! - abortable: [`abort_all`] makes any in-flight partial stop at the next
//!   encoder/decoder step, so the final pass never waits on it;
//! - no VAD, no perf.log line per call (the preview logs a summary).

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use whisper_rs::{FullParams, SamplingStrategy, WhisperState};

use super::{gpu, is_hallucination, num_cpus, strip_nonspeech_annotations, Transcriber};

/// Bumped by [`abort_all`]; a decode aborts when this no longer matches the
/// generation it started with. No allocation, nothing leaked per decode.
static GENERATION: AtomicU64 = AtomicU64::new(1);

unsafe extern "C" fn abort_if_stale(user_data: *mut c_void) -> bool {
    GENERATION.load(Ordering::Relaxed) != user_data as usize as u64
}

/// Abort every in-flight partial decode (called when recording stops).
pub fn abort_all() {
    GENERATION.fetch_add(1, Ordering::SeqCst);
}

/// Current generation; a preview session captures it at start.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::SeqCst)
}

pub struct PartialDecoder {
    state: WhisperState,
}

pub enum PartialResult {
    Text(String),
    Aborted,
}

impl Transcriber {
    pub fn partial_decoder(&self) -> Result<PartialDecoder, String> {
        self.ctx
            .create_state()
            .map(|state| PartialDecoder { state })
            .map_err(|e| format!("preview state: {e}"))
    }
}

impl PartialDecoder {
    /// Decode `samples` (16 kHz mono). `generation` comes from
    /// [`generation`] at session start. `shrink_ctx` trims the encoder to the
    /// window length (faster, slightly rougher).
    pub fn decode(
        &mut self,
        t: &Transcriber,
        samples: &[f32],
        prompt: Option<&str>,
        generation: u64,
        shrink_ctx: bool,
        cpu_threads: i32,
    ) -> Result<PartialResult, String> {
        let on_gpu = t.backend().is_gpu();
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(if on_gpu { 4 } else { cpu_threads.clamp(1, num_cpus() as i32) });
        params.set_translate(false);
        params.set_language(Some("en"));
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_single_segment(true);
        params.set_no_timestamps(true);
        params.set_temperature_inc(0.0);
        params.set_suppress_blank(true);
        let tokens = prompt
            .filter(|p| !p.trim().is_empty())
            .and_then(|p| t.ctx.tokenize(p, p.len() + 8).ok());
        if let Some(tok) = tokens.as_deref() {
            params.set_tokens(tok);
        }
        if shrink_ctx {
            // 50 encoder frames per second of audio, plus headroom.
            let secs = samples.len() as f64 / 16_000.0;
            let ctx = ((secs * 50.0).ceil() as i32 + 64).min(1500);
            params.set_audio_ctx(ctx);
        }
        unsafe {
            params.set_abort_callback(Some(abort_if_stale));
            params.set_abort_callback_user_data(generation as usize as *mut c_void);
        }

        let _gpu_guard = on_gpu.then(gpu::InflightGuard::begin);
        let started_gen = generation;
        match self.state.full(params, samples) {
            Ok(()) => {}
            Err(_) if GENERATION.load(Ordering::SeqCst) != started_gen => {
                return Ok(PartialResult::Aborted)
            }
            Err(e) => return Err(format!("partial decode: {e}")),
        }
        if GENERATION.load(Ordering::SeqCst) != started_gen {
            return Ok(PartialResult::Aborted);
        }
        let mut text = String::new();
        for seg in self.state.as_iter() {
            if let Ok(s) = seg.to_str_lossy() {
                text.push_str(&s);
            }
        }
        let text = strip_nonspeech_annotations(&text);
        if is_hallucination(&text) {
            return Ok(PartialResult::Text(String::new()));
        }
        Ok(PartialResult::Text(text))
    }
}

/// Time one decode (bench helper).
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, u128) {
    let t0 = Instant::now();
    let r = f();
    (r, t0.elapsed().as_millis())
}
