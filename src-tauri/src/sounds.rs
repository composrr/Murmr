//! Murmr UI sounds.
//!
//! Two paths, in priority order:
//!   1. **User override** — if `<app-data>/sounds/<key>.wav` exists, we
//!      decode + play that. Lets people skin Murmr with their own sounds.
//!   2. **Embedded default** — WAV files baked into the binary at compile
//!      time via `include_bytes!`. Always available, no install dance.
//!
//! Only the start + stop sounds have embedded defaults — the error beep
//! falls back to a synthesized warm tone (no embedded WAV for that yet).
//!
//! `rodio::OutputStream` is `!Send`, so each sound spins up its own
//! short-lived OS thread that opens a stream, plays, sleeps long enough
//! for the sound to finish, and drops everything. Audio init failures
//! are silently swallowed — sounds are polish, not a hard requirement.
//!
//! Volume: every playback path runs through `Source::amplify(volume)`
//! where `volume` comes from `settings.sound_volume` (0.0–1.0+, where
//! values >1.0 boost above the file's native level).

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rodio::source::{SineWave, Source};
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink};
use tauri::{AppHandle, Manager};

use crate::perf_log;
use crate::settings::SettingsStore;

// ---- Embedded default sounds ----------------------------------------------

/// Compile-time-baked start chime (button-down).
const START_WAV: &[u8] = include_bytes!("../runtime/sounds/start.wav");
/// Compile-time-baked stop chime (button-up / recording-end).
const STOP_WAV: &[u8] = include_bytes!("../runtime/sounds/stop.wav");

// (PLAYBACK_HOLD_MS retired — Sink::sleep_until_end now blocks until the
// audio buffer drains naturally, so no fixed timer needed.)

// ---- Synthesized error beep params ----------------------------------------

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
        let sounds_dir = app.path().app_data_dir().ok().map(|d| d.join("sounds"));
        if let Some(dir) = &sounds_dir {
            let _ = std::fs::create_dir_all(dir);
        }
        Arc::new(Self { settings, sounds_dir })
    }

    pub fn play_start_click(&self) {
        let s = self.settings.get();
        if !s.sound_start_click {
            return;
        }
        let volume = s.sound_volume.max(0.0);
        let custom = self.custom_path_for("start");
        spawn_playback("murmr-snd-start", move |handle| {
            if play_custom(handle, custom.as_deref(), volume) {
                return;
            }
            play_embedded(handle, START_WAV, volume);
        });
    }

    /// Stop chime — fired the instant the user releases / toggles off
    /// recording, BEFORE transcription runs. Tying it to release (rather
    /// than transcribe-complete) keeps the audio feedback snappy and
    /// directly tied to the user's action.
    pub fn play_complete_chime(&self) {
        let s = self.settings.get();
        if !s.sound_complete_chime {
            return;
        }
        let volume = s.sound_volume.max(0.0);
        let custom = self.custom_path_for("complete");
        spawn_playback("murmr-snd-stop", move |handle| {
            if play_custom(handle, custom.as_deref(), volume) {
                return;
            }
            play_embedded(handle, STOP_WAV, volume);
        });
    }

    pub fn play_error_beep(&self) {
        let s = self.settings.get();
        if !s.sound_error_beep {
            return;
        }
        let volume = s.sound_volume.max(0.0);
        let custom = self.custom_path_for("error");
        spawn_playback("murmr-snd-error", move |handle| {
            if play_custom(handle, custom.as_deref(), volume) {
                return;
            }
            play_synth_warm_tone(
                handle,
                ERROR_FUND_HZ,
                ERROR_HARMONIC_HZ,
                ERROR_HARMONIC_GAIN,
                ERROR_DURATION_MS,
                ERROR_AMPLITUDE * volume,
            );
        });
    }

    fn custom_path_for(&self, key: &str) -> Option<PathBuf> {
        let dir = self.sounds_dir.as_ref()?;
        Some(dir.join(format!("{key}.wav")))
    }

    pub fn sounds_dir(&self) -> Option<&PathBuf> {
        self.sounds_dir.as_ref()
    }
}

// ---- Playback helpers -----------------------------------------------------

/// Try to decode + play a custom user WAV. Returns true if it succeeded
/// (so the caller skips the fallback), false if the file's missing or
/// can't be decoded. Blocks the calling thread until playback finishes.
fn play_custom(handle: &OutputStreamHandle, path: Option<&std::path::Path>, volume: f32) -> bool {
    let path = match path {
        Some(p) if p.exists() => p,
        _ => return false,
    };
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let decoder = match Decoder::new(BufReader::new(file)) {
        Ok(d) => d,
        Err(e) => {
            perf_log::append(&format!("[sounds] custom file at {path:?} won't decode: {e:?}"));
            return false;
        }
    };
    play_via_sink(handle, decoder, volume, "custom");
    true
}

/// Decode + play a baked-in WAV blob (start.wav / stop.wav). Blocks the
/// calling thread until playback finishes — important so the
/// `OutputStream` in the parent scope stays alive until the audio buffer
/// is fully drained.
fn play_embedded(handle: &OutputStreamHandle, bytes: &'static [u8], volume: f32) {
    let cursor = Cursor::new(bytes);
    let decoder = match Decoder::new(BufReader::new(cursor)) {
        Ok(d) => d,
        Err(e) => {
            perf_log::append(&format!("[sounds] embedded WAV won't decode: {e:?}"));
            return;
        }
    };
    play_via_sink(handle, decoder, volume, "embedded");
}

/// Wrap a Source in a Sink + block until done. Sink owns the source and
/// keeps the OutputStream attached so we don't drop audio mid-playback —
/// the previous timer-based approach (sleep N ms then drop) was racy on
/// macOS where CoreAudio takes longer to spin up.
fn play_via_sink<S>(handle: &OutputStreamHandle, source: S, volume: f32, label: &str)
where
    S: Source<Item = i16> + Send + 'static,
{
    let sink = match Sink::try_new(handle) {
        Ok(s) => s,
        Err(e) => {
            perf_log::append(&format!("[sounds] Sink::try_new failed ({label}): {e:?}"));
            return;
        }
    };
    sink.set_volume(volume);
    sink.append(source);
    sink.sleep_until_end();
}

/// Synthesized warm tone — used by the error beep when there's no custom
/// override. Two-tone (fundamental + octave overtone), short fade in/out.
fn play_synth_warm_tone(
    handle: &OutputStreamHandle,
    fund_hz: f32,
    harm_hz: f32,
    harm_gain: f32,
    duration_ms: u64,
    amplitude: f32,
) {
    let dur = Duration::from_millis(duration_ms);
    let fade = Duration::from_millis(duration_ms.min(40).max(10));
    let fund = SineWave::new(fund_hz).take_duration(dur).amplify(amplitude).fade_in(fade);
    let harm = SineWave::new(harm_hz).take_duration(dur).amplify(amplitude * harm_gain).fade_in(fade);
    let mixed = fund.mix(harm).convert_samples::<i16>();

    let sink = match Sink::try_new(handle) {
        Ok(s) => s,
        Err(e) => {
            perf_log::append(&format!("[sounds] Sink::try_new failed (synth): {e:?}"));
            return;
        }
    };
    sink.append(mixed);
    sink.sleep_until_end();
}

fn spawn_playback<F>(name: &'static str, play: F)
where
    F: FnOnce(&OutputStreamHandle) + Send + 'static,
{
    let _ = thread::Builder::new().name(name.into()).spawn(move || {
        match OutputStream::try_default() {
            Ok((_stream, handle)) => {
                play(&handle);
                // play_via_sink blocks until the buffer drains, so by the
                // time we get here the audio has finished. _stream drops
                // cleanly. No fixed-timer race anymore.
            }
            Err(e) => {
                // Pipe to perf_log too — Mac windowed apps eat stderr, so
                // without this any sound failure is invisible to users.
                let msg = format!("[sounds] audio init failed for {name}: {e:?}");
                eprintln!("{msg}");
                perf_log::append(&msg);
            }
        }
    });
}
