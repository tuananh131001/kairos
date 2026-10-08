use kairos_core::{FieldError, PausePreset, PauseSource, Settings};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;
pub const SOCKET_NAME: &str = "kairosd.sock";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ClientMessage {
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    #[serde(flatten)]
    pub request: Request,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    GetState,
    GetSettings,
    Subscribe,
    UpdateSettings {
        settings: Settings,
    },
    Pause {
        preset: PausePreset,
        source: PauseSource,
    },
    Resume,
    PostponeBreak,
    RestartCapture,
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ServerMessage {
    pub v: u32,
    #[serde(flatten)]
    pub event: Event,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityLabel {
    Active,
    Watching,
    Away,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseLabel {
    Running,
    OnBreak,
    Paused,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    Granted,
    Denied,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BreakEndReasonLabel {
    Completed,
    Postponed,
    CancelledByPause,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StateEvent {
    pub phase: PhaseLabel,
    pub activity: ActivityLabel,
    pub load_percent: f64,
    pub break_in_s: Option<u64>,
    pub break_remaining_s: Option<u64>,
    pub postponed: bool,
    pub paused_until: Option<i64>,
    pub today_worked_s: u64,
    pub permission: Permission,
    pub meeting_active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Event {
    Reply {
        id: Option<u64>,
        ok: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<ErrorBody>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        state: Option<StateEvent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        settings: Option<Settings>,
    },
    State(StateEvent),
    BreakStarted {
        text: String,
        remaining_s: u64,
    },
    BreakEnded {
        reason: BreakEndReasonLabel,
    },
    MeetingDetected,
    MeetingEnded,
}

impl ServerMessage {
    pub fn new(event: Event) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            event,
        }
    }
}

impl ClientMessage {
    pub fn new(id: Option<u64>, request: Request) -> Self {
        Self {
            v: PROTOCOL_VERSION,
            id,
            request,
        }
    }
}

#[derive(Debug)]
pub enum DecodeError {
    Json(serde_json::Error),
    Version(u32),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecodeError::Json(e) => write!(f, "invalid message: {e}"),
            DecodeError::Version(v) => write!(f, "unsupported protocol version {v}"),
        }
    }
}

impl std::error::Error for DecodeError {}

pub fn encode<T: Serialize>(message: &T) -> String {
    let mut line = serde_json::to_string(message).unwrap_or_default();
    line.push('\n');
    line
}

pub fn decode_client(line: &str) -> Result<ClientMessage, DecodeError> {
    let message: ClientMessage = serde_json::from_str(line.trim()).map_err(DecodeError::Json)?;
    if message.v != PROTOCOL_VERSION {
        return Err(DecodeError::Version(message.v));
    }
    Ok(message)
}

pub fn decode_server(line: &str) -> Result<ServerMessage, DecodeError> {
    let message: ServerMessage = serde_json::from_str(line.trim()).map_err(DecodeError::Json)?;
    if message.v != PROTOCOL_VERSION {
        return Err(DecodeError::Version(message.v));
    }
    Ok(message)
}
