#![allow(improper_ctypes_definitions)]
use crate::macos::common::*;
use crate::rdev::{Event, GrabError};
use cocoa::base::nil;
use cocoa::foundation::NSAutoreleasePool;
use core_graphics::event::{CGEventTapLocation, CGEventType};
use std::os::raw::c_void;

static mut GLOBAL_CALLBACK: Option<Box<dyn FnMut(Event) -> Option<Event>>> = None;
// The active event tap, stashed so `raw_callback` can re-arm it if macOS
// disables it. Set once in `grab()` before the run loop starts.
static mut EVENT_TAP: CFMachPortRef = std::ptr::null();

// CGEventType discriminants macOS delivers when it turns our tap off. They
// aren't real input events — they're a signal that we must call
// CGEventTapEnable(tap, true) to keep receiving events. (CGEventType is
// #[repr(u32)] but doesn't derive PartialEq, so we compare the raw value.)
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

#[link(name = "Cocoa", kind = "framework")]
extern "C" {}

unsafe extern "C" fn raw_callback(
    _proxy: CGEventTapProxy,
    _type: CGEventType,
    cg_event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    // macOS disables an event tap if a callback runs too long
    // (kCGEventTapDisabledByTimeout) or during a burst of user input
    // (kCGEventTapDisabledByUserInput). It notifies us by invoking the
    // callback with these out-of-band types; without re-enabling here the
    // hotkey silently dies until the app restarts — the classic "I have to
    // press it two or three times" symptom. Re-arm the tap and move on.
    let raw_type = _type as u32;
    if raw_type == TAP_DISABLED_BY_TIMEOUT || raw_type == TAP_DISABLED_BY_USER_INPUT {
        if !EVENT_TAP.is_null() {
            CGEventTapEnable(EVENT_TAP, true);
        }
        return cg_event;
    }

    // println!("Event ref {:?}", cg_event_ptr);
    // let cg_event: CGEvent = transmute_copy::<*mut c_void, CGEvent>(&cg_event_ptr);
    let opt = KEYBOARD_STATE.lock();
    if let Ok(mut keyboard) = opt {
        if let Some(event) = convert(_type, &cg_event, &mut keyboard) {
            if let Some(callback) = &mut GLOBAL_CALLBACK {
                if callback(event).is_none() {
                    cg_event.set_type(CGEventType::Null);
                }
            }
        }
    }
    cg_event
}

pub fn grab<T>(callback: T) -> Result<(), GrabError>
where
    T: FnMut(Event) -> Option<Event> + 'static,
{
    unsafe {
        GLOBAL_CALLBACK = Some(Box::new(callback));
        let _pool = NSAutoreleasePool::new(nil);
        let tap = CGEventTapCreate(
            // Murmr patch: Session instead of HID. HID-level taps are denied
            // on macOS 26+ when any app claims Secure Input (password fields,
            // 1Password, terminal in some configs). Session is sufficient for
            // global hotkey detection and works with standard Accessibility +
            // Input Monitoring grants.
            CGEventTapLocation::Session,
            kCGHeadInsertEventTap,
            CGEventTapOption::Default,
            kCGEventMaskForAllEvents,
            raw_callback,
            nil,
        );
        if tap.is_null() {
            return Err(GrabError::EventTapError);
        }
        let _loop = CFMachPortCreateRunLoopSource(nil, tap, 0);
        if _loop.is_null() {
            return Err(GrabError::LoopSourceError);
        }

        // Stash the tap so raw_callback can re-enable it if macOS ever
        // disables it (see TAP_DISABLED_* handling above).
        EVENT_TAP = tap;

        let current_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(current_loop, _loop, kCFRunLoopCommonModes);

        CGEventTapEnable(tap, true);
        CFRunLoopRun();
    }
    Ok(())
}
