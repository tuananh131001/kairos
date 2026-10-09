use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Serialize};

pub type Timestamp = DateTime<Utc>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Active,
    Watching,
    Away,
}

impl ActivityKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActivityKind::Active => "active",
            ActivityKind::Watching => "watching",
            ActivityKind::Away => "away",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "active" => Some(ActivityKind::Active),
            "watching" => Some(ActivityKind::Watching),
            "away" => Some(ActivityKind::Away),
            _ => None,
        }
    }

    pub fn is_working(self) -> bool {
        matches!(self, ActivityKind::Active | ActivityKind::Watching)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Running,
    OnBreak,
    Paused,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Running => "running",
            Phase::OnBreak => "on_break",
            Phase::Paused => "paused",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "running" => Some(Phase::Running),
            "on_break" => Some(Phase::OnBreak),
            "paused" => Some(Phase::Paused),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PausePreset {
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "2h")]
    H2,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "1d")]
    D1,
}

impl PausePreset {
    pub fn duration(self) -> TimeDelta {
        match self {
            PausePreset::M30 => TimeDelta::minutes(30),
            PausePreset::H2 => TimeDelta::hours(2),
            PausePreset::H4 => TimeDelta::hours(4),
            PausePreset::D1 => TimeDelta::hours(24),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PausePreset::M30 => "30m",
            PausePreset::H2 => "2h",
            PausePreset::H4 => "4h",
            PausePreset::D1 => "1d",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "30m" => Some(PausePreset::M30),
            "2h" => Some(PausePreset::H2),
            "4h" => Some(PausePreset::H4),
            "1d" => Some(PausePreset::D1),
            _ => None,
        }
    }

    pub fn resets_load(self) -> bool {
        matches!(self, PausePreset::D1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseSource {
    Menu,
    MeetingNotification,
}

impl PauseSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PauseSource::Menu => "menu",
            PauseSource::MeetingNotification => "meeting_notification",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "menu" => Some(PauseSource::Menu),
            "meeting_notification" => Some(PauseSource::MeetingNotification),
            _ => None,
        }
    }
}
