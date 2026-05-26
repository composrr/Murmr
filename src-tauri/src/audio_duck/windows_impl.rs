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
//!   On `duck()` we save `(session_instance_id → original_volume)` for
//!   every session we touched. `unduck()` re-enumerates and restores by
//!   session_instance_id. Keying by session ID (not process ID) is
//!   critical — apps like Chrome, Discord, Slack, OBS routinely have
//!   MULTIPLE audio sessions per process (one per browser tab, one per
//!   voice channel, plus notification sounds). Keying by PID meant the
//!   last session enumerated overwrote all the others' saved values, so
//!   `unduck()` restored every session of those apps to whichever single
//!   value happened to win the race — leaving some sessions stuck below
//!   their original volume. Fixed in v0.1.44.
//!
//!   Sessions whose process / session exited between duck/unduck just
//!   disappear from the enumeration and are silently dropped (no harm —
//!   the session is gone, there's nothing to restore).
//!
//!   Sessions that came into existence AFTER `duck()` (e.g. you opened
//!   YouTube mid-dictation) are not in our saved map → we leave them at
//!   whatever volume they currently have. Correct: they were never ducked.

use std::collections::HashMap;

use parking_lot::Mutex;
use windows::core::{Interface, PWSTR};
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    ISimpleAudioVolume, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows_sys::Win32::System::Threading::GetCurrentProcessId;

use crate::perf_log;

/// session_instance_id (unique per audio session, stable for its lifetime) →
/// original volume (0.0–1.0) at the time we ducked. Cleared each `unduck()`.
static SAVED_VOLUMES: Mutex<Option<HashMap<String, f32>>> = Mutex::new(None);

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

/// Read a `PWSTR` returned by a COM call into an owned `String`. Returns
/// an empty string if the pointer is null. Used for session instance IDs.
unsafe fn pwstr_to_string(ptr: PWSTR) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *ptr.0.add(len) != 0 {
        len += 1;
    }
    let slice = std::slice::from_raw_parts(ptr.0, len);
    String::from_utf16_lossy(slice)
}

/// Walk every active render device, every active session on each device.
/// `f` is called with (session_instance_id, simple_audio_volume) for each
/// non-Murmr session. The instance ID is what we key saved volumes by —
/// it's unique per session (NOT per process), so apps with multiple
/// sessions per PID (Chrome tabs, Discord channels, OBS scenes, etc.) get
/// their volumes saved + restored individually.
///
/// Short-circuits on COM errors at the device level so one bad device
/// doesn't kill the whole walk.
fn for_each_session<F: FnMut(&str, &ISimpleAudioVolume)>(mut f: F) {
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

                // Get extended interface so we can read process_id +
                // session instance identifier.
                let control2: IAudioSessionControl2 = match control.cast() {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                let pid = control2.GetProcessId().unwrap_or(0);
                if pid == 0 || pid == our_pid {
                    // pid 0 = system sounds session; skip those + our own.
                    continue;
                }

                // The session instance ID is unique per session for its
                // entire lifetime. If a session is destroyed + recreated
                // it gets a NEW id, which is what we want — the new
                // session was never ducked.
                let session_id_ptr = match control2.GetSessionInstanceIdentifier() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let session_id = pwstr_to_string(session_id_ptr);
                if session_id.is_empty() {
                    continue;
                }

                let vol: ISimpleAudioVolume = match control.cast() {
                    Ok(v) => v,
                    Err(_) => continue,
                };

                f(&session_id, &vol);
            }
        }
    }
}

pub fn duck(amount: f32) {
    {
        let guard = SAVED_VOLUMES.lock();
        if guard.is_some() {
            perf_log::append("[duck] already ducked, skipping");
            return;
        }
    }

    let mut saved: HashMap<String, f32> = HashMap::new();
    let factor = (1.0 - amount).clamp(0.0, 1.0);
    let mut count = 0;

    for_each_session(|session_id, vol| {
        unsafe {
            let current = vol.GetMasterVolume().unwrap_or(1.0);
            // Insert keyed by session id — never overwrites a previously-
            // saved session, since each session has a unique id.
            saved.insert(session_id.to_string(), current);
            let target = (current * factor).clamp(0.0, 1.0);
            // GUID context arg = caller's identifier so the OS can route
            // change-notifications back to us. Passing zero/null is fine
            // when we don't care.
            if let Err(e) = vol.SetMasterVolume(target, std::ptr::null()) {
                perf_log::append(&format!(
                    "[duck] session={session_id} SetMasterVolume failed: {e:?}"
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
    let saved = match SAVED_VOLUMES.lock().take() {
        Some(s) => s,
        None => {
            perf_log::append("[duck] unduck: not currently ducked, skipping");
            return;
        }
    };

    let mut restored = 0;
    let mut missing = 0;
    for_each_session(|session_id, vol| {
        if let Some(original) = saved.get(session_id) {
            unsafe {
                if let Err(e) = vol.SetMasterVolume(*original, std::ptr::null()) {
                    perf_log::append(&format!(
                        "[duck] session={session_id} restore failed: {e:?}"
                    ));
                } else {
                    restored += 1;
                }
            }
        }
    });

    // Any session in `saved` that didn't show up in the unduck walk —
    // its process or session was closed between duck and unduck. Nothing
    // to restore, but log the count so a perf.log paste can show us if
    // it's happening at unexpected rates.
    if restored < saved.len() {
        missing = saved.len() - restored;
    }
    perf_log::append(&format!(
        "[duck] restored {restored}/{} session(s) ({} missing — process/session ended)",
        saved.len(),
        missing,
    ));
}
