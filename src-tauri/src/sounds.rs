//! Subtle UI sounds.
//!
//! Two paths:
//!
//! 1. **Custom WAVs** — if the user drops `start.wav`, `complete.wav`, or
//!    `error.wav` into `<app-data>/sounds/`, we play those verbatim. Lets
//!    people skin Murmr with their own sounds.
//! 2. **Synthesized fallback** — soft, muted, low-pitched tones generated
//!    at runtime via `rodio`. Default voice. Tuned to feel like a modern
//!    macOS-style "tock" rather than a sharp digital chirp.
//!
//! `rodio::OutputStream` is `!Send`, so each sound spins up its own
//! short-lived OS thread that opens a stream, plays, sleeps the duration,
//! and drops everything. Audio init failures are silently swallowed —
//! sounds are polish, not a hard requirement.

use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rodio::source::{SineWave, Source};
use rodio::{Decoder, OutputStream, OutputStreamHandle};
use tauri::{AppHandle, Manager};

use crate::settings::SettingsStore;

// Synthesized tone parameters — chosen to feel muted and round rather than
// sharp / digital. Lower fundamental (~280–400 Hz) + a soft octave overtone,
// short envelope, very low amplitude.
const START_FUND_HZ: f32 = 280.0;
const START_HARMONIC_HZ: f32 = 560.0;
const START_HARMONIC_GAIN: f32 = 0.30;
const START_DURATION_MS: u64 = 90;
const START_AMPLITUDE: f32 = 0.10;

const COMPLETE_FUND_HZ: f32 = 380.0;
const COMPLETE_HARMONIC_HZ: f32 = 760.0;
const COMPLETE_HARMONIC_GAIN: f32 = 0.30;
const COMPLETE_DURATION_MS: u64 = 130;
const COMPLETE_AMPLITUDE: f32 = 0.10;

const ERROR_FUND_HZ: f32 = 220.0;
const ERROR_HARMONIC_HZ: f32 = 330.0;
const ERROR_HARMONIC_GAIN: f32 = 0.40;
const ERROR_DURATION_MS: u64 = 240;
const ERROR_AMPLITUDE: f32 = 0.13;

pub struct SoundPlayer {
    settings: Arc<SettingsStore>,
    sounds_dir: Option<PathBuf>,
}

impl SoundPlayer {
    pub fn new(settings: Arc<SettingsStore>, app: &AppHandle) -> Arc<Self> {
        // Pre-resolve the custom-sounds directory; create it lazily on first
        // use. Failures here are non-fatal — we'll just skip custom files.
        let sounds_dir = app
            .path()
            .app_data_dir()
            .ok()
            .map(|d| d.join("sounds"));

        if let Some(dir) = &sounds_dir {
            let _ = std::fs::create_dir_all(dir);
        }

        Arc::new(Self { settings, sounds_dir })
    }

    pub fn play_start_click(&self) {
        if !self.settings.get().sound_start_click {
            return;
        }
        self.play("start", START_DURATION_MS, |handle| {
            play_warm_tone(
                handle,
                START_FUND_HZ,
                START_HARMONIC_HZ,
                START_HARMONIC_GAIN,
                START_DURATION_MS,
                START_AMPLITUDE,
            );
        });
    }

    pub fn play_complete_chime(&self) {
        if !self.settings.get().sound_complete_chime {
            return;
        }
        self.play("complete", COMPLETE_DURATION_MS, |handle| {
            play_warm_tone(
                handle,
                COMPLETE_FUND_HZ,
                COMPLETE_HARMONIC_HZ,
                COMPLETE_HARMONIC_GAIN,
                COMPLETE_DURATION_MS,
                COMPLETE_AMPLITUDE,
            );
        });
    }

    pub fn play_error_beep(&self) {
        if !self.settings.get().sound_error_beep {
            return;
        }
        self.play("error", ERROR_DURATION_MS, |handle| {
            play_warm_tone(
                handle,
                ERROR_FUND_HZ,
                ERROR_HARMONIC_HZ,
                ERROR_HARMONIC_GAIN,
                ERROR_DURATION_MS,
                ERROR_AMPLITUDE,
            );
        });
    }

    fn play<F>(&self, key: &'static str, fallback_duration_ms: u64, synth: F)
    where
        F: FnOnce(&OutputStreamHandle) + Send + 'static,
    {
        let custom = self.custom_path_for(key);
        spawn_playback(playback_thread_name(key), fallback_duration_ms, move |handle| {
            if let Some(path) = custom.as_ref().filter(|p| p.exists()) {
                if let Ok(file) = File::open(path) {
                    if let Ok(decoder) = Decoder::new(BufReader::new(file)) {
                        let _ = handle.play_raw(decoder.convert_samples());
                        return;
                    }
                }
                eprintln!("[sounds] custom file at {path:?} couldn't be decoded; falling back");
            }
            synth(handle);
        });
    }

    fn custom_path_for(&self, key: &str) -> Option<PathBuf> {
        let dir = self.sounds_dir.as_ref()?;
        // Accept either `<key>.wav` or `<key>.mp3` (rodio decodes both with
        // its default features — we kept just `wav` to slim deps, so .wav
        // is the supported drop-in).
        Some(dir.join(format!("{key}.wav")))
    }

    pub fn sounds_dir(&self) -> Option<&PathBuf> {
        self.sounds_dir.as_ref()
    }
}

fn playback_thread_name(key: &str) -> &'static str {
    match key {
        "start" => "murmr-snd-start",
        "complete" => "murmr-snd-complete",
        "error" => "murmr-snd-error",
        _ => "murmr-snd",
    }
}

/// Two-tone synthesized tone: fundamental + octave overtone, mixed to feel
/// rounder than a single sine. Symmetric fade-in/out so it sounds like a
/// soft bump rather than a click.
fn play_warm_tone(
    handle: &OutputStreamHandle,
    fund_hz: f32,
    harm_hz: f32,
    harm_gain: f32,
    duration_ms: u64,
    amplitude: f32,
) {
    let dur = Duration::from_millis(duration_ms);
    let fade = Duration::from_millis(duration_ms.min(40).max(10));

    let fund = SineWave::new(fund_hz)
        .take_duration(dur)
        .amplify(amplitude)
        .fade_in(fade);
    let harm = SineWave::new(harm_hz)
        .take_duration(dur)
        .amplify(amplitude * harm_gain)
        .fade_in(fade);
    let mixed = fund.mix(harm);
    let _ = handle.play_raw(mixed.convert_samples());
}

fn spawn_playback<F>(name: &'static str, hold_ms: u64, play: F)
where
    F: FnOnce(&OutputStreamHandle) + Send + 'static,
{
    let _ = thread::Builder::new().name(name.into()).spawn(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => {
                play(&handle);
                // Keep the stream alive long enough for the sound to finish.
                // 200 ms padding handles file-decoded sources that may be
                // longer than the synthesized fallback's hold_ms.
                thread::sleep(Duration::from_millis(hold_ms + 200));
            }
            Err(e) => {
                eprintln!("[sounds] audio init failed: {e:?}");
            }
        }
    });
}
