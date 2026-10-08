use chrono::TimeDelta;
use serde::{Deserialize, Serialize};

use crate::settings::{FieldError, Settings};
use crate::types::{ActivityKind, PausePreset, PauseSource, Phase, Timestamp};

pub const POSTPONE_SECONDS: i64 = 300;
pub const TICK_GAP_SECONDS: i64 = 5;
const FULL: f64 = 100.0;
const EPSILON: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ActivePause {
    pub preset: PausePreset,
    pub source: PauseSource,
    pub frozen_load_percent: f64,
    pub started_at: Timestamp,
    pub until_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BreakChain {
    pub started_at: Timestamp,
    pub postpone_count: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineState {
    pub load_percent: f64,
    pub phase: Phase,
    pub not_before: Option<Timestamp>,
    pub last_tick: Option<Timestamp>,
    pub pause: Option<ActivePause>,
    pub break_chain: Option<BreakChain>,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            load_percent: 0.0,
            phase: Phase::Running,
            not_before: None,
            last_tick: None,
            pause: None,
            break_chain: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BreakEndReason {
    Completed,
    Postponed { not_before: Timestamp },
    CancelledByPause,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    BreakStarted {
        at: Timestamp,
        remaining_s: u64,
        chain_started_at: Timestamp,
        postpone_count: u32,
        resumed_chain: bool,
    },
    BreakEnded {
        at: Timestamp,
        reason: BreakEndReason,
        postpone_count: u32,
    },
    PauseStarted(ActivePause),
    PauseEnded {
        at: Timestamp,
        early: bool,
        load_percent: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    NotOnBreak,
    NotPaused,
    InvalidSettings(Vec<FieldError>),
}

impl std::fmt::Display for EngineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EngineError::NotOnBreak => write!(f, "no break in progress"),
            EngineError::NotPaused => write!(f, "not paused"),
            EngineError::InvalidSettings(errors) => {
                let joined: Vec<_> = errors.iter().map(|e| e.message.as_str()).collect();
                write!(f, "{}", joined.join(" "))
            }
        }
    }
}

impl std::error::Error for EngineError {}

#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub phase: Phase,
    pub load_percent: f64,
    pub break_in_s: Option<f64>,
    pub break_remaining_s: Option<f64>,
    pub postponed: bool,
    pub paused_until: Option<Timestamp>,
}

#[derive(Clone, Debug)]
pub struct Engine {
    settings: Settings,
    state: EngineState,
}

impl Engine {
    pub fn new(settings: Settings) -> Self {
        Self::with_state(settings, EngineState::default())
    }

    pub fn with_state(settings: Settings, state: EngineState) -> Self {
        Self { settings, state }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn state(&self) -> &EngineState {
        &self.state
    }

    pub fn phase(&self) -> Phase {
        self.state.phase
    }

    pub fn load_percent(&self) -> f64 {
        self.state.load_percent
    }

    pub fn is_paused(&self) -> bool {
        self.state.phase == Phase::Paused
    }

    pub fn is_postponed(&self) -> bool {
        self.state.phase == Phase::Running && self.state.break_chain.is_some()
    }

    pub fn tick_gap(&self, now: Timestamp) -> bool {
        self.state
            .last_tick
            .is_some_and(|last| now - last > TimeDelta::seconds(TICK_GAP_SECONDS))
    }

    pub fn hold(&mut self, now: Timestamp) {
        self.state.last_tick = Some(now);
    }

    pub fn tick(&mut self, now: Timestamp, activity: ActivityKind) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let Some(last) = self.state.last_tick else {
            self.state.last_tick = Some(now);
            return events;
        };
        if now <= last {
            self.state.last_tick = Some(now);
            return events;
        }
        let gap = now - last > TimeDelta::seconds(TICK_GAP_SECONDS);
        let mut from = last;
        if let Some(pause) = self.state.pause.clone() {
            if now < pause.until_at {
                self.state.last_tick = Some(now);
                return events;
            }
            events.push(self.end_pause(pause.until_at, false));
            from = pause.until_at.max(last);
        }
        let effective = if gap { ActivityKind::Away } else { activity };
        let dt = seconds_between(from, now);
        self.advance(now, dt, effective, &mut events);
        self.state.last_tick = Some(now);
        events
    }

    fn advance(
        &mut self,
        now: Timestamp,
        dt: f64,
        activity: ActivityKind,
        events: &mut Vec<EngineEvent>,
    ) {
        match self.state.phase {
            Phase::Paused => {}
            Phase::Running => {
                let delta = if activity.is_working() {
                    self.settings.rise_per_second() * dt
                } else {
                    -self.settings.fall_per_second() * dt
                };
                self.state.load_percent = (self.state.load_percent + delta).clamp(0.0, FULL);
                if self.state.load_percent <= EPSILON {
                    self.state.load_percent = 0.0;
                    if let Some(chain) = self.state.break_chain.take() {
                        self.state.not_before = None;
                        events.push(EngineEvent::BreakEnded {
                            at: now,
                            reason: BreakEndReason::Completed,
                            postpone_count: chain.postpone_count,
                        });
                    }
                }
                if self.state.load_percent >= FULL - EPSILON
                    && self.state.not_before.is_none_or(|nb| now >= nb)
                {
                    events.push(self.start_break(now));
                }
            }
            Phase::OnBreak => {
                let delta = self.settings.fall_per_second() * dt;
                self.state.load_percent = (self.state.load_percent - delta).max(0.0);
                if self.state.load_percent <= EPSILON {
                    self.state.load_percent = 0.0;
                    self.state.phase = Phase::Running;
                    self.state.not_before = None;
                    let chain = self.state.break_chain.take();
                    events.push(EngineEvent::BreakEnded {
                        at: now,
                        reason: BreakEndReason::Completed,
                        postpone_count: chain.map_or(0, |c| c.postpone_count),
                    });
                }
            }
        }
    }

    fn start_break(&mut self, now: Timestamp) -> EngineEvent {
        self.state.load_percent = FULL;
        self.state.phase = Phase::OnBreak;
        self.state.not_before = None;
        let resumed_chain = self.state.break_chain.is_some();
        let chain = self.state.break_chain.get_or_insert(BreakChain {
            started_at: now,
            postpone_count: 0,
        });
        EngineEvent::BreakStarted {
            at: now,
            remaining_s: u64::from(self.settings.break_seconds),
            chain_started_at: chain.started_at,
            postpone_count: chain.postpone_count,
            resumed_chain,
        }
    }

    pub fn postpone(&mut self, now: Timestamp) -> Result<Vec<EngineEvent>, EngineError> {
        if self.state.phase != Phase::OnBreak {
            return Err(EngineError::NotOnBreak);
        }
        let not_before = now + TimeDelta::seconds(POSTPONE_SECONDS);
        self.state.phase = Phase::Running;
        self.state.not_before = Some(not_before);
        let chain = self.state.break_chain.get_or_insert(BreakChain {
            started_at: now,
            postpone_count: 0,
        });
        chain.postpone_count += 1;
        let postpone_count = chain.postpone_count;
        self.state.last_tick = Some(now);
        Ok(vec![EngineEvent::BreakEnded {
            at: now,
            reason: BreakEndReason::Postponed { not_before },
            postpone_count,
        }])
    }

    pub fn pause(
        &mut self,
        now: Timestamp,
        preset: PausePreset,
        source: PauseSource,
    ) -> Vec<EngineEvent> {
        let mut events = Vec::new();
        let frozen = match self.state.pause.take() {
            Some(previous) => {
                events.push(EngineEvent::PauseEnded {
                    at: now,
                    early: true,
                    load_percent: previous.frozen_load_percent,
                });
                previous.frozen_load_percent
            }
            None => self.state.load_percent,
        };
        if let Some(chain) = self.state.break_chain.take() {
            events.push(EngineEvent::BreakEnded {
                at: now,
                reason: BreakEndReason::CancelledByPause,
                postpone_count: chain.postpone_count,
            });
        } else if self.state.phase == Phase::OnBreak {
            events.push(EngineEvent::BreakEnded {
                at: now,
                reason: BreakEndReason::CancelledByPause,
                postpone_count: 0,
            });
        }
        let pause = ActivePause {
            preset,
            source,
            frozen_load_percent: frozen,
            started_at: now,
            until_at: now + preset.duration(),
        };
        self.state.phase = Phase::Paused;
        self.state.not_before = None;
        self.state.load_percent = frozen;
        self.state.pause = Some(pause.clone());
        self.state.last_tick = Some(now);
        events.push(EngineEvent::PauseStarted(pause));
        events
    }

    pub fn resume(&mut self, now: Timestamp) -> Result<Vec<EngineEvent>, EngineError> {
        if self.state.pause.is_none() {
            return Err(EngineError::NotPaused);
        }
        let event = self.end_pause(now, true);
        self.state.last_tick = Some(now);
        Ok(vec![event])
    }

    fn end_pause(&mut self, at: Timestamp, early: bool) -> EngineEvent {
        let pause = self.state.pause.take();
        let load = match &pause {
            Some(p) if p.preset.resets_load() => 0.0,
            Some(p) => p.frozen_load_percent,
            None => self.state.load_percent,
        };
        self.state.phase = Phase::Running;
        self.state.load_percent = load;
        self.state.not_before = None;
        self.state.break_chain = None;
        EngineEvent::PauseEnded {
            at,
            early,
            load_percent: load,
        }
    }

    pub fn apply_settings(&mut self, new: Settings) -> Result<(), EngineError> {
        new.validate().map_err(EngineError::InvalidSettings)?;
        self.settings = new;
        Ok(())
    }

    pub fn break_in_seconds(&self, now: Timestamp) -> Option<f64> {
        if self.state.phase != Phase::Running {
            return None;
        }
        let to_full = (FULL - self.state.load_percent).max(0.0) / self.settings.rise_per_second();
        let floor = self
            .state
            .not_before
            .map_or(0.0, |nb| seconds_between(now, nb).max(0.0));
        Some(to_full.max(floor))
    }

    pub fn break_remaining_seconds(&self) -> Option<f64> {
        (self.state.phase == Phase::OnBreak)
            .then(|| self.state.load_percent / FULL * f64::from(self.settings.break_seconds))
    }

    pub fn snapshot(&self, now: Timestamp) -> Snapshot {
        Snapshot {
            phase: self.state.phase,
            load_percent: self.state.load_percent,
            break_in_s: self.break_in_seconds(now),
            break_remaining_s: self.break_remaining_seconds(),
            postponed: self.is_postponed(),
            paused_until: self.state.pause.as_ref().map(|p| p.until_at),
        }
    }
}

fn seconds_between(from: Timestamp, to: Timestamp) -> f64 {
    (to - from).num_milliseconds() as f64 / 1000.0
}
