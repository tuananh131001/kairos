use chrono::{TimeDelta, TimeZone, Utc};
use kairos_core::*;

struct FakeClock {
    now: Timestamp,
}

impl FakeClock {
    fn new() -> Self {
        Self {
            now: Utc.with_ymd_and_hms(2026, 1, 5, 9, 0, 0).unwrap(),
        }
    }

    fn advance(&mut self, secs: i64) -> Timestamp {
        self.now += TimeDelta::seconds(secs);
        self.now
    }
}

fn started_engine(clock: &FakeClock, load: f64) -> Engine {
    let state = EngineState {
        load_percent: load,
        last_tick: Some(clock.now),
        ..EngineState::default()
    };
    Engine::with_state(Settings::default(), state)
}

fn run(
    engine: &mut Engine,
    clock: &mut FakeClock,
    secs: i64,
    kind: ActivityKind,
) -> Vec<EngineEvent> {
    let mut events = Vec::new();
    for _ in 0..secs {
        let now = clock.advance(1);
        events.extend(engine.tick(now, kind));
    }
    events
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected {expected}, got {actual}"
    );
}

fn break_started(events: &[EngineEvent]) -> Vec<&EngineEvent> {
    events
        .iter()
        .filter(|e| matches!(e, EngineEvent::BreakStarted { .. }))
        .collect()
}

#[test]
fn m1_active_25_minutes_reaches_50_percent() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 0.0);
    run(&mut engine, &mut clock, 25 * 60, ActivityKind::Active);
    assert_close(engine.load_percent(), 50.0);
    assert_close(engine.break_in_seconds(clock.now).unwrap(), 25.0 * 60.0);
}

#[test]
fn m2_away_three_minutes_from_50_is_20() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 50.0);
    run(&mut engine, &mut clock, 180, ActivityKind::Away);
    assert_close(engine.load_percent(), 20.0);
}

#[test]
fn m3_away_floors_at_zero() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 30.0);
    run(&mut engine, &mut clock, 600, ActivityKind::Away);
    assert_eq!(engine.load_percent(), 0.0);
}

#[test]
fn m4_watching_rises_like_active() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 60.0);
    run(&mut engine, &mut clock, 300, ActivityKind::Watching);
    assert_close(engine.load_percent(), 70.0);
}

#[test]
fn m4_classifier_reports_watching_for_video_without_input() {
    let mut motion = MotionDetector::new();
    for _ in 0..5 {
        motion.push(0.25);
    }
    let kind = classify(&ClassifierInput {
        screen_locked: false,
        tick_gap: false,
        input_idle_seconds: 120.0,
        idle_threshold_seconds: 60,
        motion: motion.is_motion(),
    });
    assert_eq!(kind, ActivityKind::Watching);
}

#[test]
fn m5_sleep_gap_decays_to_zero() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 60.0);
    assert!(engine.tick_gap(clock.now + TimeDelta::hours(2)));
    let now = clock.advance(2 * 3600);
    engine.tick(now, ActivityKind::Active);
    assert_eq!(engine.load_percent(), 0.0);
}

#[test]
fn m5_small_gap_is_not_a_sleep() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 60.0);
    let now = clock.advance(5);
    engine.tick(now, ActivityKind::Active);
    assert_close(engine.load_percent(), 60.0 + 5.0 * 100.0 / 3000.0);
}

#[test]
fn m6_settings_change_keeps_load_and_changes_rate() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 50.0);
    let new = Settings {
        work_minutes: 25,
        ..Settings::default()
    };
    engine.apply_settings(new).unwrap();
    assert_eq!(engine.load_percent(), 50.0);
    let events = run(&mut engine, &mut clock, 60, ActivityKind::Active);
    assert!(events.is_empty());
    assert_close(engine.load_percent(), 54.0);
}

#[test]
fn m6_saving_at_high_load_never_triggers_a_break() {
    let clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.0);
    engine
        .apply_settings(Settings {
            work_minutes: 1,
            ..Settings::default()
        })
        .unwrap();
    assert_eq!(engine.phase(), Phase::Running);
    assert_eq!(engine.load_percent(), 99.0);
}

#[test]
fn m7_invalid_settings_rejected_and_unchanged() {
    let mut engine = Engine::new(Settings::default());
    let zero_work = Settings {
        work_minutes: 0,
        ..Settings::default()
    };
    let short_break = Settings {
        break_seconds: 5,
        ..Settings::default()
    };
    match engine.apply_settings(zero_work) {
        Err(EngineError::InvalidSettings(errors)) => assert_eq!(errors[0].field, "work_minutes"),
        other => panic!("unexpected {other:?}"),
    }
    match engine.apply_settings(short_break) {
        Err(EngineError::InvalidSettings(errors)) => assert_eq!(errors[0].field, "break_seconds"),
        other => panic!("unexpected {other:?}"),
    }
    assert_eq!(engine.settings(), &Settings::default());
}

#[test]
fn settings_validation_ranges() {
    let ok = |s: Settings| s.validate().is_ok();
    assert!(ok(Settings::default()));
    assert!(ok(Settings {
        work_minutes: 240,
        break_seconds: 10,
        ..Settings::default()
    }));
    assert!(!ok(Settings {
        work_minutes: 241,
        ..Settings::default()
    }));
    assert!(!ok(Settings {
        break_seconds: 3601,
        ..Settings::default()
    }));
    assert!(!ok(Settings {
        reminder_text: String::new(),
        ..Settings::default()
    }));
    assert!(ok(Settings {
        reminder_text: "é".repeat(200),
        ..Settings::default()
    }));
    assert!(!ok(Settings {
        reminder_text: "a".repeat(201),
        ..Settings::default()
    }));
    assert!(!ok(Settings {
        meeting_notify_delay_seconds: 9,
        ..Settings::default()
    }));
    assert!(!ok(Settings {
        meeting_notify_delay_seconds: 301,
        ..Settings::default()
    }));
}

#[test]
fn b1_break_starts_when_meter_reaches_100() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.97);
    let events = run(&mut engine, &mut clock, 1, ActivityKind::Active);
    assert_eq!(engine.phase(), Phase::OnBreak);
    match &events[..] {
        [EngineEvent::BreakStarted { remaining_s, .. }] => assert_eq!(*remaining_s, 600),
        other => panic!("unexpected {other:?}"),
    }
    assert_close(engine.break_remaining_seconds().unwrap(), 600.0);
}

#[test]
fn b2_full_break_completes() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    let events = run(&mut engine, &mut clock, 600, ActivityKind::Active);
    assert_eq!(engine.load_percent(), 0.0);
    assert_eq!(engine.phase(), Phase::Running);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded {
            reason: BreakEndReason::Completed,
            ..
        }
    )));
}

#[test]
fn b3_postpone_at_full_load() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    let events = engine.postpone(clock.now).unwrap();
    assert!(matches!(
        events[0],
        EngineEvent::BreakEnded {
            reason: BreakEndReason::Postponed { .. },
            postpone_count: 1,
            ..
        }
    ));
    assert_eq!(engine.phase(), Phase::Running);
    assert!(engine.is_postponed());
    assert_eq!(engine.load_percent(), 100.0);
    assert_close(engine.break_in_seconds(clock.now).unwrap(), 300.0);
}

#[test]
fn b4_postponed_break_returns_after_five_minutes() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    engine.postpone(clock.now).unwrap();
    let early = run(&mut engine, &mut clock, 299, ActivityKind::Active);
    assert!(break_started(&early).is_empty());
    assert_eq!(engine.load_percent(), 100.0);
    let events = run(&mut engine, &mut clock, 1, ActivityKind::Active);
    match &events[..] {
        [EngineEvent::BreakStarted {
            remaining_s,
            postpone_count,
            resumed_chain,
            ..
        }] => {
            assert_eq!(*remaining_s, 600);
            assert_eq!(*postpone_count, 1);
            assert!(*resumed_chain);
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn b5_postpone_at_40_percent_waits_for_full_meter() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    run(&mut engine, &mut clock, 360, ActivityKind::Active);
    assert_close(engine.load_percent(), 40.0);
    assert_close(engine.break_remaining_seconds().unwrap(), 240.0);
    engine.postpone(clock.now).unwrap();
    let before = run(&mut engine, &mut clock, 30 * 60 - 2, ActivityKind::Active);
    assert!(break_started(&before).is_empty());
    let after = run(&mut engine, &mut clock, 4, ActivityKind::Active);
    assert_eq!(break_started(&after).len(), 1);
}

#[test]
fn b6_postpone_has_no_limit() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    for i in 1..=5 {
        let events = engine.postpone(clock.now).unwrap();
        assert!(
            matches!(events[0], EngineEvent::BreakEnded { postpone_count, .. } if postpone_count == i)
        );
        let returned = run(&mut engine, &mut clock, 300, ActivityKind::Active);
        assert_eq!(break_started(&returned).len(), 1);
    }
}

#[test]
fn postpone_outside_break_is_rejected() {
    let clock = FakeClock::new();
    let mut engine = started_engine(&clock, 10.0);
    assert_eq!(engine.postpone(clock.now), Err(EngineError::NotOnBreak));
}

#[test]
fn postponed_break_resolved_by_resting_to_zero() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    engine.postpone(clock.now).unwrap();
    let events = run(&mut engine, &mut clock, 600, ActivityKind::Away);
    assert!(!engine.is_postponed());
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded {
            reason: BreakEndReason::Completed,
            ..
        }
    )));
}

#[test]
fn p1_pause_freezes_load() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 80.0);
    engine.pause(clock.now, PausePreset::M30, PauseSource::Menu);
    assert_eq!(engine.phase(), Phase::Paused);
    run(&mut engine, &mut clock, 600, ActivityKind::Active);
    assert_eq!(engine.load_percent(), 80.0);
    assert_eq!(
        engine.snapshot(clock.now).paused_until,
        Some(clock.now - TimeDelta::seconds(600) + TimeDelta::minutes(30))
    );
}

#[test]
fn p2_preset_durations() {
    for (preset, hours) in [
        (PausePreset::H2, 2),
        (PausePreset::H4, 4),
        (PausePreset::D1, 24),
    ] {
        let clock = FakeClock::new();
        let mut engine = started_engine(&clock, 10.0);
        let events = engine.pause(clock.now, preset, PauseSource::Menu);
        match events.last() {
            Some(EngineEvent::PauseStarted(p)) => {
                assert_eq!(p.until_at, clock.now + TimeDelta::hours(hours))
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn p3_short_pause_restores_frozen_load() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 80.0);
    engine.pause(clock.now, PausePreset::H2, PauseSource::Menu);
    let events = run(&mut engine, &mut clock, 2 * 3600, ActivityKind::Away);
    assert!(events
        .iter()
        .any(|e| matches!(e, EngineEvent::PauseEnded { early: false, .. })));
    assert_eq!(engine.phase(), Phase::Running);
    assert_close(engine.load_percent(), 80.0);
}

#[test]
fn p4_day_pause_resets_load() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 80.0);
    engine.pause(clock.now, PausePreset::D1, PauseSource::Menu);
    let now = clock.advance(24 * 3600);
    engine.tick(now, ActivityKind::Active);
    assert_eq!(engine.phase(), Phase::Running);
    assert_eq!(engine.load_percent(), 0.0);
}

#[test]
fn p5_resume_early() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 40.0);
    engine.pause(clock.now, PausePreset::H4, PauseSource::Menu);
    clock.advance(60);
    let events = engine.resume(clock.now).unwrap();
    assert!(matches!(
        events[0],
        EngineEvent::PauseEnded { early: true, .. }
    ));
    run(&mut engine, &mut clock, 60, ActivityKind::Active);
    assert_close(engine.load_percent(), 42.0);
    assert_eq!(engine.resume(clock.now), Err(EngineError::NotPaused));
}

#[test]
fn p6_pause_cancels_postponed_break() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    engine.postpone(clock.now).unwrap();
    let events = engine.pause(clock.now, PausePreset::M30, PauseSource::Menu);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded {
            reason: BreakEndReason::CancelledByPause,
            postpone_count: 1,
            ..
        }
    )));
    clock.advance(1800);
    let events = engine.tick(clock.now, ActivityKind::Active);
    assert!(break_started(&events).is_empty() || engine.state().not_before.is_none());
    assert!(!engine.is_postponed());
}

#[test]
fn p6_pause_cancels_break_in_progress() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.99);
    run(&mut engine, &mut clock, 1, ActivityKind::Active);
    let events = engine.pause(clock.now, PausePreset::M30, PauseSource::Menu);
    assert!(events.iter().any(|e| matches!(
        e,
        EngineEvent::BreakEnded {
            reason: BreakEndReason::CancelledByPause,
            ..
        }
    )));
    assert_eq!(engine.phase(), Phase::Paused);
}

#[test]
fn p7_engine_state_round_trips_through_restore() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 80.0);
    engine.pause(clock.now, PausePreset::H4, PauseSource::Menu);
    let saved = engine.state().clone();
    let mut restored = Engine::with_state(Settings::default(), saved.clone());
    clock.advance(600);
    restored.tick(clock.now, ActivityKind::Away);
    assert_eq!(restored.phase(), Phase::Paused);
    assert_eq!(restored.state().pause, saved.pause);
    assert_eq!(restored.load_percent(), 80.0);
}

#[test]
fn pause_expiring_during_downtime_decays_remaining_gap() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 80.0);
    engine.pause(clock.now, PausePreset::M30, PauseSource::Menu);
    clock.advance(33 * 60);
    engine.tick(clock.now, ActivityKind::Active);
    assert_eq!(engine.phase(), Phase::Running);
    assert_close(engine.load_percent(), 50.0);
}

#[test]
fn repausing_keeps_original_frozen_load() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 70.0);
    engine.pause(clock.now, PausePreset::M30, PauseSource::Menu);
    clock.advance(60);
    let events = engine.pause(clock.now, PausePreset::H2, PauseSource::Menu);
    assert!(matches!(
        events[0],
        EngineEvent::PauseEnded { early: true, .. }
    ));
    match events.last() {
        Some(EngineEvent::PauseStarted(p)) => assert_eq!(p.frozen_load_percent, 70.0),
        other => panic!("unexpected {other:?}"),
    }
}

fn classify_with(locked: bool, gap: bool, idle: f64, motion: bool) -> ActivityKind {
    classify(&ClassifierInput {
        screen_locked: locked,
        tick_gap: gap,
        input_idle_seconds: idle,
        idle_threshold_seconds: 60,
        motion,
    })
}

#[test]
fn a1_recent_input_is_active() {
    assert_eq!(
        classify_with(false, false, 0.5, false),
        ActivityKind::Active
    );
    assert_eq!(
        classify_with(false, false, 59.9, true),
        ActivityKind::Active
    );
}

#[test]
fn a2_motion_on_any_display_is_watching() {
    let mut motion = MotionDetector::new();
    for ratio in [0.0, 0.05, 0.04, 0.10, 0.03] {
        motion.push(ratio);
    }
    assert!(motion.is_motion());
    assert_eq!(
        classify_with(false, false, 60.0, motion.is_motion()),
        ActivityKind::Watching
    );
}

#[test]
fn a3_static_screen_without_input_is_away() {
    let mut motion = MotionDetector::new();
    for ratio in [0.0, 0.0, 0.05, 0.0, 0.01] {
        motion.push(ratio);
    }
    assert!(!motion.is_motion());
    assert_eq!(
        classify_with(false, false, 61.0, motion.is_motion()),
        ActivityKind::Away
    );
}

#[test]
fn a3_motion_needs_four_of_five_samples() {
    let mut motion = MotionDetector::new();
    for _ in 0..3 {
        motion.push(0.5);
    }
    assert!(!motion.is_motion());
    motion.push(0.5);
    assert!(motion.is_motion());
    for _ in 0..2 {
        motion.push(0.0);
    }
    assert!(!motion.is_motion());
}

#[test]
fn a4_locked_screen_is_away_even_with_video() {
    assert_eq!(classify_with(true, false, 0.0, true), ActivityKind::Away);
    assert_eq!(classify_with(false, true, 0.0, true), ActivityKind::Away);
}

#[test]
fn recorder_splits_segments_on_kind_change() {
    let clock = FakeClock::new();
    let t0 = clock.now;
    let t1 = t0 + TimeDelta::seconds(10);
    let mut recorder = ActivityRecorder::new();
    assert_eq!(
        recorder.record(t0, ActivityKind::Active),
        vec![SegmentOp::Open {
            kind: ActivityKind::Active,
            at: t0
        }]
    );
    assert!(recorder.record(t0, ActivityKind::Active).is_empty());
    assert_eq!(
        recorder.record(t1, ActivityKind::Away),
        vec![
            SegmentOp::Close { at: t1 },
            SegmentOp::Open {
                kind: ActivityKind::Away,
                at: t1
            }
        ]
    );
    assert_eq!(recorder.stop(t1), vec![SegmentOp::Close { at: t1 }]);
    assert!(recorder.stop(t1).is_empty());
}

fn av(mic: bool, camera: bool) -> AvState {
    AvState { mic, camera }
}

fn drive(
    detector: &mut MeetingDetector,
    clock: &mut FakeClock,
    secs: i64,
    state: AvState,
    enabled: bool,
    paused: bool,
) -> Vec<MeetingEvent> {
    let mut events = Vec::new();
    for _ in 0..secs {
        let now = clock.advance(1);
        events.extend(detector.update(now, state, enabled, paused));
    }
    events
}

fn notifications(events: &[MeetingEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, MeetingEvent::Started { notify: true, .. }))
        .count()
}

#[test]
fn g1_mic_on_for_delay_notifies_once() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(30);
    let before = drive(&mut detector, &mut clock, 30, av(true, false), true, false);
    assert_eq!(notifications(&before), 0);
    let at = drive(&mut detector, &mut clock, 1, av(true, false), true, false);
    assert_eq!(notifications(&at), 1);
    let later = drive(&mut detector, &mut clock, 600, av(true, false), true, false);
    assert_eq!(notifications(&later), 0);
    assert!(detector.is_nudged());
}

#[test]
fn g3_brief_mic_off_does_not_start_new_meeting() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(30);
    drive(&mut detector, &mut clock, 60, av(true, false), true, false);
    let off = drive(
        &mut detector,
        &mut clock,
        120,
        av(false, false),
        true,
        false,
    );
    let on = drive(&mut detector, &mut clock, 120, av(true, false), true, false);
    assert!(off.is_empty());
    assert_eq!(notifications(&on), 0);
}

#[test]
fn g4_new_meeting_after_five_minutes_off() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(30);
    drive(&mut detector, &mut clock, 60, av(true, false), true, false);
    let off = drive(
        &mut detector,
        &mut clock,
        301,
        av(false, false),
        true,
        false,
    );
    assert!(matches!(
        off.last(),
        Some(MeetingEvent::Ended { mic_used: true, .. })
    ));
    assert!(!detector.is_active());
    let camera = drive(&mut detector, &mut clock, 31, av(false, true), true, false);
    assert_eq!(notifications(&camera), 1);
}

#[test]
fn g5_mic_shorter_than_delay_is_ignored() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(120);
    let events = drive(&mut detector, &mut clock, 90, av(true, false), true, false);
    let off = drive(
        &mut detector,
        &mut clock,
        120,
        av(false, false),
        true,
        false,
    );
    assert!(events.is_empty() && off.is_empty());
}

#[test]
fn g7_detection_disabled_never_notifies() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(30);
    let events = drive(&mut detector, &mut clock, 600, av(true, true), false, false);
    assert!(events.is_empty());
    assert!(!detector.is_active());
}

#[test]
fn g7_disabling_mid_meeting_ends_it() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(30);
    drive(&mut detector, &mut clock, 60, av(true, false), true, false);
    let events = drive(&mut detector, &mut clock, 1, av(true, false), false, false);
    assert!(matches!(events[..], [MeetingEvent::Ended { .. }]));
}

#[test]
fn p8_paused_meeting_is_not_notified() {
    let mut clock = FakeClock::new();
    let mut detector = MeetingDetector::new(30);
    let events = drive(&mut detector, &mut clock, 60, av(true, false), true, true);
    assert_eq!(notifications(&events), 0);
    let after_pause = drive(&mut detector, &mut clock, 600, av(true, false), true, false);
    assert_eq!(notifications(&after_pause), 0);
    assert!(!detector.is_nudged());
}

#[test]
fn g8_meeting_does_not_block_breaks() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 99.0);
    let mut detector = MeetingDetector::new(30);
    let mut breaks = 0;
    for _ in 0..60 {
        let now = clock.advance(1);
        detector.update(now, av(true, true), true, engine.is_paused());
        breaks += break_started(&engine.tick(now, ActivityKind::Active)).len();
    }
    assert!(detector.is_nudged());
    assert_eq!(breaks, 1);
}

#[test]
fn key_result_meter_accuracy() {
    let mut clock = FakeClock::new();
    let mut engine = started_engine(&clock, 0.0);
    run(&mut engine, &mut clock, 25 * 60, ActivityKind::Active);
    run(&mut engine, &mut clock, 60, ActivityKind::Away);
    assert!((engine.load_percent() - 40.0).abs() <= 0.5);
}
