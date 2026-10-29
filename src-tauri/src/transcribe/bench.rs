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
