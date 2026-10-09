#![cfg(target_os = "macos")]

mod av;
mod ffi;
mod motion;
mod system;

pub use av::AvProbe;
pub use motion::{tile_luminance, MotionFrameAnalyzer, ScreenMotionProbe, TILE_COLUMNS, TILE_ROWS};
pub use system::{
    active_display_ids, input_idle_seconds, is_screen_locked, request_screen_recording_permission,
    screen_recording_permission,
};
