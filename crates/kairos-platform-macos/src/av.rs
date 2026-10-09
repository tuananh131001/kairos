use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use kairos_core::AvState;

use crate::ffi::{self, ObjectId, PropertyAddress};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

static AUDIO_DIRTY: AtomicBool = AtomicBool::new(true);

extern "C" fn audio_changed(
    _object: ObjectId,
    _count: u32,
    _addresses: *const PropertyAddress,
    _data: *mut c_void,
) -> ffi::OSStatus {
    AUDIO_DIRTY.store(true, Ordering::Release);
    0
}

pub struct AvProbe {
    listened: Vec<ObjectId>,
    last_audio_poll: Option<Instant>,
    last_camera_poll: Option<Instant>,
    state: AvState,
}

impl Default for AvProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl AvProbe {
    pub fn new() -> Self {
        Self {
            listened: Vec::new(),
            last_audio_poll: None,
            last_camera_poll: None,
            state: AvState::default(),
        }
    }

    pub fn read(&mut self) -> AvState {
        let now = Instant::now();
        let audio_due = self
            .last_audio_poll
            .is_none_or(|t| now.duration_since(t) >= POLL_INTERVAL);
        if AUDIO_DIRTY.swap(false, Ordering::AcqRel) || audio_due {
            let devices = input_audio_devices();
            self.listen(&devices);
            self.state.mic = devices
                .iter()
                .any(|d| running_somewhere(*d, Backend::Audio));
            self.last_audio_poll = Some(now);
        }
        let camera_due = self
            .last_camera_poll
            .is_none_or(|t| now.duration_since(t) >= POLL_INTERVAL);
        if camera_due {
            self.state.camera = camera_devices()
                .iter()
                .any(|d| running_somewhere(*d, Backend::Camera));
            self.last_camera_poll = Some(now);
        }
        self.state
    }

    pub fn audio_listener_count(&self) -> usize {
        self.listened.len()
    }

    fn listen(&mut self, devices: &[ObjectId]) {
        if self.listened.is_empty() {
            let address = global(ffi::kPropertyDevices);
            unsafe {
                ffi::AudioObjectAddPropertyListener(
                    ffi::kSystemObject,
                    &address,
                    audio_changed,
                    std::ptr::null_mut(),
                );
            }
        }
        for device in devices {
            if self.listened.contains(device) {
                continue;
            }
            let address = global(ffi::kPropertyIsRunningSomewhere);
            let status = unsafe {
                ffi::AudioObjectAddPropertyListener(
                    *device,
                    &address,
                    audio_changed,
                    std::ptr::null_mut(),
                )
            };
            if status == 0 {
                self.listened.push(*device);
            }
        }
    }
}

impl Drop for AvProbe {
    fn drop(&mut self) {
        let address = global(ffi::kPropertyIsRunningSomewhere);
        for device in &self.listened {
            unsafe {
                ffi::AudioObjectRemovePropertyListener(
                    *device,
                    &address,
                    audio_changed,
                    std::ptr::null_mut(),
                );
            }
        }
        if !self.listened.is_empty() {
            let address = global(ffi::kPropertyDevices);
            unsafe {
                ffi::AudioObjectRemovePropertyListener(
                    ffi::kSystemObject,
                    &address,
                    audio_changed,
                    std::ptr::null_mut(),
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Backend {
    Audio,
    Camera,
}

fn global(selector: u32) -> PropertyAddress {
    PropertyAddress {
        selector,
        scope: ffi::kScopeGlobal,
        element: ffi::kElementMain,
    }
}

fn data_size(object: ObjectId, address: &PropertyAddress, backend: Backend) -> Option<u32> {
    let mut size = 0u32;
    let status = unsafe {
        match backend {
            Backend::Audio => {
                ffi::AudioObjectGetPropertyDataSize(object, address, 0, std::ptr::null(), &mut size)
            }
            Backend::Camera => {
                ffi::CMIOObjectGetPropertyDataSize(object, address, 0, std::ptr::null(), &mut size)
            }
        }
    };
    (status == 0).then_some(size)
}

fn object_list(object: ObjectId, address: &PropertyAddress, backend: Backend) -> Vec<ObjectId> {
    let Some(size) = data_size(object, address, backend) else {
        return Vec::new();
    };
    let count = size as usize / std::mem::size_of::<ObjectId>();
    if count == 0 {
        return Vec::new();
    }
    let mut ids = vec![0 as ObjectId; count];
    let status = unsafe {
        match backend {
            Backend::Audio => {
                let mut io = size;
                ffi::AudioObjectGetPropertyData(
                    object,
                    address,
                    0,
                    std::ptr::null(),
                    &mut io,
                    ids.as_mut_ptr().cast(),
                )
            }
            Backend::Camera => {
                let mut used = 0u32;
                ffi::CMIOObjectGetPropertyData(
                    object,
                    address,
                    0,
                    std::ptr::null(),
                    size,
                    &mut used,
                    ids.as_mut_ptr().cast(),
                )
            }
        }
    };
    if status != 0 {
        return Vec::new();
    }
    ids
}

fn input_audio_devices() -> Vec<ObjectId> {
    object_list(
        ffi::kSystemObject,
        &global(ffi::kPropertyDevices),
        Backend::Audio,
    )
    .into_iter()
    .filter(|device| {
        let address = PropertyAddress {
            selector: ffi::kAudioPropertyStreams,
            scope: ffi::kScopeInput,
            element: ffi::kElementMain,
        };
        data_size(*device, &address, Backend::Audio).is_some_and(|s| s > 0)
    })
    .collect()
}

fn camera_devices() -> Vec<ObjectId> {
    object_list(
        ffi::kSystemObject,
        &global(ffi::kPropertyDevices),
        Backend::Camera,
    )
}

fn running_somewhere(device: ObjectId, backend: Backend) -> bool {
    let address = global(ffi::kPropertyIsRunningSomewhere);
    let mut value: u32 = 0;
    let size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        match backend {
            Backend::Audio => {
                let mut io = size;
                ffi::AudioObjectGetPropertyData(
                    device,
                    &address,
                    0,
                    std::ptr::null(),
                    &mut io,
                    (&mut value as *mut u32).cast(),
                )
            }
            Backend::Camera => {
                let mut used = 0u32;
                ffi::CMIOObjectGetPropertyData(
                    device,
                    &address,
                    0,
                    std::ptr::null(),
                    size,
                    &mut used,
                    (&mut value as *mut u32).cast(),
                )
            }
        }
    };
    status == 0 && value != 0
}
