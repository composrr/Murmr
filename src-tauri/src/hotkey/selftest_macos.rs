//! macOS hotkey self-test (feature `hotkey-selftest`, never in releases).
//!
//! With `MURMR_HOTKEY_SELFTEST=1`, presses Right Option through the OS
//! (CGEventPost at the HID level, so events travel the same path as a real
//! keyboard and hit our event tap) at a range of tap and hold lengths, then
//! exits. Practice mode is on, so nothing is pasted or saved. Results are
//! in perf.log as `[ctrl] DictationUp ... → Toggled / push-to-talk complete`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation, CGEventType};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use tauri::AppHandle;

use crate::perf_log;

const RIGHT_OPTION: u16 = 61;
const ESCAPE: u16 = 53;
const FLAGS_RIGHT_OPTION_DOWN: u64 = 0x0008_0140;
const FLAGS_NONE: u64 = 0x0000_0100;

fn post_flags(keycode: u16, flags: u64, down: bool) {
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        perf_log::append("[hk-selftest] CGEventSource failed");
        return;
    };
    let Ok(ev) = CGEvent::new_keyboard_event(src, keycode, down) else {
        perf_log::append("[hk-selftest] CGEvent create failed");
        return;
    };
    ev.set_type(CGEventType::FlagsChanged);
    ev.set_flags(CGEventFlags::from_bits_truncate(flags));
    ev.post(CGEventTapLocation::HID);
}

fn post_key(keycode: u16) {
    let Ok(src) = CGEventSource::new(CGEventSourceStateID::HIDSystemState) else {
        return;
    };
    for down in [true, false] {
        if let Ok(ev) = CGEvent::new_keyboard_event(src.clone(), keycode, down) {
            ev.post(CGEventTapLocation::HID);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn press_option(hold_ms: u64) {
    post_flags(RIGHT_OPTION, FLAGS_RIGHT_OPTION_DOWN, true);
    std::thread::sleep(Duration::from_millis(hold_ms));
    post_flags(RIGHT_OPTION, FLAGS_NONE, false);
}

fn ms(n: u64) {
    std::thread::sleep(Duration::from_millis(n));
}

pub fn spawn(app: AppHandle, practice: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        practice.store(true, Ordering::SeqCst);
        ms(5000); // let the app, hook and HUD finish starting
        perf_log::append("[hk-selftest] START");

        // Tap to start, tap to stop: the user's normal flow. Includes the
        // very first press after launch (the "first try" case).
        for hold in [40u64, 90, 150, 200] {
            perf_log::append(&format!("[hk-selftest] CASE tap {hold}ms then tap to stop"));
            press_option(hold);
            ms(1500);
            press_option(60);
            ms(2500); // transcription of room noise (practice: not pasted)
        }

        // Tap to start, Escape to cancel.
        perf_log::append("[hk-selftest] CASE tap 80ms then Esc");
        press_option(80);
        ms(1200);
        post_key(ESCAPE);
        ms(1500);

        // Hold for push-to-talk.
        for hold in [400u64, 1200] {
            perf_log::append(&format!("[hk-selftest] CASE hold {hold}ms"));
            press_option(hold);
            ms(2500);
        }

        perf_log::append("[hk-selftest] DONE");
        practice.store(false, Ordering::SeqCst);
        ms(500);
        app.exit(0);
    });
}
