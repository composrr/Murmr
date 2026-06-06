//! Windows audio ducking via per-app session volume.
//!
//! Why per-session and not master volume?
//!   The Windows master scalar (`IAudioEndpointVolume`) only affects the
//!   default render endpoint. Anyone using Voicemeeter, OBS, Discord with
//!   custom audio routing, Bluetooth headphones, or any virtual audio cable
//!   gets nothing — their audio stream isn't on the endpoint we touched.
//!
//!   `IAudioSessionManager2::GetSessionEnumerator` walks every active audio
//!   session on every active render device. We dim each one individually
//!   via `ISimpleAudioVolume::SetMasterVolume`. Discord, Teams, WhisperFlow
//!   all do the same thing. Works regardless of routing topology.
//!
//! State management:
//!   On `duck()` we save `(process_id → original_volume)` for every session
//!   we touched. `unduck()` re-enumerates and restores by process_id, so a
//!   process that opened a NEW audio stream after `duck()` doesn't get
//!   restored to "1.0" — it keeps whatever it currently has.
//!
//!   KNOWN LIMITATION: apps with multiple audio sessions per PID (Chrome
//!   tabs, Discord channels, OBS scenes) all get restored to the value of
//!   whichever single session was enumerated last during duck — so the
//!   restore is "good enough" but not perfectly per-session. v0.1.44 tried
//!   to key by session_instance_id (PWSTR from GetSessionInstanceIdentifier)
//!   to fix this, but the PWSTR handling crashed mid-dictation on user
//!   machines (STATUS_ILLEGAL_INSTRUCTION). v0.1.52 tried a MAX-over-PID
//!   heuristic but it didn't actually help the Apex Legends case the user
//!   reported (still under investigation — likely a different cause).
//!   For now we keep the PID-keyed last-wins restore.
//!
//!   Sessions whose process exited between duck/unduck just disappear from
//!   the enumeration and are silently dropped.
//!
//! Crash isolation:
//!   Both `duck()` and `unduck()` are wrapped in `std::panic::catch_unwind`
//!   so any panic inside COM enumeration / interface casting / volume
//!   manipulation cannot take down the whole Murmr process. Failed-to-duck
//!   is a much less bad outcome than crashed-the-app.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;

use parking_lot::Mutex;
use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::perf_log;

/// process_id → original volume (0.0–1.0) at the time we ducked. Cleared
/// each `unduck()`.
static SAVED_VOLUMES: Mutex<Option<HashMap<u32, f32>>> = Mutex::new(None);

/// Per-thread COM init. Re-entry on the same thread is harmless (returns
/// S_FALSE for matching apartment, RPC_E_CHANGED_MODE for a flip).
fn ensure_com_init() {
    use std::sync::Once;
    thread_local! {
        static INIT: Once = Once::new();
    }
    INIT.with(|once| {
        once.call_once(|| unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        });
    });
}

/// Our own process ID. Skip Murmr's audio sessions (start chime, success
/// chime, etc.) so we don't dim our own UI sounds.
fn current_process_id() -> u32 {
    unsafe { GetCurrentProcessId() }
}

/// Walk every active render device, every active session on each device.
/// `f` is called with (process_id, simple_audio_volume) for each non-Murmr
/// session — short-circuits on COM errors at the device level so one bad
/// device doesn't kill the whole walk.
fn for_each_session<F: FnMut(u32, &ISimpleAudioVolume)>(mut f: F) {
    let our_pid = current_process_id();

    unsafe {
        ensure_com_init();

        let enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(e) => e,
                Err(e) => {
                    perf_log::append(&format!("[duck] CoCreateInstance failed: {e:?}"));
                    return;
                }
            };

        let devices = match enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            Ok(d) => d,
            Err(e) => {
                perf_log::append(&format!("[duck] EnumAudioEndpoints failed: {e:?}"));
                return;
            }
        };

        let device_count = devices.GetCount().unwrap_or(0);
        perf_log::append(&format!("[duck] {device_count} active render device(s)"));

        for di in 0..device_count {
            let device = match devices.Item(di) {
                Ok(d) => d,
                Err(_) => continue,
            };

            let session_mgr: IAudioSessionManager2 = match device
                .Activate::<IAudioSessionManager2>(CLSCTX_ALL, None)
            {
                Ok(m) => m,
                Err(e) => {
                    perf_log::append(&format!("[duck] device {di} Activate failed: {e:?}"));
                    continue;
                }
            };

            let sessions = match session_mgr.GetSessionEnumerator() {
                Ok(s) => s,
                Err(e) => {
                    perf_log::append(&format!("[duck] device {di} GetSessionEnumerator failed: {e:?}"));
                    continue;
                }
            };
            let session_count = sessions.GetCount().unwrap_or(0);

            for si in 0..session_count {
                let control = match sessions.GetSession(si) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                // Get extended interface so we can read process_id.
                let control2: IAudioSessionControl2 = match control.cast() {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let pid = control2.GetProcessId().unwrap_or(0);
                if pid == 0 || pid == our_pid {
                    // pid 0 = system sounds session; skip those + our own.
                    continue;
                }

                let vol: ISimpleAudioVolume = match control.cast() {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                f(pid, &vol);
            }
        }
    }
}

pub fn duck(amount: f32) {
    // Wrap the entire duck pass in catch_unwind so any panic in COM
    // enumeration / interface casting / volume calls cannot abort the
    // process. With panic = "abort" in Cargo.toml, an uncaught panic in
    // unsafe code is fatal — and audio ducking is non-essential, so
    // failing silently is the right tradeoff. Per-thread Once guards
    // CoInitializeEx so re-entry from the catch_unwind closure is safe.
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        duck_inner(amount);
    }));
    if let Err(e) = result {
        let msg = e
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<no message>");
        perf_log::append(&format!("[duck] panic during duck(): {msg}"));
        // Clear saved state so unduck() doesn't try to restore based on
        // possibly-partial data.
        *SAVED_VOLUMES.lock() = None;
    }
}

fn duck_inner(amount: f32) {
    {
        let guard = SAVED_VOLUMES.lock();
        if guard.is_some() {
            perf_log::append("[duck] already ducked, skipping");
            return;
        }
    }

    let mut saved: HashMap<u32, f32> = HashMap::new();
    let factor = (1.0 - amount).clamp(0.0, 1.0);
    let mut count = 0;

    for_each_session(|pid, vol| {
        unsafe {
            let current = vol.GetMasterVolume().unwrap_or(1.0);
            // Insert keyed by PID. For apps with multiple audio sessions
            // per PID (Chrome tabs, Discord channels, OBS) this means the
            // last session's volume wins the saved slot, and on unduck
            // all sessions of that PID restore to that single value.
            // Imperfect but stable — see the module doc comment for the
            // history with per-session keying.
            saved.insert(pid, current);
            let target = (current * factor).clamp(0.0, 1.0);
            // GUID context arg = caller's identifier so the OS can route
            // change-notifications back to us. Passing zero/null is fine
            // when we don't care.
            if let Err(e) = vol.SetMasterVolume(target, std::ptr::null()) {
                perf_log::append(&format!(
                    "[duck] pid={pid} SetMasterVolume failed: {e:?}"
                ));
            } else {
                count += 1;
            }
        }
    });

    perf_log::append(&format!(
        "[duck] ducked {count} session(s) by {:.0}%",
        amount * 100.0
    ));
    *SAVED_VOLUMES.lock() = Some(saved);
}

pub fn unduck() {
    let result = std::panic::catch_unwind(AssertUnwindSafe(unduck_inner));
    if let Err(e) = result {
        let msg = e
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<no message>");
        perf_log::append(&format!("[duck] panic during unduck(): {msg}"));
        // Clear so a future duck() isn't blocked by the "already ducked"
        // check pointing at stale data.
        *SAVED_VOLUMES.lock() = None;
    }
}

fn unduck_inner() {
    let saved = match SAVED_VOLUMES.lock().take() {
        Some(s) => s,
        None => {
            perf_log::append("[duck] unduck: not currently ducked, skipping");
            return;
        }
    };

    let mut restored = 0;
    for_each_session(|pid, vol| {
        if let Some(original) = saved.get(&pid) {
            unsafe {
                if let Err(e) = vol.SetMasterVolume(*original, std::ptr::null()) {
                    perf_log::append(&format!("[duck] pid={pid} restore failed: {e:?}"));
                } else {
                    restored += 1;
                }
            }
        }
    });

    let missing = saved.len().saturating_sub(restored);
    perf_log::append(&format!(
        "[duck] restored {restored}/{} session(s) ({} missing — process/session ended)",
        saved.len(),
        missing,
    ));
}
