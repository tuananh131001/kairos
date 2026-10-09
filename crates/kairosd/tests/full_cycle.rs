use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{TimeDelta, TimeZone, Utc};
use kairos_core::{AvState, PausePreset, PauseSource, Settings, Timestamp};
use kairos_ipc::*;
use kairosd::{bind_socket, run, AppLauncher, Clock, Daemon, Probes, TickAck};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

#[derive(Default)]
struct World {
    now: Option<Timestamp>,
    idle: f64,
    locked: bool,
    motion: f64,
    av: AvState,
    permission: bool,
    sampling: bool,
    launches: usize,
}

type Shared = Arc<Mutex<World>>;

struct FakeProbes(Shared);
struct FakeClock(Shared);
struct FakeLauncher(Shared);

impl Probes for FakeProbes {
    fn input_idle_seconds(&mut self) -> f64 {
        self.0.lock().unwrap().idle
    }
    fn screen_locked(&mut self) -> bool {
        self.0.lock().unwrap().locked
    }
    fn take_motion_ratio(&mut self) -> f64 {
        self.0.lock().unwrap().motion
    }
    fn av(&mut self) -> AvState {
        self.0.lock().unwrap().av
    }
    fn permission(&mut self) -> bool {
        self.0.lock().unwrap().permission
    }
    fn start_sampling(&mut self) -> Result<(), String> {
        self.0.lock().unwrap().sampling = true;
        Ok(())
    }
    fn stop_sampling(&mut self) {
        self.0.lock().unwrap().sampling = false;
    }
    fn is_sampling(&self) -> bool {
        self.0.lock().unwrap().sampling
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Timestamp {
        self.0.lock().unwrap().now.unwrap()
    }
}

impl AppLauncher for FakeLauncher {
    fn launch(&mut self) {
        self.0.lock().unwrap().launches += 1;
    }
}

struct Client {
    lines: Lines<BufReader<OwnedReadHalf>>,
    writer: OwnedWriteHalf,
    next_id: u64,
}

impl Client {
    async fn connect(path: &std::path::Path) -> Self {
        let stream = UnixStream::connect(path).await.unwrap();
        let (r, w) = stream.into_split();
        Self {
            lines: BufReader::new(r).lines(),
            writer: w,
            next_id: 1,
        }
    }

    async fn send(&mut self, request: Request) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let line = encode(&ClientMessage::new(Some(id), request));
        self.writer.write_all(line.as_bytes()).await.unwrap();
        id
    }

    async fn next(&mut self) -> Event {
        let line = tokio::time::timeout(Duration::from_secs(3), self.lines.next_line())
            .await
            .expect("timed out waiting for message")
            .unwrap()
            .expect("connection closed");
        decode_server(&line).unwrap().event
    }

    async fn request(&mut self, request: Request) -> Event {
        let id = self.send(request).await;
        loop {
            let event = self.next().await;
            if matches!(&event, Event::Reply { id: Some(r), .. } if *r == id) {
                return event;
            }
        }
    }

    async fn expect(&mut self, what: &str, pred: impl Fn(&Event) -> bool) -> Event {
        loop {
            let event = self.next().await;
            if pred(&event) {
                return event;
            }
            assert!(
                !matches!(event, Event::BreakStarted { .. } | Event::MeetingDetected)
                    || what.contains("any"),
                "unexpected {event:?} while waiting for {what}"
            );
        }
    }

    async fn drain(&mut self) -> Vec<Event> {
        let mut out = Vec::new();
        while let Ok(Ok(Some(line))) =
            tokio::time::timeout(Duration::from_millis(50), self.lines.next_line()).await
        {
            out.push(decode_server(&line).unwrap().event);
        }
        out
    }
}

struct Harness {
    world: Shared,
    ticks: mpsc::Sender<TickAck>,
}

impl Harness {
    async fn tick(&self, n: usize) {
        for _ in 0..n {
            {
                let mut world = self.world.lock().unwrap();
                let now = world.now.unwrap() + TimeDelta::seconds(1);
                world.now = Some(now);
            }
            let (tx, rx) = oneshot::channel();
            self.ticks.send(Some(tx)).await.unwrap();
            rx.await.unwrap();
        }
    }

    fn set(&self, f: impl FnOnce(&mut World)) {
        f(&mut self.world.lock().unwrap());
    }
}

fn advance(world: &Shared, by: TimeDelta) {
    let mut w = world.lock().unwrap();
    w.now = Some(w.now.unwrap() + by);
}

fn state_of(event: &Event) -> StateEvent {
    match event {
        Event::Reply {
            state: Some(state), ..
        } => state.clone(),
        Event::State(state) => state.clone(),
        other => panic!("no state in {other:?}"),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn full_cycle() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("kairosd.sock");
    let db = dir.path().join("kairos.db");
    let world: Shared = Arc::new(Mutex::new(World {
        now: Some(Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap()),
        permission: true,
        idle: 0.0,
        ..World::default()
    }));
    let store = kairos_store::Store::open(&db).unwrap();
    let daemon = Daemon::new(
        store,
        FakeProbes(world.clone()),
        FakeClock(world.clone()),
        FakeLauncher(world.clone()),
    )
    .unwrap();
    let listener = bind_socket(&socket).unwrap();
    let mode = std::fs::metadata(&socket).unwrap();
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&mode.permissions()) & 0o777,
        0o600
    );
    let (tick_tx, tick_rx) = mpsc::channel(1);
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let server = tokio::spawn(run(daemon, listener, tick_rx, async {
        let _ = stop_rx.await;
    }));
    let h = Harness {
        world: world.clone(),
        ticks: tick_tx,
    };

    let mut client = Client::connect(&socket).await;
    let reply = client.request(Request::Subscribe).await;
    assert_eq!(state_of(&reply).load_percent, 0.0);

    let bad = client
        .request(Request::UpdateSettings {
            settings: Settings {
                work_minutes: 0,
                ..Settings::default()
            },
        })
        .await;
    match bad {
        Event::Reply {
            ok: false,
            error: Some(error),
            ..
        } => {
            assert_eq!(error.code, "validation");
            assert_eq!(error.fields[0].field, "work_minutes");
        }
        other => panic!("unexpected {other:?}"),
    }

    let settings = Settings {
        work_minutes: 1,
        break_seconds: 10,
        reminder_text: "Drink water 💧".into(),
        meeting_notify_delay_seconds: 10,
        ..Settings::default()
    };
    let saved = client
        .request(Request::UpdateSettings {
            settings: settings.clone(),
        })
        .await;
    assert!(
        matches!(saved, Event::Reply { ok: true, settings: Some(ref s), .. } if *s == settings)
    );
    match client.request(Request::GetSettings).await {
        Event::Reply {
            settings: Some(s), ..
        } => assert_eq!(s, settings),
        other => panic!("unexpected {other:?}"),
    }

    h.tick(30).await;
    let events = client.drain().await;
    let state = state_of(
        events
            .iter()
            .rev()
            .find(|e| matches!(e, Event::State(_)))
            .unwrap(),
    );
    assert!(
        (state.load_percent - 48.33).abs() < 0.1,
        "{}",
        state.load_percent
    );
    assert_eq!(state.activity, ActivityLabel::Active);
    assert!(h.world.lock().unwrap().sampling);

    h.tick(31).await;
    let started = client
        .expect("break", |e| matches!(e, Event::BreakStarted { .. }))
        .await;
    assert_eq!(
        started,
        Event::BreakStarted {
            text: "Drink water 💧".into(),
            remaining_s: 10
        }
    );

    let postponed = client.request(Request::PostponeBreak).await;
    let state = state_of(&postponed);
    assert!(state.postponed);
    assert_eq!(state.break_in_s, Some(300));
    client
        .expect("postponed", |e| {
            matches!(
                e,
                Event::BreakEnded {
                    reason: BreakEndReasonLabel::Postponed
                }
            )
        })
        .await;
    let again = client.request(Request::PostponeBreak).await;
    assert!(matches!(again, Event::Reply { ok: false, .. }));

    h.tick(299).await;
    let events = client.drain().await;
    assert!(!events
        .iter()
        .any(|e| matches!(e, Event::BreakStarted { .. })));
    h.tick(1).await;
    client
        .expect("second break", |e| matches!(e, Event::BreakStarted { .. }))
        .await;

    h.set(|w| w.idle = 600.0);
    h.tick(10).await;
    client
        .expect("completed", |e| {
            matches!(
                e,
                Event::BreakEnded {
                    reason: BreakEndReasonLabel::Completed
                }
            )
        })
        .await;
    client.drain().await;

    let paused = client
        .request(Request::Pause {
            preset: PausePreset::M30,
            source: PauseSource::Menu,
        })
        .await;
    let state = state_of(&paused);
    assert_eq!(state.phase, PhaseLabel::Paused);
    assert_eq!(state.activity, ActivityLabel::Paused);
    assert!(state.paused_until.is_some());
    assert!(!h.world.lock().unwrap().sampling);

    h.set(|w| {
        w.av = AvState {
            mic: true,
            camera: false,
        }
    });
    h.tick(60).await;
    let events = client.drain().await;
    assert!(!events.iter().any(|e| matches!(e, Event::MeetingDetected)));

    let resumed = client.request(Request::Resume).await;
    assert_eq!(state_of(&resumed).phase, PhaseLabel::Running);
    h.tick(1).await;
    assert!(h.world.lock().unwrap().sampling);

    h.set(|w| w.av = AvState::default());
    h.tick(301).await;
    client.drain().await;
    h.set(|w| {
        w.av = AvState {
            mic: false,
            camera: true,
        }
    });
    h.tick(11).await;
    client
        .expect("meeting", |e| matches!(e, Event::MeetingDetected))
        .await;
    let state = state_of(&client.request(Request::GetState).await);
    assert!(state.meeting_active);

    let paused = client
        .request(Request::Pause {
            preset: PausePreset::H2,
            source: PauseSource::MeetingNotification,
        })
        .await;
    assert_eq!(state_of(&paused).phase, PhaseLabel::Paused);
    client.request(Request::Resume).await;
    assert_eq!(h.world.lock().unwrap().launches, 0);

    h.set(|w| w.av = AvState::default());
    h.tick(301).await;
    client
        .expect("meeting ended", |e| matches!(e, Event::MeetingEnded))
        .await;

    drop(client);
    h.set(|w| w.idle = 0.0);
    h.tick(61).await;
    assert_eq!(h.world.lock().unwrap().launches, 1);

    let mut late = Client::connect(&socket).await;
    let reply = late.request(Request::Subscribe).await;
    assert_eq!(state_of(&reply).phase, PhaseLabel::OnBreak);
    match late.next().await {
        Event::BreakStarted { remaining_s, .. } => assert!(remaining_s <= 10 && remaining_s > 0),
        other => panic!("unexpected {other:?}"),
    }

    let reply = late.request(Request::Shutdown).await;
    assert!(matches!(reply, Event::Reply { ok: true, .. }));
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .expect("daemon did not exit")
        .unwrap();
    drop(stop_tx);

    let store = kairos_store::Store::open(&db).unwrap();
    let first = store.break_event(1).unwrap().unwrap();
    assert_eq!(first.outcome.as_deref(), Some("completed"));
    assert_eq!(first.postpone_count, 1);
    let pause = store.latest_pause().unwrap().unwrap();
    assert_eq!(pause.pause.source, PauseSource::MeetingNotification);
    assert!(pause.meeting_event_id.is_some());
    assert!(pause.cancelled_at.is_some());
    let meeting = store
        .meeting_row(pause.meeting_event_id.unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(meeting.user_action, "pause_2h");
    assert!(meeting.camera_used);
    assert!(meeting.notified_at.is_some());
    let worked = store
        .worked_seconds_between(
            Utc.with_ymd_and_hms(2026, 3, 10, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 3, 11, 0, 0, 0).unwrap(),
        )
        .unwrap();
    assert!(worked >= 360, "worked {worked}");
}

#[tokio::test(flavor = "current_thread")]
async fn restart_restores_pause() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("kairos.db");
    let world: Shared = Arc::new(Mutex::new(World {
        now: Some(Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap()),
        permission: true,
        ..World::default()
    }));
    let make = || {
        Daemon::new(
            kairos_store::Store::open(&db).unwrap(),
            FakeProbes(world.clone()),
            FakeClock(world.clone()),
            FakeLauncher(world.clone()),
        )
        .unwrap()
    };
    let mut daemon = make();
    for _ in 0..600 {
        advance(&world, TimeDelta::seconds(1));
        daemon.tick(1);
    }
    let load = daemon.engine().load_percent();
    assert!((load - 20.0).abs() < 0.1, "{load}");
    daemon.handle(
        Request::Pause {
            preset: PausePreset::H4,
            source: PauseSource::Menu,
        },
        1,
    );
    let until = daemon.engine().state().pause.as_ref().unwrap().until_at;
    drop(daemon);
    advance(&world, TimeDelta::minutes(5));
    let restored = make();
    assert!(restored.engine().is_paused());
    assert_eq!(
        restored.engine().state().pause.as_ref().unwrap().until_at,
        until
    );
    assert!((restored.engine().load_percent() - load).abs() < 0.01);
}

#[tokio::test(flavor = "current_thread")]
async fn missing_permission_stops_tracking() {
    let dir = tempfile::tempdir().unwrap();
    let world: Shared = Arc::new(Mutex::new(World {
        now: Some(Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap()),
        permission: false,
        ..World::default()
    }));
    let mut daemon = Daemon::new(
        kairos_store::Store::open(&dir.path().join("k.db")).unwrap(),
        FakeProbes(world.clone()),
        FakeClock(world.clone()),
        FakeLauncher(world.clone()),
    )
    .unwrap();
    for _ in 0..120 {
        advance(&world, TimeDelta::seconds(1));
        daemon.tick(1);
    }
    assert_eq!(daemon.engine().load_percent(), 0.0);
    assert!(!world.lock().unwrap().sampling);
    assert_eq!(daemon.current_state().permission, Permission::Denied);
    world.lock().unwrap().permission = true;
    for _ in 0..3 {
        advance(&world, TimeDelta::seconds(1));
        daemon.tick(1);
    }
    assert_eq!(daemon.current_state().permission, Permission::Granted);
    assert!(world.lock().unwrap().sampling);
    assert!(daemon.engine().load_percent() > 0.0);
}

#[tokio::test(flavor = "current_thread")]
async fn rejects_garbage_and_second_instance() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("s.sock");
    let world: Shared = Arc::new(Mutex::new(World {
        now: Some(Utc.with_ymd_and_hms(2026, 3, 10, 9, 0, 0).unwrap()),
        permission: true,
        ..World::default()
    }));
    let daemon = Daemon::new(
        kairos_store::Store::open_in_memory().unwrap(),
        FakeProbes(world.clone()),
        FakeClock(world.clone()),
        FakeLauncher(world.clone()),
    )
    .unwrap();
    let listener = bind_socket(&socket).unwrap();
    let (_tick_tx, tick_rx) = mpsc::channel(1);
    let server = tokio::spawn(run(daemon, listener, tick_rx, std::future::pending()));
    let mut client = Client::connect(&socket).await;
    client.writer.write_all(b"not json\n").await.unwrap();
    match client.next().await {
        Event::Reply {
            ok: false,
            error: Some(e),
            ..
        } => assert_eq!(e.code, "bad_request"),
        other => panic!("unexpected {other:?}"),
    }
    let err = bind_socket(&socket).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    client.request(Request::Shutdown).await;
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}
