//! Convert captured audio into the format Whisper expects: 16 kHz, mono, f32.
//!
//! Mics deliver multichannel audio at 44.1 / 48 kHz (sometimes higher). Whisper
//! requires 16 kHz mono f32. This module mixes channels to mono first, then
//! resamples with rubato's FFT-based resampler (high quality, good fit for
//! finite-length offline buffers).

use rubato::{FftFixedIn, Resampler};

/// Mix multichannel interleaved samples down to a mono stream.
fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks_exact(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Resample mono f32 audio from `source_sr` to 16 000 Hz using FFT resampling.
fn resample_mono_to_16k(mono: &[f32], source_sr: u32) -> Result<Vec<f32>, String> {
    if source_sr == 16_000 {
        return Ok(mono.to_vec());
    }

    const CHUNK: usize = 1024;
    let mut resampler = FftFixedIn::<f32>::new(source_sr as usize, 16_000, CHUNK, 2, 1)
        .map_err(|e| format!("resampler init failed: {e}"))?;

    let in_per_chunk = resampler.input_frames_next();
    let mut out: Vec<f32> = Vec::with_capacity(mono.len() * 16_000 / source_sr as usize + 1024);
    let mut pos = 0;

    while pos + in_per_chunk <= mono.len() {
        let waves_in: [&[f32]; 1] = [&mono[pos..pos + in_per_chunk]];
        let waves_out = resampler
            .process(&waves_in, None)
            .map_err(|e| format!("resampler process failed: {e}"))?;
        out.extend_from_slice(&waves_out[0]);
        pos += in_per_chunk;
    }

    if pos < mono.len() {
        let tail: [&[f32]; 1] = [&mono[pos..]];
        let waves_out = resampler
            .process_partial(Some(&tail), None)
            .map_err(|e| format!("resampler partial failed: {e}"))?;
        out.extend_from_slice(&waves_out[0]);
    }

    Ok(out)
}

/// Convert raw captured samples → 16 kHz mono f32, ready for Whisper.
pub fn to_whisper_format(samples: &[f32], source_sr: u32, channels: u16) -> Result<Vec<f32>, String> {
    let mono = to_mono(samples, channels);
    resample_mono_to_16k(&mono, source_sr)
}
