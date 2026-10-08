use serde::{Deserialize, Serialize};

pub const DEFAULT_REMINDER_TEXT: &str = "Time to rest your eyes. Stand up and stretch.";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub work_minutes: u32,
    pub break_seconds: u32,
    pub reminder_text: String,
    pub idle_threshold_seconds: u32,
    pub meeting_detection_enabled: bool,
    pub meeting_notify_delay_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    fn new(field: &str, message: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            work_minutes: 50,
            break_seconds: 600,
            reminder_text: DEFAULT_REMINDER_TEXT.to_string(),
            idle_threshold_seconds: 60,
            meeting_detection_enabled: true,
            meeting_notify_delay_seconds: 30,
        }
    }
}

impl Settings {
    pub const WORK_MINUTES_RANGE: (u32, u32) = (1, 240);
    pub const BREAK_SECONDS_RANGE: (u32, u32) = (10, 3600);
    pub const REMINDER_TEXT_MAX_CHARS: usize = 200;
    pub const MEETING_DELAY_RANGE: (u32, u32) = (10, 300);

    pub fn validate(&self) -> Result<(), Vec<FieldError>> {
        let mut errors = Vec::new();
        let (lo, hi) = Self::WORK_MINUTES_RANGE;
        if !(lo..=hi).contains(&self.work_minutes) {
            errors.push(FieldError::new(
                "work_minutes",
                "Work duration must be between 1 and 240 minutes.",
            ));
        }
        let (lo, hi) = Self::BREAK_SECONDS_RANGE;
        if !(lo..=hi).contains(&self.break_seconds) {
            errors.push(FieldError::new(
                "break_seconds",
                "Break duration must be between 0:10 and 60:00.",
            ));
        }
        let chars = self.reminder_text.chars().count();
        if self.reminder_text.trim().is_empty() || chars > Self::REMINDER_TEXT_MAX_CHARS {
            errors.push(FieldError::new(
                "reminder_text",
                "Reminder text must be 1 to 200 characters.",
            ));
        }
        if self.idle_threshold_seconds == 0 {
            errors.push(FieldError::new(
                "idle_threshold_seconds",
                "Idle threshold must be positive.",
            ));
        }
        let (lo, hi) = Self::MEETING_DELAY_RANGE;
        if !(lo..=hi).contains(&self.meeting_notify_delay_seconds) {
            errors.push(FieldError::new(
                "meeting_notify_delay_seconds",
                "Meeting notification delay must be between 10 seconds and 5 minutes.",
            ));
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    pub fn rise_per_second(&self) -> f64 {
        100.0 / (f64::from(self.work_minutes.max(1)) * 60.0)
    }

    pub fn fall_per_second(&self) -> f64 {
        100.0 / f64::from(self.break_seconds.max(1))
    }
}
