//! Audio capture and processing for Murmr.
//!
//! Phase 2 added: capture mic audio, resample to 16 kHz mono f32 (Whisper's
//! required input format).
//! Phase 3 added: streaming `Recorder` (start/stop/cancel) that backs the
//! global hotkey loop.

pub mod capture;
pub mod resample;

pub use capture::{
    record_for_seconds, record_for_seconds_with_rms, ErrorCallback, Recorder, RmsCallback,
};
pub use resample::to_whisper_format;

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InputDevice {
    pub name: String,
    pub is_default: bool,
}

/// Enumerate the system's input devices. Used by the Microphone settings page.
pub fn list_input_devices() -> Result<Vec<InputDevice>, String> {
    let host = cpal::default_host();
    let default_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();
    let devices = host
        .input_devices()
        .map_err(|e| format!("enumerate input devices: {e}"))?;
    let mut out = Vec::new();
    for d in devices {
        let name = d.name().unwrap_or_else(|_| "<unknown>".into());
        let is_default = name == default_name;
        out.push(InputDevice { name, is_default });
    }
    Ok(out)
}
