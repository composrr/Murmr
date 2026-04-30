//! System-volume "ducking" while recording.
//!
//! When the user starts dictating, lower the default playback device's
//! master volume so background music / videos / Zoom calls don't drown out
//! their voice (and so the listener — the user themselves — isn't fighting
//! ambient audio while monitoring the input). When recording stops, restore
//! the volume the user originally had.
//!
//! Implementation notes:
//!
//! - Windows uses `IAudioEndpointVolume::SetMasterVolumeLevelScalar` against
//!   the default render endpoint. This affects the master output mixer, so
//!   it dims EVERY app's audio including our own start/stop chimes — but
//!   we only dim by ~30% by default so the chimes still cut through.
//! - macOS / Linux are no-ops for v1. macOS would use CoreAudio's
//!   `AudioObjectSetPropertyData`; Linux varies by sound server (PulseAudio
//!   has `pa_context_set_sink_volume_by_index`, PipeWire has its own API).
//!   Punted until we have Mac users.
//! - We capture the prior volume on first `duck()` and restore it on
//!   `unduck()`. If `duck()` is called twice in a row (e.g. tap-to-toggle),
//!   the second call is a no-op so we don't accidentally save the
//!   already-ducked volume as the "prior" value.

use crate::perf_log;

#[cfg(target_os = "windows")]
mod windows_impl;

/// Lower the system master volume by `amount` (0.0–1.0). 0.3 means the
/// volume is multiplied by 0.7 (a 30% reduction). 0.0 is a no-op.
///
/// Idempotent: calling `duck` while already ducked has no effect.
pub fn duck(amount: f32) {
    perf_log::append(&format!("[duck] requested amount={amount:.2}"));
    if amount <= 0.0 || amount > 1.0 {
        perf_log::append("[duck] skipping (amount out of range or zero)");
        return;
    }
    #[cfg(target_os = "windows")]
    windows_impl::duck(amount);
    #[cfg(not(target_os = "windows"))]
    {
        perf_log::append("[duck] no-op on this platform");
        let _ = amount;
    }
}

/// Restore the volume captured by the most recent `duck` call. No-op if
/// `duck` was never called or if we've already unducked.
pub fn unduck() {
    perf_log::append("[duck] unduck requested");
    #[cfg(target_os = "windows")]
    windows_impl::unduck();
}
