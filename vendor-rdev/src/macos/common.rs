#![allow(clippy::upper_case_acronyms)]
use crate::macos::keyboard::Keyboard;
use crate::rdev::{Button, Event, EventType};
use cocoa::base::id;
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType, EventField};
use lazy_static::lazy_static;
use std::convert::TryInto;
use std::os::raw::c_void;
use std::sync::Mutex;
use std::time::SystemTime;

use crate::macos::keycodes::key_from_code;

pub type CFMachPortRef = *const c_void;
pub type CFIndex = u64;
pub type CFAllocatorRef = id;
pub type CFRunLoopSourceRef = id;
pub type CFRunLoopRef = id;
pub type CFRunLoopMode = id;
pub type CGEventTapProxy = id;
pub type CGEventRef = CGEvent;

// https://developer.apple.com/documentation/coregraphics/cgeventtapplacement?language=objc
pub type CGEventTapPlacement = u32;
#[allow(non_upper_case_globals)]
pub const kCGHeadInsertEventTap: u32 = 0;

// https://developer.apple.com/documentation/coregraphics/cgeventtapoptions?language=objc
#[allow(non_upper_case_globals)]
#[repr(u32)]
pub enum CGEventTapOption {
    #[cfg(feature = "unstable_grab")]
    Default = 0,
    ListenOnly = 1,
}

pub static mut LAST_FLAGS: CGEventFlags = CGEventFlags::CGEventFlagNull;
lazy_static! {
    pub static ref KEYBOARD_STATE: Mutex<Keyboard> = Mutex::new(Keyboard::new().unwrap());
}

// https://developer.apple.com/documentation/coregraphics/cgeventmask?language=objc
pub type CGEventMask = u64;
#[allow(non_upper_case_globals)]
pub const kCGEventMaskForAllEvents: u64 = (1 << CGEventType::LeftMouseDown as u64)
    + (1 << CGEventType::LeftMouseUp as u64)
    + (1 << CGEventType::RightMouseDown as u64)
    + (1 << CGEventType::RightMouseUp as u64)
    + (1 << CGEventType::MouseMoved as u64)
    + (1 << CGEventType::LeftMouseDragged as u64)
    + (1 << CGEventType::RightMouseDragged as u64)
    + (1 << CGEventType::KeyDown as u64)
    + (1 << CGEventType::KeyUp as u64)
    + (1 << CGEventType::FlagsChanged as u64)
    + (1 << CGEventType::ScrollWheel as u64);

#[cfg(target_os = "macos")]
#[link(name = "Cocoa", kind = "framework")]
extern "C" {
    #[allow(improper_ctypes)]
    pub fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOption,
        eventsOfInterest: CGEventMask,
        callback: QCallback,
        user_info: id,
    ) -> CFMachPortRef;
    pub fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        tap: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;
    pub fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFRunLoopMode);
    pub fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    pub fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    pub fn CFRunLoopRun();

    pub static kCFRunLoopCommonModes: CFRunLoopMode;

}
pub type QCallback = unsafe extern "C" fn(
    proxy: CGEventTapProxy,
    _type: CGEventType,
    cg_event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

pub unsafe fn convert(
    _type: CGEventType,
    cg_event: &CGEvent,
    keyboard_state: &mut Keyboard,
) -> Option<Event> {
    let option_type = match _type {
        CGEventType::LeftMouseDown => Some(EventType::ButtonPress(Button::Left)),
        CGEventType::LeftMouseUp => Some(EventType::ButtonRelease(Button::Left)),
        CGEventType::RightMouseDown => Some(EventType::ButtonPress(Button::Right)),
        CGEventType::RightMouseUp => Some(EventType::ButtonRelease(Button::Right)),
        CGEventType::MouseMoved => {
            let point = cg_event.location();
            Some(EventType::MouseMove {
                x: point.x,
                y: point.y,
            })
        }
        CGEventType::KeyDown => {
            let code = cg_event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
            Some(EventType::KeyPress(key_from_code(code.try_into().ok()?)))
        }
        CGEventType::KeyUp => {
            let code = cg_event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
            Some(EventType::KeyRelease(key_from_code(code.try_into().ok()?)))
        }
        CGEventType::FlagsChanged => {
            let code = cg_event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE);
            let code: u16 = code.try_into().ok()?;
            let flags = cg_event.get_flags();
            // Murmr patch: decide press vs release from THIS key's own flag
            // bit instead of comparing the whole flag word to the last one
            // we saw. The old numeric compare broke whenever a flags event
            // was missed (tap re-armed, Secure Input, synthetic paste): the
            // stale LAST_FLAGS flipped the next press into a "release" and
            // the hotkey ignored the user. Caps Lock and unknown codes have
            // no per-key bit, so they keep the old compare.
            let pressed = match modifier_pressed(code, flags.bits()) {
                Some(p) => p,
                None => flags >= LAST_FLAGS,
            };
            LAST_FLAGS = flags;
            if pressed {
                Some(EventType::KeyPress(key_from_code(code)))
            } else {
                Some(EventType::KeyRelease(key_from_code(code)))
            }
        }
        CGEventType::ScrollWheel => {
            let delta_y =
                cg_event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_1);
            let delta_x =
                cg_event.get_integer_value_field(EventField::SCROLL_WHEEL_EVENT_POINT_DELTA_AXIS_2);
            Some(EventType::Wheel { delta_x, delta_y })
        }
        _ => None,
    };
    if let Some(event_type) = option_type {
        // Murmr patch: skip create_string_for_key to avoid dispatch_assert_queue
        // crash from TSMGetInputSourceProperty being called off the main thread
        // inside the CGEventTap callback on macOS 26+.
        let _ = &keyboard_state;
        let name: Option<String> = None;
        return Some(Event {
            event_type,
            time: SystemTime::now(),
            name,
        });
    }
    None
}

// Device-dependent modifier bits (IOKit NX_DEVICE*KEYMASK) and the
// device-independent class bits (kCGEventFlagMask*).
const NX_DEVICELCTLKEYMASK: u64 = 0x0000_0001;
const NX_DEVICELSHIFTKEYMASK: u64 = 0x0000_0002;
const NX_DEVICERSHIFTKEYMASK: u64 = 0x0000_0004;
const NX_DEVICELCMDKEYMASK: u64 = 0x0000_0008;
const NX_DEVICERCMDKEYMASK: u64 = 0x0000_0010;
const NX_DEVICELALTKEYMASK: u64 = 0x0000_0020;
const NX_DEVICERALTKEYMASK: u64 = 0x0000_0040;
const NX_DEVICERCTLKEYMASK: u64 = 0x0000_2000;
const NX_DEVICE_ANY: u64 = NX_DEVICELCTLKEYMASK
    | NX_DEVICELSHIFTKEYMASK
    | NX_DEVICERSHIFTKEYMASK
    | NX_DEVICELCMDKEYMASK
    | NX_DEVICERCMDKEYMASK
    | NX_DEVICELALTKEYMASK
    | NX_DEVICERALTKEYMASK
    | NX_DEVICERCTLKEYMASK;
const FLAG_SHIFT: u64 = 0x0002_0000;
const FLAG_CONTROL: u64 = 0x0004_0000;
const FLAG_ALTERNATE: u64 = 0x0008_0000;
const FLAG_COMMAND: u64 = 0x0010_0000;
const FLAG_SECONDARY_FN: u64 = 0x0080_0000;

/// Is the modifier with this keycode held, according to `flags` from its
/// own FlagsChanged event? `None` for keys with no per-key bit (Caps Lock,
/// unknown codes), where the caller falls back to the old compare.
///
/// Uses the left/right device bit when the keyboard reports device bits
/// (all Apple keyboards do). Some third-party keyboards only set the class
/// bit (e.g. "Alternate"), so fall back to that when no device bit is set.
pub fn modifier_pressed(code: u16, flags: u64) -> Option<bool> {
    let (device, class) = match code {
        59 => (NX_DEVICELCTLKEYMASK, FLAG_CONTROL),
        62 => (NX_DEVICERCTLKEYMASK, FLAG_CONTROL),
        56 => (NX_DEVICELSHIFTKEYMASK, FLAG_SHIFT),
        60 => (NX_DEVICERSHIFTKEYMASK, FLAG_SHIFT),
        55 => (NX_DEVICELCMDKEYMASK, FLAG_COMMAND),
        54 => (NX_DEVICERCMDKEYMASK, FLAG_COMMAND),
        58 => (NX_DEVICELALTKEYMASK, FLAG_ALTERNATE),
        61 => (NX_DEVICERALTKEYMASK, FLAG_ALTERNATE),
        63 => return Some(flags & FLAG_SECONDARY_FN != 0),
        _ => return None,
    };
    if flags & device != 0 {
        Some(true)
    } else if flags & NX_DEVICE_ANY != 0 {
        // Keyboard reports device bits, just not this key's: released.
        Some(false)
    } else {
        Some(flags & class != 0)
    }
}

#[cfg(test)]
mod modifier_tests {
    use super::modifier_pressed;

    #[test]
    fn right_option_press_and_release() {
        // Right Option down: Alternate + device R-alt + non-coalesced.
        assert_eq!(modifier_pressed(61, 0x0008_0140), Some(true));
        // Released, nothing else held.
        assert_eq!(modifier_pressed(61, 0x0000_0100), Some(false));
    }

    #[test]
    fn right_option_release_while_left_option_held() {
        // Left Option still down: class bit set, but R-alt device bit clear.
        assert_eq!(modifier_pressed(61, 0x0008_0120), Some(false));
    }

    #[test]
    fn right_option_release_while_shift_held() {
        // The old numeric compare got this wrong if a Shift event was missed.
        assert_eq!(modifier_pressed(61, 0x0002_0102), Some(false));
        assert_eq!(modifier_pressed(61, 0x000A_0142), Some(true));
    }

    #[test]
    fn keyboard_without_device_bits_uses_class_bit() {
        assert_eq!(modifier_pressed(61, 0x0008_0000), Some(true));
        assert_eq!(modifier_pressed(61, 0x0000_0000), Some(false));
    }

    #[test]
    fn caps_lock_has_no_per_key_bit() {
        assert_eq!(modifier_pressed(57, 0x0001_0000), None);
    }
}
