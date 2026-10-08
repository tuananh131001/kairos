use core_foundation::base::{CFType, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;

use crate::ffi;

const HID_SYSTEM_STATE: i32 = 1;
const ANY_INPUT_EVENT: u32 = u32::MAX;

pub fn input_idle_seconds() -> f64 {
    unsafe { ffi::CGEventSourceSecondsSinceLastEventType(HID_SYSTEM_STATE, ANY_INPUT_EVENT) }
}

pub fn is_screen_locked() -> bool {
    let raw = unsafe { ffi::CGSessionCopyCurrentDictionary() };
    if raw.is_null() {
        return true;
    }
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_create_rule(raw) };
    let key = CFString::from_static_string("CGSSessionScreenIsLocked");
    dict.find(&key)
        .and_then(|v| v.downcast::<CFBoolean>())
        .is_some_and(bool::from)
}

pub fn screen_recording_permission() -> bool {
    unsafe { ffi::CGPreflightScreenCaptureAccess() }
}

pub fn request_screen_recording_permission() -> bool {
    unsafe { ffi::CGRequestScreenCaptureAccess() }
}

pub fn active_display_ids() -> Vec<u32> {
    let mut ids = [0u32; 32];
    let mut count = 0u32;
    let status =
        unsafe { ffi::CGGetActiveDisplayList(ids.len() as u32, ids.as_mut_ptr(), &mut count) };
    if status != 0 {
        return Vec::new();
    }
    let mut out = ids[..count as usize].to_vec();
    out.sort_unstable();
    out
}
