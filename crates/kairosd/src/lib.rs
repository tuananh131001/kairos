mod daemon;
mod log;
mod server;

use std::path::PathBuf;

use kairos_core::{AvState, Timestamp};

pub use daemon::{Daemon, Outcome};
pub use server::{bind_socket, run, Command, TickAck};

pub const APP_BUNDLE_ID: &str = "com.kairos.app";

pub trait Probes: Send {
    fn input_idle_seconds(&mut self) -> f64;
    fn screen_locked(&mut self) -> bool;
    fn take_motion_ratio(&mut self) -> f64;
    fn av(&mut self) -> AvState;
    fn permission(&mut self) -> bool;
    fn start_sampling(&mut self) -> Result<(), String>;
    fn stop_sampling(&mut self);
    fn is_sampling(&self) -> bool;
    fn maintain(&mut self) {}
}

pub trait Clock: Send {
    fn now(&self) -> Timestamp;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        chrono::Utc::now()
    }
}

pub trait AppLauncher: Send {
    fn launch(&mut self);
}

pub struct OpenAppLauncher {
    pub bundle_id: String,
}

impl AppLauncher for OpenAppLauncher {
    fn launch(&mut self) {
        match std::process::Command::new("/usr/bin/open")
            .args(["-g", "-b", &self.bundle_id])
            .spawn()
        {
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
                log!("relaunching {}", self.bundle_id);
            }
            Err(err) => log!("failed to relaunch {}: {err}", self.bundle_id),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Paths {
    pub socket: PathBuf,
    pub db: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        let dir = kairos_store::support_dir();
        Self {
            socket: std::env::var_os("KAIROS_SOCKET")
                .map_or_else(|| dir.join(kairos_ipc::SOCKET_NAME), PathBuf::from),
            db: std::env::var_os("KAIROS_DB").map_or_else(|| dir.join("kairos.db"), PathBuf::from),
        }
    }
}

#[cfg(target_os = "macos")]
pub mod mac {
    use kairos_core::AvState;
    use kairos_platform_macos as platform;

    pub struct MacProbes {
        motion: platform::ScreenMotionProbe,
        av: platform::AvProbe,
        requested_permission: bool,
    }

    impl MacProbes {
        pub fn new(excluded_bundle_ids: Vec<String>) -> Self {
            Self {
                motion: platform::ScreenMotionProbe::new(excluded_bundle_ids),
                av: platform::AvProbe::new(),
                requested_permission: false,
            }
        }
    }

    impl super::Probes for MacProbes {
        fn input_idle_seconds(&mut self) -> f64 {
            platform::input_idle_seconds()
        }

        fn screen_locked(&mut self) -> bool {
            platform::is_screen_locked()
        }

        fn take_motion_ratio(&mut self) -> f64 {
            self.motion.take_changed_ratio()
        }

        fn av(&mut self) -> AvState {
            self.av.read()
        }

        fn permission(&mut self) -> bool {
            let granted = platform::screen_recording_permission();
            if !granted && !self.requested_permission {
                self.requested_permission = true;
                platform::request_screen_recording_permission();
            }
            granted
        }

        fn start_sampling(&mut self) -> Result<(), String> {
            self.motion.start()
        }

        fn stop_sampling(&mut self) {
            self.motion.stop();
        }

        fn is_sampling(&self) -> bool {
            self.motion.is_running()
        }

        fn maintain(&mut self) {
            if self.motion.displays_changed() {
                crate::log!("display configuration changed, restarting capture");
                if let Err(err) = self.motion.start() {
                    crate::log!("capture restart failed: {err}");
                }
            }
        }
    }
}
