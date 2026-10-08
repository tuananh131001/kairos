use std::time::Instant;

use chrono::{FixedOffset, TimeDelta, TimeZone, Utc};
use kairos_core::*;
use kairos_store::{local_midnight, Store};

fn t(h: u32, m: u32) -> Timestamp {
    Utc.with_ymd_and_hms(2026, 3, 10, h, m, 0).unwrap()
}

#[test]
fn file_database_uses_wal_and_migrates() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested/kairos.db");
    let store = Store::open(&path).unwrap();
    assert_eq!(store.journal_mode().unwrap(), "wal");
    assert_eq!(store.schema_version().unwrap(), 1);
    drop(store);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.schema_version().unwrap(), 1);
}

#[test]
fn settings_default_and_round_trip() {
    let store = Store::open_in_memory().unwrap();
    assert_eq!(store.load_settings().unwrap(), Settings::default());
    let custom = Settings {
        work_minutes: 25,
        break_seconds: 75,
        reminder_text: "Drink water 💧".into(),
        meeting_detection_enabled: false,
        meeting_notify_delay_seconds: 120,
        ..Settings::default()
    };
    store.save_settings(&custom, t(9, 0)).unwrap();
    assert_eq!(store.load_settings().unwrap(), custom);
}

#[test]
fn r8_segment_spanning_midnight_counts_only_today() {
    let store = Store::open_in_memory().unwrap();
    let start = Utc.with_ymd_and_hms(2026, 3, 9, 23, 50, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 3, 10, 0, 20, 0).unwrap();
    let id = store.open_segment(ActivityKind::Active, start).unwrap();
    store.close_segment(id, end).unwrap();
    let now = Utc.with_ymd_and_hms(2026, 3, 10, 1, 0, 0).unwrap();
    assert_eq!(store.today_worked_seconds(&Utc, now).unwrap(), 20 * 60);
}

#[test]
fn today_total_uses_local_midnight_and_open_segments() {
    let store = Store::open_in_memory().unwrap();
    let tz = FixedOffset::east_opt(7 * 3600).unwrap();
    let now = t(5, 0);
    assert_eq!(local_midnight(&tz, now), t(0, 0) - TimeDelta::hours(7));
    store
        .open_segment(ActivityKind::Watching, t(4, 30))
        .unwrap();
    let away = store.open_segment(ActivityKind::Away, t(1, 0)).unwrap();
    store.close_segment(away, t(2, 0)).unwrap();
    assert_eq!(store.today_worked_seconds(&tz, now).unwrap(), 30 * 60);
}

#[test]
fn today_total_query_is_fast_with_a_year_of_rows() {
    let store = Store::open_in_memory().unwrap();
    let base = Utc.with_ymd_and_hms(2025, 3, 10, 0, 0, 0).unwrap();
    for i in 0..180_000i64 {
        let start = base + TimeDelta::seconds(i * 175);
        let id = store
            .open_segment(
                if i % 3 == 0 {
                    ActivityKind::Away
                } else {
                    ActivityKind::Active
                },
                start,
            )
            .unwrap();
        store
            .close_segment(id, start + TimeDelta::seconds(170))
            .unwrap();
    }
    let now = base + TimeDelta::seconds(180_000 * 175);
    let started = Instant::now();
    let total = store.today_worked_seconds(&Utc, now).unwrap();
    assert!(total > 0);
    assert!(started.elapsed().as_millis() < 5, "{:?}", started.elapsed());
}

#[test]
fn break_chain_postpone_then_complete() {
    let store = Store::open_in_memory().unwrap();
    let state = EngineState {
        load_percent: 99.99,
        last_tick: Some(t(9, 0)),
        ..EngineState::default()
    };
    let mut engine = Engine::with_state(Settings::default(), state);
    let mut now = t(9, 0);
    let mut tick = |engine: &mut Engine, secs: i64| {
        for _ in 0..secs {
            now += TimeDelta::seconds(1);
            let events = engine.tick(now, ActivityKind::Active);
            store.apply_engine_events(&events, None).unwrap();
        }
        now
    };
    let now1 = tick(&mut engine, 1);
    let id = store.latest_break().unwrap().unwrap().id;
    store
        .apply_engine_events(&engine.postpone(now1).unwrap(), None)
        .unwrap();
    let row = store.break_event(id).unwrap().unwrap();
    assert_eq!(row.outcome.as_deref(), Some("postponed"));
    assert_eq!(row.postpone_count, 1);
    tick(&mut engine, 300);
    let row = store.break_event(id).unwrap().unwrap();
    assert_eq!(row.outcome, None);
    assert_eq!(row.ended_at, None);
    tick(&mut engine, 600);
    let row = store.latest_break().unwrap().unwrap();
    assert_eq!(row.id, id);
    assert_eq!(row.outcome.as_deref(), Some("completed"));
    assert_eq!(row.postpone_count, 1);
    assert_eq!(row.planned_seconds, 600);
}

#[test]
fn p6_pause_marks_postponed_break_cancelled() {
    let store = Store::open_in_memory().unwrap();
    let mut engine = Engine::with_state(
        Settings::default(),
        EngineState {
            load_percent: 99.99,
            last_tick: Some(t(9, 0)),
            ..EngineState::default()
        },
    );
    let events = engine.tick(t(9, 0) + TimeDelta::seconds(1), ActivityKind::Active);
    store.apply_engine_events(&events, None).unwrap();
    let now = t(9, 1);
    store
        .apply_engine_events(&engine.postpone(now).unwrap(), None)
        .unwrap();
    store
        .apply_engine_events(
            &engine.pause(now, PausePreset::M30, PauseSource::Menu),
            None,
        )
        .unwrap();
    let row = store.latest_break().unwrap().unwrap();
    assert_eq!(row.outcome.as_deref(), Some("cancelled_by_pause"));
}

#[test]
fn p5_resume_sets_cancelled_at() {
    let store = Store::open_in_memory().unwrap();
    let mut engine = Engine::new(Settings::default());
    engine.tick(t(9, 0), ActivityKind::Active);
    store
        .apply_engine_events(
            &engine.pause(t(9, 0), PausePreset::H4, PauseSource::Menu),
            None,
        )
        .unwrap();
    assert!(store.active_pause(t(9, 30)).unwrap().is_some());
    store
        .apply_engine_events(&engine.resume(t(9, 30)).unwrap(), None)
        .unwrap();
    let row = store.latest_pause().unwrap().unwrap();
    assert_eq!(row.cancelled_at, Some(t(9, 30)));
    assert!(store.active_pause(t(9, 31)).unwrap().is_none());
}

#[test]
fn p7_pause_survives_restart() {
    let store = Store::open_in_memory().unwrap();
    let mut engine = Engine::with_state(
        Settings::default(),
        EngineState {
            load_percent: 80.0,
            last_tick: Some(t(9, 0)),
            ..EngineState::default()
        },
    );
    store
        .apply_engine_events(
            &engine.pause(t(9, 0), PausePreset::H4, PauseSource::Menu),
            None,
        )
        .unwrap();
    store.checkpoint(engine.state(), t(9, 0)).unwrap();
    let recovered = store.recover(Settings::default(), t(10, 0)).unwrap();
    assert_eq!(recovered.engine.phase(), Phase::Paused);
    assert_eq!(recovered.engine.load_percent(), 80.0);
    assert_eq!(
        recovered.engine.state().pause.as_ref().unwrap().until_at,
        t(13, 0)
    );
}

#[test]
fn restore_after_30_minute_downtime() {
    let store = Store::open_in_memory().unwrap();
    let engine_state = EngineState {
        load_percent: 70.0,
        last_tick: Some(t(9, 0)),
        ..EngineState::default()
    };
    store.open_segment(ActivityKind::Active, t(8, 30)).unwrap();
    store.checkpoint(&engine_state, t(9, 0)).unwrap();
    let recovered = store.recover(Settings::default(), t(9, 30)).unwrap();
    assert_eq!(recovered.closed_segments, 1);
    assert_eq!(recovered.engine.load_percent(), 0.0);
    assert_eq!(recovered.engine.phase(), Phase::Running);
    assert_eq!(
        store.worked_seconds_between(t(0, 0), t(9, 30)).unwrap(),
        30 * 60
    );
}

#[test]
fn restore_after_short_downtime_decays_partially() {
    let store = Store::open_in_memory().unwrap();
    let engine_state = EngineState {
        load_percent: 70.0,
        last_tick: Some(t(9, 0)),
        ..EngineState::default()
    };
    store.checkpoint(&engine_state, t(9, 0)).unwrap();
    let recovered = store.recover(Settings::default(), t(9, 2)).unwrap();
    assert!((recovered.engine.load_percent() - 50.0).abs() < 0.01);
}

#[test]
fn restore_expires_past_pause() {
    let store = Store::open_in_memory().unwrap();
    let mut engine = Engine::with_state(
        Settings::default(),
        EngineState {
            load_percent: 80.0,
            last_tick: Some(t(9, 0)),
            ..EngineState::default()
        },
    );
    store
        .apply_engine_events(
            &engine.pause(t(9, 0), PausePreset::M30, PauseSource::Menu),
            None,
        )
        .unwrap();
    store.checkpoint(engine.state(), t(9, 10)).unwrap();
    let recovered = store.recover(Settings::default(), t(9, 31)).unwrap();
    assert_eq!(recovered.engine.phase(), Phase::Running);
    assert!((recovered.engine.load_percent() - 70.0).abs() < 0.01);
    assert!(store.active_pause(t(9, 31)).unwrap().is_none());
}

#[test]
fn meeting_rows_and_pause_link() {
    let store = Store::open_in_memory().unwrap();
    let id = store
        .insert_meeting(t(9, 0), true, false, Some(t(9, 1)))
        .unwrap();
    let mut engine = Engine::new(Settings::default());
    engine.tick(t(9, 1), ActivityKind::Active);
    let events = engine.pause(t(9, 2), PausePreset::H2, PauseSource::MeetingNotification);
    store.apply_engine_events(&events, Some(id)).unwrap();
    store.set_meeting_action(id, PausePreset::H2).unwrap();
    store.end_meeting(id, t(10, 0), true, true).unwrap();
    let row = store.meeting_row(id).unwrap().unwrap();
    assert_eq!(row.user_action, "pause_2h");
    assert!(row.camera_used);
    assert_eq!(row.ended_at, Some(t(10, 0)));
    let pause = store.latest_pause().unwrap().unwrap();
    assert_eq!(pause.meeting_event_id, Some(id));
    assert_eq!(pause.pause.source, PauseSource::MeetingNotification);
}
