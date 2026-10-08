use std::time::{Duration, Instant};

use kairos_platform_macos::*;

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let permission = screen_recording_permission();
    println!(
        "screen_recording_permission={permission} displays={:?}",
        active_display_ids()
    );
    let mut motion = ScreenMotionProbe::new(vec!["com.kairos.app".into()]);
    if let Err(err) = motion.start() {
        println!("screen capture unavailable: {err}");
    }
    let mut av = AvProbe::new();
    let started = Instant::now();
    println!(
        "{:>5} {:>7} {:<7} {:>13} {:>7} {:<6} {}",
        "t", "idle_s", "locked", "changed_ratio", "frames", "mic", "camera"
    );
    for t in 1..=seconds {
        std::thread::sleep(Duration::from_secs(1));
        if motion.displays_changed() {
            println!("displays changed, restarting capture");
            let _ = motion.start();
        }
        let state = av.read();
        println!(
            "{t:>5} {:>7.1} {:<7} {:>13.3} {:>7} {:<6} {}",
            input_idle_seconds(),
            is_screen_locked(),
            motion.take_changed_ratio(),
            motion.frames_received(),
            state.mic,
            state.camera,
        );
    }
    motion.stop();
    println!(
        "done in {:.1}s, audio listeners={}",
        started.elapsed().as_secs_f64(),
        av.audio_listener_count()
    );
}
