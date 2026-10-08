mod classifier;
mod engine;
mod meeting;
mod recorder;
mod settings;
mod types;

pub use classifier::{
    classify, ClassifierInput, MotionDetector, MOTION_AREA_THRESHOLD, MOTION_REQUIRED,
    MOTION_WINDOW,
};
pub use engine::{
    ActivePause, BreakChain, BreakEndReason, Engine, EngineError, EngineEvent, EngineState,
    Snapshot, POSTPONE_SECONDS, TICK_GAP_SECONDS,
};
pub use meeting::{AvState, MeetingDetector, MeetingEvent, MEETING_GAP_SECONDS};
pub use recorder::{ActivityRecorder, SegmentOp};
pub use settings::{FieldError, Settings, DEFAULT_REMINDER_TEXT};
pub use types::{ActivityKind, PausePreset, PauseSource, Phase, Timestamp};
