use chrono::{Local, TimeDelta};
use kairos_core::{
    classify, ActivityKind, ActivityRecorder, BreakEndReason, ClassifierInput, Engine, EngineError,
    EngineEvent, MeetingDetector, MeetingEvent, MotionDetector, PauseSource, Phase, SegmentOp,
    Timestamp,
};
use kairos_ipc::{
    ActivityLabel, BreakEndReasonLabel, ErrorBody, Event, Permission, PhaseLabel, Request,
    StateEvent,
};
use kairos_store::Store;

use crate::{log, AppLauncher, Clock, Probes};

const CHECKPOINT_SECONDS: i64 = 10;
const PERMISSION_RECHECK_GRANTED_SECONDS: i64 = 30;
const PERMISSION_RECHECK_DENIED_SECONDS: i64 = 2;
const RELAUNCH_COOLDOWN_SECONDS: i64 = 30;

pub struct Outcome {
    pub reply: Event,
    pub followups: Vec<Event>,
    pub shutdown: bool,
}

pub struct Daemon<P: Probes, C: Clock, L: AppLauncher> {
    store: Store,
    engine: Engine,
    probes: P,
    clock: C,
    launcher: L,
    motion: MotionDetector,
    meeting: MeetingDetector,
    recorder: ActivityRecorder,
    open_segment: Option<i64>,
    meeting_id: Option<i64>,
    activity: Option<ActivityKind>,
    permission: bool,
    last_permission_check: Option<Timestamp>,
    last_checkpoint: Timestamp,
    last_launch: Option<Timestamp>,
    outbox: Vec<Event>,
}

impl<P: Probes, C: Clock, L: AppLauncher> Daemon<P, C, L> {
    pub fn new(store: Store, probes: P, clock: C, launcher: L) -> Result<Self, String> {
        let now = clock.now();
        let settings = store.load_settings().map_err(|e| e.to_string())?;
        let recovered = store
            .recover(settings.clone(), now)
            .map_err(|e| e.to_string())?;
        log!(
            "restored phase={} load={:.1}% (closed {} dangling segments, {} recovery events)",
            recovered.engine.phase().as_str(),
            recovered.engine.load_percent(),
            recovered.closed_segments,
            recovered.events.len()
        );
        Ok(Self {
            meeting: MeetingDetector::new(settings.meeting_notify_delay_seconds),
            store,
            engine: recovered.engine,
            probes,
            clock,
            launcher,
            motion: MotionDetector::new(),
            recorder: ActivityRecorder::new(),
            open_segment: None,
            meeting_id: None,
            activity: None,
            permission: false,
            last_permission_check: None,
            last_checkpoint: now,
            last_launch: None,
            outbox: Vec::new(),
        })
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    pub fn take_outbox(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.outbox)
    }

    pub fn tick(&mut self, subscribers: usize) {
        let now = self.clock.now();
        self.refresh_permission(now);
        let was_paused = self.engine.is_paused();
        if !self.permission {
            self.engine.hold(now);
            self.stop_recording(now);
            self.activity = None;
            self.sync_sampling();
            self.maybe_checkpoint(now, false);
            return;
        }
        if was_paused {
            self.motion.reset();
        } else {
            self.probes.maintain();
            self.motion.push(self.probes.take_motion_ratio());
        }
        let span_start = self.engine.state().last_tick.unwrap_or(now);
        let kind = classify(&ClassifierInput {
            screen_locked: self.probes.screen_locked(),
            tick_gap: self.engine.tick_gap(now),
            input_idle_seconds: self.probes.input_idle_seconds(),
            idle_threshold_seconds: self.engine.settings().idle_threshold_seconds,
            motion: self.motion.is_motion(),
        });
        let events = self.engine.tick(now, kind);
        if self.engine.is_paused() {
            self.activity = None;
            self.stop_recording(now);
        } else {
            self.activity = Some(kind);
            let ops = self.recorder.record(span_start.min(now), kind);
            self.apply_segment_ops(ops);
        }
        let phase_changed = !events.is_empty();
        self.handle_engine_events(&events, None, subscribers);
        self.update_meeting(now, subscribers);
        self.sync_sampling();
        self.maybe_checkpoint(now, phase_changed);
    }

    pub fn handle(&mut self, request: Request, subscribers: usize) -> Outcome {
        let now = self.clock.now();
        let mut followups = Vec::new();
        let mut shutdown = false;
        let result: Result<(Option<StateEvent>, Option<kairos_core::Settings>), ErrorBody> =
            match request {
                Request::GetState => Ok((Some(self.state(now)), None)),
                Request::Subscribe => {
                    if self.engine.phase() == Phase::OnBreak {
                        followups.push(self.break_started_event());
                    }
                    Ok((Some(self.state(now)), None))
                }
                Request::GetSettings => Ok((None, Some(self.engine.settings().clone()))),
                Request::UpdateSettings { settings } => self.update_settings(settings, now),
                Request::Pause { preset, source } => {
                    let meeting_link = (source == PauseSource::MeetingNotification)
                        .then_some(self.meeting_id)
                        .flatten();
                    let events = self.engine.pause(now, preset, source);
                    self.stop_recording(now);
                    self.activity = None;
                    self.handle_engine_events(&events, meeting_link, subscribers);
                    if let Some(id) = meeting_link {
                        self.log_err(self.store.set_meeting_action(id, preset));
                    }
                    self.sync_sampling();
                    self.checkpoint(now);
                    log!("paused for {} from {}", preset.as_str(), source.as_str());
                    Ok((Some(self.state(now)), None))
                }
                Request::Resume => match self.engine.resume(now) {
                    Ok(events) => {
                        self.handle_engine_events(&events, None, subscribers);
                        self.sync_sampling();
                        self.checkpoint(now);
                        log!("resumed at {:.1}%", self.engine.load_percent());
                        Ok((Some(self.state(now)), None))
                    }
                    Err(err) => Err(engine_error(err)),
                },
                Request::PostponeBreak => match self.engine.postpone(now) {
                    Ok(events) => {
                        self.handle_engine_events(&events, None, subscribers);
                        self.checkpoint(now);
                        Ok((Some(self.state(now)), None))
                    }
                    Err(err) => Err(engine_error(err)),
                },
                Request::RestartCapture => {
                    self.last_permission_check = None;
                    self.refresh_permission(now);
                    self.probes.stop_sampling();
                    self.sync_sampling();
                    Ok((Some(self.state(now)), None))
                }
                Request::Shutdown => {
                    shutdown = true;
                    Ok((None, None))
                }
            };
        let reply = match result {
            Ok((state, settings)) => Event::Reply {
                id: None,
                ok: true,
                error: None,
                state,
                settings,
            },
            Err(error) => Event::Reply {
                id: None,
                ok: false,
                error: Some(error),
                state: None,
                settings: None,
            },
        };
        Outcome {
            reply,
            followups,
            shutdown,
        }
    }

    pub fn current_state(&mut self) -> StateEvent {
        let now = self.clock.now();
        self.state(now)
    }

    pub fn state(&mut self, now: Timestamp) -> StateEvent {
        let snapshot = self.engine.snapshot(now);
        let today = self
            .store
            .today_worked_seconds(&Local, now)
            .unwrap_or_else(|e| {
                log!("today total query failed: {e}");
                0
            });
        StateEvent {
            phase: match snapshot.phase {
                Phase::Running => PhaseLabel::Running,
                Phase::OnBreak => PhaseLabel::OnBreak,
                Phase::Paused => PhaseLabel::Paused,
            },
            activity: match (snapshot.phase, self.activity) {
                (Phase::Paused, _) => ActivityLabel::Paused,
                (_, Some(ActivityKind::Active)) => ActivityLabel::Active,
                (_, Some(ActivityKind::Watching)) => ActivityLabel::Watching,
                _ => ActivityLabel::Away,
            },
            load_percent: (snapshot.load_percent * 100.0).round() / 100.0,
            break_in_s: snapshot.break_in_s.map(|s| s.ceil() as u64),
            break_remaining_s: snapshot.break_remaining_s.map(|s| s.ceil() as u64),
            postponed: snapshot.postponed,
            paused_until: snapshot.paused_until.map(|t| t.timestamp()),
            today_worked_s: today.max(0) as u64,
            permission: if self.permission {
                Permission::Granted
            } else {
                Permission::Denied
            },
            meeting_active: self.meeting.is_nudged(),
        }
    }

    pub fn shutdown(&mut self) {
        let now = self.clock.now();
        self.stop_recording(now);
        self.probes.stop_sampling();
        if let Some(id) = self.meeting_id.take() {
            self.log_err(self.store.end_meeting(id, now, false, false));
        }
        self.checkpoint(now);
        log!("shutdown complete");
    }

    fn update_settings(
        &mut self,
        settings: kairos_core::Settings,
        now: Timestamp,
    ) -> Result<(Option<StateEvent>, Option<kairos_core::Settings>), ErrorBody> {
        self.engine
            .apply_settings(settings.clone())
            .map_err(engine_error)?;
        self.store
            .save_settings(&settings, now)
            .map_err(|e| ErrorBody {
                code: "storage".into(),
                message: e.to_string(),
                fields: Vec::new(),
            })?;
        self.meeting
            .set_delay(settings.meeting_notify_delay_seconds);
        log!(
            "settings saved: work={}m break={}s",
            settings.work_minutes,
            settings.break_seconds
        );
        Ok((Some(self.state(now)), Some(settings)))
    }

    fn refresh_permission(&mut self, now: Timestamp) {
        let interval = if self.permission {
            PERMISSION_RECHECK_GRANTED_SECONDS
        } else {
            PERMISSION_RECHECK_DENIED_SECONDS
        };
        let due = self
            .last_permission_check
            .is_none_or(|t| now - t >= TimeDelta::seconds(interval) || now < t);
        if !due {
            return;
        }
        self.last_permission_check = Some(now);
        let granted = self.probes.permission();
        if granted != self.permission {
            log!(
                "screen recording permission {}",
                if granted { "granted" } else { "missing" }
            );
            self.permission = granted;
        }
    }

    fn sync_sampling(&mut self) {
        let want = self.permission && !self.engine.is_paused();
        if want && !self.probes.is_sampling() {
            if let Err(err) = self.probes.start_sampling() {
                log!("screen sampling failed to start: {err}");
            }
        } else if !want && self.probes.is_sampling() {
            self.probes.stop_sampling();
            self.motion.reset();
        }
    }

    fn update_meeting(&mut self, now: Timestamp, subscribers: usize) {
        let av = self.probes.av();
        let enabled = self.engine.settings().meeting_detection_enabled;
        match self
            .meeting
            .update(now, av, enabled, self.engine.is_paused())
        {
            Some(MeetingEvent::Started {
                started_at,
                detected_at,
                notify,
                mic,
                camera,
            }) => {
                let notified_at = notify.then_some(detected_at);
                match self
                    .store
                    .insert_meeting(started_at, mic, camera, notified_at)
                {
                    Ok(id) => self.meeting_id = Some(id),
                    Err(e) => log!("meeting insert failed: {e}"),
                }
                log!("meeting detected (mic={mic} camera={camera} notify={notify})");
                if notify {
                    self.outbox.push(Event::MeetingDetected);
                    self.relaunch_if_needed(now, subscribers);
                }
            }
            Some(MeetingEvent::Ended {
                ended_at,
                mic_used,
                camera_used,
            }) => {
                if let Some(id) = self.meeting_id.take() {
                    self.log_err(self.store.end_meeting(id, ended_at, mic_used, camera_used));
                }
                log!("meeting ended");
                self.outbox.push(Event::MeetingEnded);
            }
            None => {}
        }
    }

    fn handle_engine_events(
        &mut self,
        events: &[EngineEvent],
        pause_meeting_id: Option<i64>,
        subscribers: usize,
    ) {
        if events.is_empty() {
            return;
        }
        self.log_err(self.store.apply_engine_events(events, pause_meeting_id));
        let now = self.clock.now();
        for event in events {
            match event {
                EngineEvent::BreakStarted {
                    at, postpone_count, ..
                } => {
                    log!(
                        "break started at {} (postpone_count={postpone_count}, subscribers={subscribers})",
                        at.to_rfc3339()
                    );
                    self.outbox.push(self.break_started_event());
                    self.relaunch_if_needed(now, subscribers);
                }
                EngineEvent::BreakEnded { reason, .. } => {
                    let label = match reason {
                        BreakEndReason::Completed => BreakEndReasonLabel::Completed,
                        BreakEndReason::Postponed { .. } => BreakEndReasonLabel::Postponed,
                        BreakEndReason::CancelledByPause => BreakEndReasonLabel::CancelledByPause,
                    };
                    log!("break ended: {label:?}");
                    self.outbox.push(Event::BreakEnded { reason: label });
                }
                EngineEvent::PauseEnded { early: false, .. } => {
                    log!("pause expired, load {:.1}%", self.engine.load_percent());
                }
                _ => {}
            }
        }
    }

    fn break_started_event(&self) -> Event {
        Event::BreakStarted {
            text: self.engine.settings().reminder_text.clone(),
            remaining_s: self
                .engine
                .break_remaining_seconds()
                .map_or(0, |s| s.ceil() as u64),
        }
    }

    fn relaunch_if_needed(&mut self, now: Timestamp, subscribers: usize) {
        if subscribers > 0 {
            return;
        }
        let cooled = self
            .last_launch
            .is_none_or(|t| now - t >= TimeDelta::seconds(RELAUNCH_COOLDOWN_SECONDS));
        if cooled {
            self.last_launch = Some(now);
            self.launcher.launch();
        }
    }

    fn apply_segment_ops(&mut self, ops: Vec<SegmentOp>) {
        for op in ops {
            match op {
                SegmentOp::Close { at } => {
                    if let Some(id) = self.open_segment.take() {
                        self.log_err(self.store.close_segment(id, at));
                    }
                }
                SegmentOp::Open { kind, at } => match self.store.open_segment(kind, at) {
                    Ok(id) => self.open_segment = Some(id),
                    Err(e) => log!("segment open failed: {e}"),
                },
            }
        }
    }

    fn stop_recording(&mut self, now: Timestamp) {
        let ops = self.recorder.stop(now);
        self.apply_segment_ops(ops);
    }

    fn maybe_checkpoint(&mut self, now: Timestamp, force: bool) {
        if force || now - self.last_checkpoint >= TimeDelta::seconds(CHECKPOINT_SECONDS) {
            self.checkpoint(now);
        }
    }

    fn checkpoint(&mut self, now: Timestamp) {
        self.last_checkpoint = now;
        self.log_err(self.store.checkpoint(self.engine.state(), now));
    }

    fn log_err<T>(&self, result: kairos_store::Result<T>) {
        if let Err(e) = result {
            log!("storage error: {e}");
        }
    }
}

fn engine_error(err: EngineError) -> ErrorBody {
    match err {
        EngineError::InvalidSettings(fields) => ErrorBody {
            code: "validation".into(),
            message: "Invalid settings.".into(),
            fields,
        },
        EngineError::NotOnBreak => ErrorBody {
            code: "not_on_break".into(),
            message: err.to_string(),
            fields: Vec::new(),
        },
        EngineError::NotPaused => ErrorBody {
            code: "not_paused".into(),
            message: err.to_string(),
            fields: Vec::new(),
        },
    }
}
