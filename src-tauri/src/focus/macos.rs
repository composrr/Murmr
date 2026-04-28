//! Detect the focused UI element on macOS via AXUIElement.
//!
//! Mirrors the Windows `uia_focused_element_rect` so the HUD can position
//! itself just below the active text input. Requires Accessibility permission
//! (already granted for the global hotkey listener).

use std::ffi::c_void;

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::geometry::{CGPoint, CGSize};

use super::ScreenRect;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXUIElementCreateSystemWide() -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> i32;
    fn AXValueGetValue(value: CFTypeRef, the_type: u32, value_ptr: *mut c_void) -> u8;
}

// AX attribute names are header-only `#define kAXFooAttribute CFSTR("AXFoo")`,
// not exported symbols. Construct CFStrings at runtime instead.
fn ax_attr(name: &str) -> CFString {
    CFString::new(name)
}

/// Codes from `AXValue.h`.
const K_AX_VALUE_CG_POINT_TYPE: u32 = 1;
const K_AX_VALUE_CG_SIZE_TYPE: u32 = 2;

/// Backing scale factor of the main NSScreen — converts AX points to the
/// physical pixels Tauri's window APIs use.
fn main_screen_scale() -> f64 {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    unsafe {
        let cls = objc2::class!(NSScreen);
        let main_screen: *mut AnyObject = msg_send![cls, mainScreen];
        if main_screen.is_null() {
            return 1.0;
        }
        let scale: f64 = msg_send![main_screen, backingScaleFactor];
        if scale <= 0.0 {
            1.0
        } else {
            scale
        }
    }
}

/// Returns the focused element's bounding rect in physical pixels, with
/// origin at the top-left of the main display's coordinate space (matches
/// Tauri's `Monitor::position()` convention).
///
/// AX returns points (1 point = 2 physical pixels on Retina); we scale up so
/// the controller's positioning math, which is already in physical pixels for
/// the Windows path, doesn't need a per-platform branch.
pub fn ax_focused_element_rect() -> Option<ScreenRect> {
    let focused_attr = ax_attr("AXFocusedUIElement");
    let position_attr = ax_attr("AXPosition");
    let size_attr = ax_attr("AXSize");

    unsafe {
        let sys_wide = AXUIElementCreateSystemWide();
        if sys_wide.is_null() {
            return None;
        }

        let mut focused: CFTypeRef = std::ptr::null();
        let err = AXUIElementCopyAttributeValue(
            sys_wide,
            focused_attr.as_concrete_TypeRef(),
            &mut focused,
        );
        CFRelease(sys_wide);
        if err != 0 || focused.is_null() {
            return None;
        }

        let mut position_val: CFTypeRef = std::ptr::null();
        let mut size_val: CFTypeRef = std::ptr::null();
        let pos_err = AXUIElementCopyAttributeValue(
            focused,
            position_attr.as_concrete_TypeRef(),
            &mut position_val,
        );
        let size_err = AXUIElementCopyAttributeValue(
            focused,
            size_attr.as_concrete_TypeRef(),
            &mut size_val,
        );
        CFRelease(focused);

        if pos_err != 0 || size_err != 0 || position_val.is_null() || size_val.is_null() {
            if !position_val.is_null() {
                CFRelease(position_val);
            }
            if !size_val.is_null() {
                CFRelease(size_val);
            }
            return None;
        }

        let mut pos = CGPoint { x: 0.0, y: 0.0 };
        let mut size = CGSize {
            width: 0.0,
            height: 0.0,
        };
        let pos_ok = AXValueGetValue(
            position_val,
            K_AX_VALUE_CG_POINT_TYPE,
            &mut pos as *mut _ as *mut c_void,
        );
        let size_ok = AXValueGetValue(
            size_val,
            K_AX_VALUE_CG_SIZE_TYPE,
            &mut size as *mut _ as *mut c_void,
        );
        CFRelease(position_val);
        CFRelease(size_val);

        if pos_ok == 0 || size_ok == 0 {
            return None;
        }

        // Reject obviously bogus rects (some apps return zero-size for
        // off-screen or never-laid-out elements).
        if size.width <= 0.0 || size.height <= 0.0 {
            return None;
        }

        let scale = main_screen_scale();
        Some(ScreenRect {
            x: (pos.x * scale).round() as i32,
            y: (pos.y * scale).round() as i32,
            width: (size.width * scale).round() as i32,
            height: (size.height * scale).round() as i32,
        })
    }
}
