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
//!   On `duck()` we save `(process_id → MAX original_volume across that
//!   PID's sessions)`. `unduck()` re-enumerates and restores every visible
//!   session of each saved PID to that max; PIDs with NO visible session
//!   get a retry tail at +2s/+10s/+30s (fullscreen games tear down and
//!   recreate audio sessions on scene transitions, and Windows PERSISTS
//!   the last-set per-app volume — so giving up at unduck time left games
//!   permanently quiet until the user fixed the mixer by hand).
//!
//!   Why MAX over PID: multi-device routing (Voicemeeter exposes a
//!   dozen-plus render devices) and multi-bus apps give one process many
//!   sessions at different volumes; last-wins saved whichever enumerated
//!   last (often a quiet sub-bus) and restored the loud master DOWN to it.
//!   Max never under-restores. (First shipped v0.1.52; reverted on a test
//!   later invalidated by the concurrent AVX-512 crash; re-applied
//!   v0.1.58 together with the retry tail.)
//!
//!   v0.1.44 tried true per-session keying via session_instance_id (PWSTR
//!   from GetSessionInstanceIdentifier) but the PWSTR handling crashed
//!   mid-dictation (STATUS_ILLEGAL_INSTRUCTION); PID+max needs no unsafe
//!   string juggling.
//!
//!   Sessions whose process exited between duck/unduck drop out of the
//!   retry tail after the final pass and are logged.
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
            // Save the MAX volume seen across all of a PID's sessions.
            // Multi-device routing setups (Voicemeeter exposes a dozen-plus
            // render devices) and multi-bus apps (Apex master + sub-mixes,
            // Chrome tabs, Discord channels) give one process MANY sessions
            // at different volumes. A plain insert meant whichever session
            // enumerated LAST won the saved slot — often a quieter sub-bus
            // — so unduck restored the app's loud master DOWN to that value
            // and the user's game audio stayed stuck quiet. Saving the max
            // means restore never lands below any session's original;
            // worst case we slightly over-restore a manually-dimmed
            // sibling session, which beats permanently under-restoring.
            //
            // (First shipped in v0.1.52, reverted after a test that — in
            // hindsight — was invalidated by the concurrent AVX-512 crash.
            // Re-applied in v0.1.58 with the retry pass below.)
            let prior = saved.get(&pid).copied().unwrap_or(0.0);
            saved.insert(pid, current.max(prior));
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
        "[duck] ducked {count} session(s) across {} PID(s) by {:.0}%",
        saved.len(),
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

    let remaining = restore_pass(&saved);
    perf_log::append(&format!(
        "[duck] restored {}/{} PID(s) on first pass",
        saved.len() - remaining.len(),
        saved.len(),
    ));

    // Sessions that weren't enumerable right now get a retry tail.
    // Fullscreen games tear down + recreate audio sessions on scene
    // transitions / alt-tab (especially with exclusive-mode audio), so
    // the session we ducked may simply not EXIST at this instant — but
    // Windows persists the last-set per-app session volume, so if we
    // give up the app comes back STILL DUCKED and stays that way until
    // the user fixes it by hand in the volume mixer. Retrying at +2s /
    // +10s / +30s catches the session when it reappears.
    if !remaining.is_empty() {
        perf_log::append(&format!(
            "[duck] {} PID(s) had no visible session — scheduling restore retries",
            remaining.len(),
        ));
        std::thread::Builder::new()
            .name("murmr-unduck-retry".into())
            .spawn(move || {
                let mut pending = remaining;
                for delay_s in [2u64, 10, 30] {
                    std::thread::sleep(std::time::Duration::from_secs(delay_s));
                    // A new duck started while we were waiting → it took a
                    // fresh snapshot that supersedes ours; stop retrying so
                    // we don't fight it.
                    if SAVED_VOLUMES.lock().is_some() {
                        perf_log::append(
                            "[duck] retry abandoned — a new duck cycle started",
                        );
                        return;
                    }
                    pending = restore_pass(&pending);
                    if pending.is_empty() {
                        perf_log::append(&format!(
                            "[duck] retry at +{delay_s}s restored all remaining session(s)",
                        ));
                        return;
                    }
                    perf_log::append(&format!(
                        "[duck] retry at +{delay_s}s: {} PID(s) still not visible",
                        pending.len(),
                    ));
                }
                perf_log::append(&format!(
                    "[duck] giving up on {} PID(s) after retries (process likely exited)",
                    pending.len(),
                ));
            })
            .ok();
    }
}

/// One restoration sweep: walk current sessions, restore any whose PID is
/// in `targets`, and return the subset of `targets` that had NO visible
/// session this pass (candidates for retry). Restore failures (COM error
/// on a visible session) are logged but NOT retried — the session was
/// there, the set just failed, and hammering it won't help.
fn restore_pass(targets: &HashMap<u32, f32>) -> HashMap<u32, f32> {
    let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for_each_session(|pid, vol| {
        if let Some(original) = targets.get(&pid) {
            seen.insert(pid);
            unsafe {
                if let Err(e) = vol.SetMasterVolume(*original, std::ptr::null()) {
                    perf_log::append(&format!("[duck] pid={pid} restore failed: {e:?}"));
                }
            }
        }
    });
    targets
        .iter()
        .filter(|(pid, _)| !seen.contains(pid))
        .map(|(pid, vol)| (*pid, *vol))
        .collect()
}
