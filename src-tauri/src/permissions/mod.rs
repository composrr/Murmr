//! OS permission status checks for the first-run onboarding walkthrough.
//!
//! macOS gates three capabilities Murmr needs behind TCC (Transparency,
//! Consent, and Control):
//!   - **Microphone** — capture audio for transcription.
//!   - **Accessibility** — synthesize the Cmd+V paste into the focused app.
//!   - **Input Monitoring** — observe the global dictation hotkey.
//!
//! The onboarding wizard polls these so it can show a live
//! "Waiting… → Granted ✓" state per permission and auto-advance, instead
//! of the old blind "we can't tell, just trust us" hand-off.
//!
//! Windows / Linux don't gate these for ordinary desktop apps the same way,
//! so every check returns `NotApplicable` there and the onboarding skips the
//! whole permissions section.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionState {
    /// The capability is allowed — Murmr can use it now.
    Granted,
    /// The user explicitly denied it; needs a trip to System Settings.
    Denied,
    /// Never asked yet — the OS will prompt on first use.
    NotDetermined,
    /// Blocked by MDM / parental controls; user can't change it.
    Restricted,
    /// Couldn't determine (API returned something unexpected).
    Unknown,
    /// This platform doesn't gate the capability — treat as fine.
    NotApplicable,
}

// ---------------------------------------------------------------------------
// macOS implementations
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod imp {
    use super::PermissionState;

    /// AVAuthorizationStatus → our enum.
    ///   0 notDetermined · 1 restricted · 2 denied · 3 authorized
    pub fn microphone() -> PermissionState {
        use objc2::runtime::AnyObject;
        use objc2::{class, msg_send};

        // AVMediaTypeAudio is an `NSString *` constant exported by
        // AVFoundation; referencing it forces the framework to link.
        #[link(name = "AVFoundation", kind = "framework")]
        extern "C" {
            static AVMediaTypeAudio: *const AnyObject;
        }

        unsafe {
            let media_type: *const AnyObject = AVMediaTypeAudio;
            if media_type.is_null() {
                return PermissionState::Unknown;
            }
            let cls = class!(AVCaptureDevice);
            // authorizationStatusForMediaType: returns AVAuthorizationStatus
            // (NSInteger).
            let status: isize = msg_send![cls, authorizationStatusForMediaType: media_type];
            match status {
                0 => PermissionState::NotDetermined,
                1 => PermissionState::Restricted,
                2 => PermissionState::Denied,
                3 => PermissionState::Granted,
                _ => PermissionState::Unknown,
            }
        }
    }

    /// AXIsProcessTrusted(): true once the user has ticked Murmr under
    /// Privacy & Security → Accessibility.
    pub fn accessibility() -> PermissionState {
        // AXIsProcessTrusted is in the ApplicationServices umbrella —
        // already linked elsewhere (focus/macos.rs), declared here too so
        // this module is self-contained. Returns a `Boolean` (unsigned
        // char); read as u8 to keep the ABI exact.
        #[link(name = "ApplicationServices", kind = "framework")]
        extern "C" {
            fn AXIsProcessTrusted() -> u8;
        }
        unsafe {
            if AXIsProcessTrusted() != 0 {
                PermissionState::Granted
            } else {
                // AX has no "not determined" — it's trusted or it isn't.
                // We report Denied; the UI frames it as "not yet enabled."
                PermissionState::Denied
            }
        }
    }

    /// IOHIDCheckAccess(kIOHIDRequestTypeListenEvent).
    ///   0 granted · 1 denied · 2 unknown(not-determined)
    pub fn input_monitoring() -> PermissionState {
        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOHIDCheckAccess(request: u32) -> u32;
        }
        // kIOHIDRequestTypeListenEvent = 1 (observe events, which is what
        // our global hotkey grab does).
        const LISTEN_EVENT: u32 = 1;
        unsafe {
            match IOHIDCheckAccess(LISTEN_EVENT) {
                0 => PermissionState::Granted,
                1 => PermissionState::Denied,
                2 => PermissionState::NotDetermined,
                _ => PermissionState::Unknown,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Non-macOS stubs — these capabilities aren't TCC-gated for desktop apps.
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::PermissionState;
    pub fn microphone() -> PermissionState {
        PermissionState::NotApplicable
    }
    pub fn accessibility() -> PermissionState {
        PermissionState::NotApplicable
    }
    pub fn input_monitoring() -> PermissionState {
        PermissionState::NotApplicable
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn microphone_state() -> PermissionState {
    imp::microphone()
}
pub fn accessibility_state() -> PermissionState {
    imp::accessibility()
}
pub fn input_monitoring_state() -> PermissionState {
    imp::input_monitoring()
}
