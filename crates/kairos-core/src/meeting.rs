use chrono::TimeDelta;

use crate::types::Timestamp;

pub const MEETING_GAP_SECONDS: i64 = 300;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AvState {
    pub mic: bool,
    pub camera: bool,
}

impl AvState {
    pub fn in_use(self) -> bool {
        self.mic || self.camera
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum MeetingEvent {
    Started {
        started_at: Timestamp,
        detected_at: Timestamp,
        notify: bool,
        mic: bool,
        camera: bool,
    },
    Ended {
        ended_at: Timestamp,
        mic_used: bool,
        camera_used: bool,
    },
}

#[derive(Clone, Debug, PartialEq)]
enum State {
    Idle,
    Pending {
        since: Timestamp,
        mic: bool,
        camera: bool,
    },
    Active {
        off_since: Option<Timestamp>,
        mic_used: bool,
        camera_used: bool,
        notified: bool,
    },
}

#[derive(Clone, Debug)]
pub struct MeetingDetector {
    delay: TimeDelta,
    state: State,
}

impl MeetingDetector {
    pub fn new(delay_seconds: u32) -> Self {
        Self {
            delay: TimeDelta::seconds(i64::from(delay_seconds)),
            state: State::Idle,
        }
    }

    pub fn set_delay(&mut self, delay_seconds: u32) {
        self.delay = TimeDelta::seconds(i64::from(delay_seconds));
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, State::Active { .. })
    }

    pub fn is_nudged(&self) -> bool {
        matches!(self.state, State::Active { notified: true, .. })
    }

    pub fn update(
        &mut self,
        now: Timestamp,
        av: AvState,
        enabled: bool,
        paused: bool,
    ) -> Option<MeetingEvent> {
        if !enabled {
            return self.reset(now);
        }
        match self.state.clone() {
            State::Idle => {
                if av.in_use() {
                    self.state = State::Pending {
                        since: now,
                        mic: av.mic,
                        camera: av.camera,
                    };
                    return self.update_pending(now, paused);
                }
                None
            }
            State::Pending { since, mic, camera } => {
                if !av.in_use() {
                    self.state = State::Idle;
                    return None;
                }
                self.state = State::Pending {
                    since,
                    mic: mic || av.mic,
                    camera: camera || av.camera,
                };
                self.update_pending(now, paused)
            }
            State::Active {
                off_since,
                mic_used,
                camera_used,
                notified,
            } => {
                let mic_used = mic_used || av.mic;
                let camera_used = camera_used || av.camera;
                if av.in_use() {
                    self.state = State::Active {
                        off_since: None,
                        mic_used,
                        camera_used,
                        notified,
                    };
                    return None;
                }
                let off_since = off_since.unwrap_or(now);
                if now - off_since >= TimeDelta::seconds(MEETING_GAP_SECONDS) {
                    self.state = State::Idle;
                    return Some(MeetingEvent::Ended {
                        ended_at: off_since,
                        mic_used,
                        camera_used,
                    });
                }
                self.state = State::Active {
                    off_since: Some(off_since),
                    mic_used,
                    camera_used,
                    notified,
                };
                None
            }
        }
    }

    fn update_pending(&mut self, now: Timestamp, paused: bool) -> Option<MeetingEvent> {
        let State::Pending { since, mic, camera } = self.state else {
            return None;
        };
        if now - since < self.delay {
            return None;
        }
        let notify = !paused;
        self.state = State::Active {
            off_since: None,
            mic_used: mic,
            camera_used: camera,
            notified: notify,
        };
        Some(MeetingEvent::Started {
            started_at: since,
            detected_at: now,
            notify,
            mic,
            camera,
        })
    }

    fn reset(&mut self, now: Timestamp) -> Option<MeetingEvent> {
        let previous = std::mem::replace(&mut self.state, State::Idle);
        match previous {
            State::Active {
                off_since,
                mic_used,
                camera_used,
                ..
            } => Some(MeetingEvent::Ended {
                ended_at: off_since.unwrap_or(now),
                mic_used,
                camera_used,
            }),
            _ => None,
        }
    }
}
