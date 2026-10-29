//! Developer benchmark: `murmr --murmr-bench <model.bin> <audio.wav> [--cpu] [--no-vad] [--ref <ref.txt>] [--runs N]`
//!
//! Runs the same engine path a dictation uses (Transcriber + VAD + long-take
//! chunking with context carry-over) on a WAV file and prints load time,
//! per-run transcribe time, real-time ratio and, with `--ref`, word error
//! rate. Exits without starting the app. Used to produce the numbers in the
//! engine PR; handy for checking a user's machine too.

use std::path::Path;
use std::time::Instant;

use super::{gpu, models, Transcriber};

pub const BENCH_ARG: &str = "--murmr-bench";

pub fn maybe_run_bench_and_exit() {
    let args: Vec<String> = std::env::args().collect();
    // ---- Live preview bench ----
    if let Some(i) = args.iter().position(|a| a == PREVIEW_BENCH_ARG) {
        let code = match run_preview(&args[i + 1..]) {
            Ok(()) => 0,
            Err(e) => {
                println!("PREVIEW error: {e}");
                1
            }
        };
        std::process::exit(code);
    }
    // ---- end live preview bench ----
    let Some(i) = args.iter().position(|a| a == BENCH_ARG) else {
        return;
    };
    let rest = &args[i + 1..];
    let code = match run(rest) {
        Ok(()) => 0,
        Err(e) => {
            println!("BENCH error: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn flag_value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn run(args: &[String]) -> Result<(), String> {
    let model = args.first().ok_or("usage: --murmr-bench <model> <wav> [...]")?;
    let wav = args.get(1).ok_or("missing wav path")?;
    let use_gpu = !args.iter().any(|a| a == "--cpu");
    let use_vad = !args.iter().any(|a| a == "--no-vad");
    let runs: usize = flag_value(args, "--runs").and_then(|v| v.parse().ok()).unwrap_or(3);
    let reference = flag_value(args, "--ref")
        .map(|p| std::fs::read_to_string(p).map_err(|e| format!("read ref: {e}")))
        .transpose()?;

    let samples = read_wav_16k_mono(Path::new(wav))?;
    let audio_s = samples.len() as f64 / 16_000.0;

    let vad_path = Path::new(model)
        .parent()
        .map(|d| d.join(models::VAD_MODEL))
        .filter(|p| p.is_file());
    let t0 = Instant::now();
    let t = Transcriber::new(model, use_gpu, if use_vad { vad_path.as_deref() } else { None })?;
    let load_ms = t0.elapsed().as_millis();
    println!(
        "BENCH model={} backend={} vad={} audio={audio_s:.2}s load={load_ms}ms gpu_compiled={}",
        t.model_name(),
        t.backend().label(),
        t.has_vad(),
        gpu::compiled_with_gpu()
    );

    let mut times = Vec::new();
    let mut last_text = String::new();
    for r in 0..runs {
        let t1 = Instant::now();
        let text = crate::controller::transcribe_maybe_chunked(&t, &samples, None, false)?;
        let ms = t1.elapsed().as_millis() as f64;
        println!(
            "BENCH run={r} ms={ms:.0} ratio={:.3}",
            ms / 1000.0 / audio_s
        );
        times.push(ms);
        last_text = text;
    }
    // Median of the warm runs (skip the first, which pays GPU pipeline setup).
    let mut warm: Vec<f64> = if times.len() > 1 { times[1..].to_vec() } else { times.clone() };
    warm.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = warm[warm.len() / 2];
    println!(
        "BENCH summary first_ms={:.0} warm_median_ms={median:.0} warm_ratio={:.3} speed={:.1}x_realtime",
        times[0],
        median / 1000.0 / audio_s,
        audio_s / (median / 1000.0)
    );
    if let Some(r) = reference {
        let (errs, n) = word_errors(&r, &last_text);
        println!(
            "BENCH wer={:.2}% ({errs}/{n} word errors)",
            100.0 * errs as f64 / n.max(1) as f64
        );
    }
    println!("BENCH text={last_text}");
    Ok(())
}

fn read_wav_16k_mono(path: &Path) -> Result<Vec<f32>, String> {
    let mut r = hound::WavReader::open(path).map_err(|e| format!("open wav: {e}"))?;
    let spec = r.spec();
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => r.samples::<f32>().filter_map(Result::ok).collect(),
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            r.samples::<i32>().filter_map(Result::ok).map(|s| s as f32 / max).collect()
        }
    };
    crate::audio::to_whisper_format(&raw, spec.sample_rate, spec.channels)
}

fn normalize(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric() || *c == '\'')
                .collect::<String>()
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Word-level edit distance (substitutions + insertions + deletions).
fn word_errors(reference: &str, hypothesis: &str) -> (usize, usize) {
    let r = normalize(reference);
    let h = normalize(hypothesis);
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for i in 1..=r.len() {
        let mut cur = vec![i; h.len() + 1];
        for j in 1..=h.len() {
            let sub = prev[j - 1] + usize::from(r[i - 1] != h[j - 1]);
            cur[j] = sub.min(prev[j] + 1).min(cur[j - 1] + 1);
        }
        prev = cur;
    }
    (prev[h.len()], r.len())
}

// ---------------------------------------------------------------------------
// Live preview bench:
//   murmr --murmr-bench-preview <model.bin> <audio.wav> [--cpu] [--cpu-preview]
// Plays the WAV in real time through the real preview worker (as if spoken
// into the mic), prints each HUD update, then — the moment "recording" stops —
// runs the normal final pass and reports how long it took. A second final pass
// with no preview running gives the baseline to compare against.
// ---------------------------------------------------------------------------

pub const PREVIEW_BENCH_ARG: &str = "--murmr-bench-preview";

struct WavPlayback {
    samples: Vec<f32>,
    started: Instant,
    done: std::sync::atomic::AtomicBool,
}

impl crate::live_preview::PreviewSource for WavPlayback {
    fn peek(&self, from: usize) -> Option<crate::audio::capture::LiveAudio> {
        if self.done.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let n = ((self.started.elapsed().as_secs_f64() * 16_000.0) as usize).min(self.samples.len());
        Some(crate::audio::capture::LiveAudio {
            samples: self.samples[from.min(n)..n].to_vec(),
            sample_rate: 16_000,
            channels: 1,
        })
    }
}

fn run_preview(args: &[String]) -> Result<(), String> {
    use crate::live_preview::{self, PreviewConfig};
    use std::sync::Arc;

    let model = args.first().ok_or("usage: --murmr-bench-preview <model> <wav> [--cpu]")?;
    let wav = args.get(1).ok_or("missing wav path")?;
    let use_gpu = !args.iter().any(|a| a == "--cpu");
    let samples = read_wav_16k_mono(Path::new(wav))?;
    let audio_s = samples.len() as f64 / 16_000.0;
    let vad_path = Path::new(model)
        .parent()
        .map(|d| d.join(models::VAD_MODEL))
        .filter(|p| p.is_file());
    let t = Arc::new(Transcriber::new(model, use_gpu, vad_path.as_deref())?);
    let cfg = if t.backend().is_gpu() && !args.iter().any(|a| a == "--cpu-preview") {
        PreviewConfig::gpu()
    } else {
        PreviewConfig::cpu()
    };
    println!(
        "PREVIEW model={} backend={} audio={audio_s:.2}s cfg={:?}",
        t.model_name(),
        t.backend().label(),
        cfg
    );
    // Warm up so first-decode GPU setup doesn't skew either side.
    let _ = t.transcribe(&samples[..samples.len().min(16_000 * 3)], None, false);

    // Baseline final pass, nothing else running.
    let t0 = Instant::now();
    let base_text = crate::controller::transcribe_maybe_chunked(&t, &samples, None, false)?;
    let base_ms = t0.elapsed().as_millis();

    let src = Arc::new(WavPlayback {
        samples: samples.clone(),
        started: Instant::now(),
        done: std::sync::atomic::AtomicBool::new(false),
    });
    let started = src.started;
    let (stats_tx, stats_rx) = std::sync::mpsc::channel();
    let first_text = Arc::new(parking_lot::Mutex::new(None::<f64>));
    let first_text_w = Arc::clone(&first_text);
    let handle = live_preview::start(
        Arc::clone(&t),
        src.clone(),
        1,
        None,
        cfg,
        move |u| {
            let at = started.elapsed().as_secs_f64();
            first_text_w.lock().get_or_insert(at);
            println!("PREVIEW t={at:5.2}s stable=\"{}\" tentative=\"{}\"", u.stable, u.tentative);
        },
        move |st| {
            let _ = stats_tx.send(st);
        },
    );
    // "Speak" the whole file in real time.
    std::thread::sleep(std::time::Duration::from_secs_f64(audio_s));
    // Stop exactly like the controller: abort the preview, then final pass.
    let t1 = Instant::now();
    handle.stop();
    src.done.store(true, std::sync::atomic::Ordering::SeqCst);
    let final_text = crate::controller::transcribe_maybe_chunked(&t, &samples, None, false)?;
    let final_ms = t1.elapsed().as_millis();
    let stats = stats_rx
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| "preview worker did not finish")?;
    let avg = if stats.decodes > 0 { stats.total_decode_ms / stats.decodes as u128 } else { 0 };
    println!(
        "PREVIEW stats decodes={} avg_decode={avg}ms max_decode={}ms skipped_ticks={} commits={} first_text_at={:.2}s",
        stats.decodes,
        stats.max_decode_ms,
        stats.skipped_ticks,
        stats.commits,
        first_text.lock().unwrap_or(-1.0)
    );
    println!(
        "PREVIEW final_after_stop={final_ms}ms baseline_final={base_ms}ms same_text={}",
        final_text == base_text
    );
    println!("PREVIEW final_text={final_text}");
    Ok(())
}
