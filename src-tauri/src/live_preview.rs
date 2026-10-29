//! Live transcript preview: rough text in the HUD while the user speaks.
//!
//! A worker thread wakes every `interval`, takes the audio captured since the
//! last commit point, and decodes it with a fast, abortable partial decoder
//! (see `transcribe/partial.rs`). Decodes run back to back on one thread, so
//! if one is still running when the next tick is due, that tick is skipped.
//!
//! Stable text: whisper revises its guess as more audio arrives. A word is
//! shown as *stable* once two consecutive decodes agree on it (and everything
//! before it); the rest is *tentative* (drawn dimmer). Stable text only ever
//! grows, so the HUD never visibly jumps backwards.
//!
//! Long takes: when the uncommitted window passes `max_window`, it is cut at
//! the quietest point near 2/3 of the window, that part is decoded once and
//! committed, and later decodes only see the audio after the cut (prompted
//! with the committed text's tail for continuity). Decode cost stays bounded.
//!
//! The preview NEVER feeds the pasted text. When recording stops, the
//! controller calls [`LivePreview::stop`], which aborts any in-flight decode,
//! and the normal full-accuracy pass runs exactly as before.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::audio::{self, Recorder};
use crate::perf_log;
use crate::transcribe::partial::{self, PartialResult};
use crate::transcribe::Transcriber;

/// Where preview audio comes from. The recorder in the app; a WAV played back
/// in real time in the bench.
pub trait PreviewSource: Send + Sync + 'static {
    /// Interleaved samples from raw index `from` to now. `None` = not recording.
    fn peek(&self, from: usize) -> Option<audio::capture::LiveAudio>;
}

impl PreviewSource for Recorder {
    fn peek(&self, from: usize) -> Option<audio::capture::LiveAudio> {
        Recorder::peek(self, from)
    }
}

/// HUD payload, emitted as `murmr:live-preview`.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct PreviewUpdate {
    pub session: u64,
    pub stable: String,
    pub tentative: String,
}

#[derive(Clone, Debug)]
pub struct PreviewConfig {
    pub interval: Duration,
    /// Shrink the encoder to the window length (CPU mode).
    pub shrink_ctx: bool,
    pub cpu_threads: i32,
    /// Commit + slide once the uncommitted window is longer than this.
    pub max_window: Duration,
    /// RMS level that counts as speech (same gate as the final pass).
    pub speech_rms: f32,
}

impl PreviewConfig {
    /// GPU: frequent ticks, full-quality encoder.
    pub fn gpu() -> Self {
        Self {
            interval: Duration::from_millis(600),
            shrink_ctx: false,
            cpu_threads: 4,
            max_window: Duration::from_secs(20),
            speech_rms: 0.015,
        }
    }
    /// CPU: slower ticks, shrunken encoder, few threads, short window.
    pub fn cpu() -> Self {
        Self {
            interval: Duration::from_millis(1500),
            shrink_ctx: true,
            cpu_threads: 4,
            max_window: Duration::from_secs(12),
            speech_rms: 0.015,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct PreviewStats {
    pub decodes: u32,
    pub skipped_ticks: u32,
    pub commits: u32,
    pub total_decode_ms: u128,
    pub max_decode_ms: u128,
    pub aborted: bool,
}

pub struct LivePreview {
    stop: Arc<AtomicBool>,
}

impl LivePreview {
    /// Stop the worker and abort any in-flight decode. Never blocks.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        partial::abort_all();
    }
}

impl Drop for LivePreview {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Start the preview worker. `on_update` is called (on the worker thread)
/// whenever the displayed text changes; `on_done` once with stats.
pub fn start(
    transcriber: Arc<Transcriber>,
    source: Arc<dyn PreviewSource>,
    session: u64,
    dictionary_prompt: Option<String>,
    cfg: PreviewConfig,
    on_update: impl Fn(PreviewUpdate) + Send + 'static,
    on_done: impl FnOnce(PreviewStats) + Send + 'static,
) -> LivePreview {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_w = Arc::clone(&stop);
    let generation = partial::generation();
    let _ = std::thread::Builder::new()
        .name("murmr-live-preview".into())
        .spawn(move || {
            let stats = run(
                &transcriber,
                source.as_ref(),
                session,
                dictionary_prompt.as_deref(),
                &cfg,
                generation,
                &stop_w,
                &on_update,
            );
            on_done(stats);
        });
    LivePreview { stop }
}

#[allow(clippy::too_many_arguments)]
fn run(
    t: &Transcriber,
    source: &dyn PreviewSource,
    session: u64,
    dict_prompt: Option<&str>,
    cfg: &PreviewConfig,
    generation: u64,
    stop: &AtomicBool,
    on_update: &dyn Fn(PreviewUpdate),
) -> PreviewStats {
    let mut stats = PreviewStats::default();
    let mut decoder = match t.partial_decoder() {
        Ok(d) => d,
        Err(e) => {
            perf_log::append(&format!("[preview] disabled for this take: {e}"));
            return stats;
        }
    };
    let mut committed_text = String::new();
    let mut committed_raw: usize = 0; // interleaved sample index
    let mut agree = Agreement::default();
    let mut last_sent = PreviewUpdate::default();
    let start = Instant::now();
    let mut next_tick = start + cfg.interval;

    let stopped = || stop.load(Ordering::SeqCst) || partial::generation() != generation;

    loop {
        // Sleep until the next tick in short steps so stop is noticed fast.
        while Instant::now() < next_tick {
            if stopped() {
                return finish(stats, stop);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        // Ticks that passed while the previous decode ran are skipped.
        let now = Instant::now();
        while next_tick <= now {
            next_tick += cfg.interval;
            stats.skipped_ticks += 1;
        }
        stats.skipped_ticks -= 1; // the tick we're serving now isn't skipped
        if stopped() {
            return finish(stats, stop);
        }

        let Some(live) = source.peek(committed_raw) else {
            return finish(stats, stop);
        };
        let ch = live.channels.max(1) as usize;
        let sr = live.sample_rate.max(1) as usize;
        let Ok(window) = audio::to_whisper_format(&live.samples, live.sample_rate, live.channels)
        else {
            continue;
        };
        if window.len() < 16_000 * 6 / 10 || !has_speech(&window, cfg.speech_rms) {
            continue;
        }

        let max_len = (cfg.max_window.as_secs_f64() * 16_000.0) as usize;
        // Dictionary words only. Feeding the committed text back as a prompt
        // makes whisper repeat it over silence (measured: one sentence came
        // back 20 times on a take with long pauses).
        let prompt = dict_prompt.map(str::to_string);

        if window.len() > max_len {
            // Commit the first part at a pause and slide the window.
            let cut = quiet_cut(&window, max_len * 2 / 3);
            let raw_frames = cut * sr / 16_000;
            if !has_speech(&window[..cut], cfg.speech_rms) {
                // Nothing said in the part we're dropping: slide past it.
                committed_raw += raw_frames * ch;
                agree = Agreement::default();
                continue;
            }
            let head = squeeze_silence(&window[..cut], cfg.speech_rms);
            let (res, ms) = partial::timed(|| {
                decoder.decode(t, &head, prompt.as_deref(), generation, cfg.shrink_ctx, cfg.cpu_threads)
            });
            stats.decodes += 1;
            stats.total_decode_ms += ms;
            stats.max_decode_ms = stats.max_decode_ms.max(ms);
            match res {
                Ok(PartialResult::Text(text)) => {
                    append_text(&mut committed_text, &text);
                    // Also catches a sentence repeated across the cut.
                    committed_text = collapse_repeats(&committed_text);
                    stats.commits += 1;
                    committed_raw += raw_frames * ch;
                    agree = Agreement::default();
                }
                Ok(PartialResult::Aborted) => return finish(stats, stop),
                Err(e) => {
                    perf_log::append(&format!("[preview] {e} — stopping preview for this take"));
                    return finish(stats, stop);
                }
            }
            continue;
        }

        let squeezed = squeeze_silence(&window, cfg.speech_rms);
        let (res, ms) = partial::timed(|| {
            decoder.decode(t, &squeezed, prompt.as_deref(), generation, cfg.shrink_ctx, cfg.cpu_threads)
        });
        stats.decodes += 1;
        stats.total_decode_ms += ms;
        stats.max_decode_ms = stats.max_decode_ms.max(ms);
        let hyp = match res {
            Ok(PartialResult::Text(text)) => collapse_repeats(&text),
            Ok(PartialResult::Aborted) => return finish(stats, stop),
            Err(e) => {
                perf_log::append(&format!("[preview] {e} — stopping preview for this take"));
                return finish(stats, stop);
            }
        };
        let (stable, tentative) = agree.update(&hyp);
        let mut stable_all = committed_text.clone();
        append_text(&mut stable_all, &stable);
        let update = PreviewUpdate { session, stable: stable_all, tentative };
        if update != last_sent && !stopped() {
            on_update(update.clone());
            last_sent = update;
        }
    }
}

fn finish(mut stats: PreviewStats, stop: &AtomicBool) -> PreviewStats {
    stats.aborted = stop.load(Ordering::SeqCst);
    stats
}

fn append_text(acc: &mut String, more: &str) {
    let more = more.trim();
    if more.is_empty() {
        return;
    }
    if !acc.is_empty() {
        acc.push(' ');
    }
    acc.push_str(more);
}

/// Any 100 ms block above the speech level.
fn has_speech(samples: &[f32], level: f32) -> bool {
    samples.chunks(1600).any(|b| {
        let rms = (b.iter().map(|&s| s * s).sum::<f32>() / b.len().max(1) as f32).sqrt();
        rms > level
    })
}

/// Shorten every silent stretch to at most 300 ms. Whisper invents words
/// (or repeats itself) over long silences; the preview doesn't need them.
fn squeeze_silence(samples: &[f32], level: f32) -> Vec<f32> {
    const BLOCK: usize = 1600; // 100 ms
    const KEEP_QUIET_BLOCKS: usize = 3;
    let mut out = Vec::with_capacity(samples.len());
    let mut quiet_run = 0usize;
    for b in samples.chunks(BLOCK) {
        let rms = (b.iter().map(|&s| s * s).sum::<f32>() / b.len().max(1) as f32).sqrt();
        if rms > level * 0.5 {
            quiet_run = 0;
            out.extend_from_slice(b);
        } else {
            quiet_run += 1;
            if quiet_run <= KEEP_QUIET_BLOCKS {
                out.extend_from_slice(b);
            }
        }
    }
    out
}

/// Remove whisper's repetition loops from a rough hypothesis: consecutive
/// duplicate sentences, phrases repeated back to back, and stuttered tokens
/// like "re-re-re-re-".
fn collapse_repeats(text: &str) -> String {
    // 1) Stuttered tokens: cut a word at its first repeated hyphen piece.
    let words: Vec<String> = text
        .split_whitespace()
        .map(|w| {
            let parts: Vec<&str> = w.split('-').collect();
            if parts.len() > 3 && parts.windows(2).filter(|p| p[0] == p[1]).count() >= 2 {
                format!("{}-", parts[0])
            } else {
                w.to_string()
            }
        })
        .collect();
    // 2) Phrases (1..=40 words, i.e. up to a long sentence) repeated back to
    //    back: keep one copy.
    let mut out: Vec<String> = Vec::with_capacity(words.len());
    for w in words {
        out.push(w);
        for n in 1..=40usize {
            let len = out.len();
            if len >= 2 * n
                && out[len - n..]
                    .iter()
                    .zip(&out[len - 2 * n..len - n])
                    .all(|(a, b)| norm(a) == norm(b))
            {
                out.truncate(len - n);
                break;
            }
        }
    }
    out.join(" ")
}

/// Quietest 100 ms block within ±3 s of `target` (index into 16 kHz audio).
fn quiet_cut(samples: &[f32], target: usize) -> usize {
    let win = 1600;
    let lo = target.saturating_sub(48_000).max(win);
    let hi = (target + 48_000).min(samples.len().saturating_sub(win));
    let mut best = target.min(samples.len());
    let mut best_rms = f32::MAX;
    let mut i = lo;
    while i < hi {
        let b = &samples[i..i + win];
        let rms = (b.iter().map(|&s| s * s).sum::<f32>() / win as f32).sqrt();
        if rms < best_rms {
            best_rms = rms;
            best = i + win / 2;
        }
        i += win / 2;
    }
    best.clamp(win, samples.len())
}

/// Two-decode agreement tracker (LocalAgreement-2 style).
#[derive(Default)]
struct Agreement {
    prev: Vec<String>,
    stable: Vec<String>,
}

fn norm(w: &str) -> String {
    w.chars()
        .filter(|c| c.is_alphanumeric() || *c == '\'')
        .collect::<String>()
        .to_lowercase()
}

impl Agreement {
    /// Feed a new hypothesis; returns (stable, tentative) display strings.
    fn update(&mut self, hyp: &str) -> (String, String) {
        let cur: Vec<String> = hyp.split_whitespace().map(str::to_string).collect();
        let n_stable = self.stable.len();
        let cur_extends_stable = cur.len() >= n_stable
            && cur[..n_stable].iter().zip(&self.stable).all(|(a, b)| norm(a) == norm(b));
        if cur_extends_stable {
            // Grow stable by the words the last two decodes agree on.
            let lcp = cur
                .iter()
                .zip(&self.prev)
                .take_while(|(a, b)| norm(a) == norm(b))
                .count();
            if lcp > n_stable {
                self.stable = cur[..lcp].to_vec();
            }
        }
        // Tentative = the newest hypothesis past what's already stable.
        let tentative = if cur.len() > self.stable.len() {
            cur[self.stable.len()..].join(" ")
        } else {
            String::new()
        };
        self.prev = cur;
        (self.stable.join(" "), tentative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_text_only_grows() {
        let mut a = Agreement::default();
        assert_eq!(a.update("hello"), ("".into(), "hello".into()));
        assert_eq!(a.update("hello there"), ("hello".into(), "there".into()));
        assert_eq!(a.update("hello there my friend"), ("hello there".into(), "my friend".into()));
        // Whisper revises an already-stable word: stable stays as shown.
        let (s, t) = a.update("Hello, they're my friend and");
        assert_eq!(s, "hello there");
        assert_eq!(t, "my friend and");
    }

    #[test]
    fn repeats_are_collapsed() {
        assert_eq!(
            collapse_repeats("Good morning. Good morning. Good morning. Thanks"),
            "Good morning. Thanks"
        );
        assert_eq!(
            collapse_repeats("the quarterly re-re-re-re-re-re- review"),
            "the quarterly re- review"
        );
        assert_eq!(collapse_repeats("no, no, I mean yes"), "no, I mean yes");
        assert_eq!(collapse_repeats("plain sentence here"), "plain sentence here");
    }

    #[test]
    fn silence_is_squeezed() {
        let mut v = vec![0.2f32; 16_000];
        v.extend(vec![0.0f32; 16_000 * 4]);
        v.extend(vec![0.2f32; 16_000]);
        let out = squeeze_silence(&v, 0.015);
        assert_eq!(out.len(), 16_000 * 2 + 1600 * 3);
    }

    #[test]
    fn cut_lands_in_silence() {
        let mut v = vec![0.3f32; 16_000 * 20];
        for x in &mut v[16_000 * 12..16_000 * 12 + 4_000] {
            *x = 0.0;
        }
        let c = quiet_cut(&v, 16_000 * 13);
        assert!(c >= 16_000 * 12 && c <= 16_000 * 12 + 4_000, "cut at {c}");
    }
}

/// Debug-only end-to-end self-test: with `MURMR_SELFTEST=1` and
/// `MURMR_FAKE_MIC_WAV=<file>`, presses the dictation key through the real
/// controller channel (practice mode: nothing is pasted or saved), holds it
/// for the WAV's length, releases, and exits. Everything is logged to
/// perf.log. Used to test the preview without touching the user's keyboard,
/// focus, or installed Murmr.
#[cfg(debug_assertions)]
pub mod selftest {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use crossbeam_channel::Sender;
    use tauri::{AppHandle, Manager};

    use crate::hotkey::HotkeyEvent;
    use crate::perf_log;

    pub fn spawn(app: AppHandle, tx: Sender<HotkeyEvent>, practice: Arc<AtomicBool>) {
        std::thread::spawn(move || {
            practice.store(true, Ordering::SeqCst);
            for label in ["main", "onboarding"] {
                if let Some(w) = app.get_webview_window(label) {
                    let _ = w.hide();
                }
            }
            let secs: f64 = std::env::var("MURMR_FAKE_MIC_WAV")
                .ok()
                .and_then(|p| hound::WavReader::open(p).ok())
                .map(|r| r.duration() as f64 / r.spec().sample_rate as f64)
                .unwrap_or(5.0);
            std::thread::sleep(Duration::from_secs(4)); // let the HUD webview load
            perf_log::append(&format!("[selftest] DictationDown (clip {secs:.1}s)"));
            let _ = tx.send(HotkeyEvent::DictationDown { pressed_at: Instant::now() });
            std::thread::sleep(Duration::from_secs_f64(secs + 0.7));
            perf_log::append("[selftest] DictationUp");
            let _ = tx.send(HotkeyEvent::DictationUp);
            std::thread::sleep(Duration::from_secs(20));
            perf_log::append("[selftest] done, exiting");
            app.exit(0);
        });
    }
}
