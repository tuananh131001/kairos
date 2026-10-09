use crate::types::ActivityKind;

pub const MOTION_WINDOW: usize = 5;
pub const MOTION_REQUIRED: usize = 4;
pub const MOTION_AREA_THRESHOLD: f64 = 0.03;

#[derive(Clone, Debug, Default)]
pub struct MotionDetector {
    samples: [f64; MOTION_WINDOW],
    len: usize,
    next: usize,
}

impl MotionDetector {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, changed_ratio: f64) {
        self.samples[self.next] = changed_ratio;
        self.next = (self.next + 1) % MOTION_WINDOW;
        self.len = (self.len + 1).min(MOTION_WINDOW);
    }

    pub fn is_motion(&self) -> bool {
        self.samples[..self.len]
            .iter()
            .filter(|r| **r >= MOTION_AREA_THRESHOLD)
            .count()
            >= MOTION_REQUIRED
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClassifierInput {
    pub screen_locked: bool,
    pub tick_gap: bool,
    pub input_idle_seconds: f64,
    pub idle_threshold_seconds: u32,
    pub motion: bool,
}

pub fn classify(input: &ClassifierInput) -> ActivityKind {
    if input.screen_locked || input.tick_gap {
        ActivityKind::Away
    } else if input.input_idle_seconds < f64::from(input.idle_threshold_seconds) {
        ActivityKind::Active
    } else if input.motion {
        ActivityKind::Watching
    } else {
        ActivityKind::Away
    }
}
