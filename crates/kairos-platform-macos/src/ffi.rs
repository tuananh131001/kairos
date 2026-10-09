#![allow(non_upper_case_globals)]

use std::ffi::c_void;

pub type OSStatus = i32;
pub type ObjectId = u32;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PropertyAddress {
    pub selector: u32,
    pub scope: u32,
    pub element: u32,
}

pub const fn fourcc(code: &[u8; 4]) -> u32 {
    ((code[0] as u32) << 24) | ((code[1] as u32) << 16) | ((code[2] as u32) << 8) | code[3] as u32
}

pub const kSystemObject: ObjectId = 1;
pub const kPropertyDevices: u32 = fourcc(b"dev#");
pub const kPropertyIsRunningSomewhere: u32 = fourcc(b"gone");
pub const kAudioPropertyStreams: u32 = fourcc(b"stm#");
pub const kScopeGlobal: u32 = fourcc(b"glob");
pub const kScopeInput: u32 = fourcc(b"inpt");
pub const kElementMain: u32 = 0;

pub type AudioListenerProc =
    extern "C" fn(ObjectId, u32, *const PropertyAddress, *mut c_void) -> OSStatus;

#[link(name = "CoreAudio", kind = "framework")]
extern "C" {
    pub fn AudioObjectGetPropertyDataSize(
        object: ObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        out_size: *mut u32,
    ) -> OSStatus;
    pub fn AudioObjectGetPropertyData(
        object: ObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        io_size: *mut u32,
        out_data: *mut c_void,
    ) -> OSStatus;
    pub fn AudioObjectAddPropertyListener(
        object: ObjectId,
        address: *const PropertyAddress,
        listener: AudioListenerProc,
        client_data: *mut c_void,
    ) -> OSStatus;
    pub fn AudioObjectRemovePropertyListener(
        object: ObjectId,
        address: *const PropertyAddress,
        listener: AudioListenerProc,
        client_data: *mut c_void,
    ) -> OSStatus;
}

#[link(name = "CoreMediaIO", kind = "framework")]
extern "C" {
    pub fn CMIOObjectGetPropertyDataSize(
        object: ObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        out_size: *mut u32,
    ) -> OSStatus;
    pub fn CMIOObjectGetPropertyData(
        object: ObjectId,
        address: *const PropertyAddress,
        qualifier_size: u32,
        qualifier: *const c_void,
        data_size: u32,
        data_used: *mut u32,
        out_data: *mut c_void,
    ) -> OSStatus;
}

pub type CGDirectDisplayId = u32;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    pub fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
    pub fn CGSessionCopyCurrentDictionary() -> core_foundation::dictionary::CFDictionaryRef;
    pub fn CGPreflightScreenCaptureAccess() -> bool;
    pub fn CGRequestScreenCaptureAccess() -> bool;
    pub fn CGGetActiveDisplayList(
        max: u32,
        displays: *mut CGDirectDisplayId,
        count: *mut u32,
    ) -> i32;
}
