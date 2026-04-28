//! Stub Windows audio-ducking implementation.
//!
//! TODO: Wire up `IAudioEndpointVolume::SetMasterVolumeLevelScalar` properly.
//! The windows-rs 0.62 generic `IMMDevice::Activate<T: Interface>(...)`
//! method couldn't be resolved at compile time — needs a different feature
//! combination or a switch to a dedicated audio crate (e.g.
//! `windows-volume-control` or hand-rolled windows-sys FFI).
//!
//! For now both functions are no-ops so the rest of the controller
//! lifecycle code (which calls `duck()` / `unduck()` around recording)
//! compiles + runs without behaviour change. The user-facing duck slider
//! in Settings → Microphone will appear to do nothing until this lands.

pub fn duck(_amount: f32) {
    // intentional no-op — see module docstring
}

pub fn unduck() {
    // intentional no-op — see module docstring
}
