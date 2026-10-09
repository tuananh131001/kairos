use std::path::{Path, PathBuf};

use chrono::{DateTime, TimeZone, Utc};
use kairos_core::{
    ActivePause, ActivityKind, BreakChain, BreakEndReason, Engine, EngineEvent, EngineState,
    PausePreset, PauseSource, Phase, Settings, Timestamp,
};
use rusqlite::{params, Connection, OptionalExtension};

pub use rusqlite::Error;
pub type Result<T> = std::result::Result<T, Error>;

const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    work_minutes INTEGER NOT NULL,
    break_seconds INTEGER NOT NULL,
    reminder_text TEXT NOT NULL,
    idle_threshold_seconds INTEGER NOT NULL,
    meeting_detection_enabled INTEGER NOT NULL,
    meeting_notify_delay_seconds INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE engine_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    load_percent REAL NOT NULL,
    phase TEXT NOT NULL CHECK (phase IN ('running', 'on_break', 'paused')),
    not_before INTEGER,
    checkpointed_at INTEGER NOT NULL,
    break_chain_started_at INTEGER,
    postpone_count INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE activity_segment (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('active', 'watching', 'away')),
    started_at INTEGER NOT NULL,
    ended_at INTEGER
);
CREATE INDEX idx_activity_segment_started_at ON activity_segment (started_at);
CREATE TABLE break_event (
    id INTEGER PRIMARY KEY,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    planned_seconds INTEGER NOT NULL,
    postpone_count INTEGER NOT NULL DEFAULT 0,
    outcome TEXT CHECK (outcome IN ('completed', 'postponed', 'cancelled_by_pause'))
);
CREATE TABLE meeting_event (
    id INTEGER PRIMARY KEY,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    mic_used INTEGER NOT NULL,
    camera_used INTEGER NOT NULL,
    notified_at INTEGER,
    user_action TEXT NOT NULL DEFAULT 'none'
        CHECK (user_action IN ('none', 'pause_30m', 'pause_2h', 'pause_4h'))
);
CREATE TABLE pause (
    id INTEGER PRIMARY KEY,
    preset TEXT NOT NULL CHECK (preset IN ('30m', '2h', '4h', '1d')),
    frozen_load_percent REAL NOT NULL,
    started_at INTEGER NOT NULL,
    until_at INTEGER NOT NULL,
    cancelled_at INTEGER,
    source TEXT NOT NULL CHECK (source IN ('menu', 'meeting_notification')),
    meeting_event_id INTEGER REFERENCES meeting_event (id)
);
"#];

pub struct Store {
    conn: Connection,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoredEngine {
    pub state: EngineState,
    pub checkpointed_at: Timestamp,
}

#[derive(Debug)]
pub struct Recovered {
    pub engine: Engine,
    pub events: Vec<EngineEvent>,
    pub closed_segments: usize,
}

pub fn default_db_path() -> PathBuf {
    support_dir().join("kairos.db")
}

pub fn support_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    home.join("Library/Application Support/Kairos")
}

fn ms(t: Timestamp) -> i64 {
    t.timestamp_millis()
}

fn from_ms(v: i64) -> Timestamp {
    DateTime::from_timestamp_millis(v).unwrap_or_default()
}

fn opt_ms(t: Option<Timestamp>) -> Option<i64> {
    t.map(ms)
}

fn invalid(column: usize, what: &str) -> Error {
    Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Text,
        format!("invalid {what}").into(),
    )
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let mut store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&mut self) -> Result<()> {
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", index as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    pub fn schema_version(&self) -> Result<i64> {
        self.conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
    }

    pub fn journal_mode(&self) -> Result<String> {
        self.conn
            .pragma_query_value(None, "journal_mode", |r| r.get(0))
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let row = self
            .conn
            .query_row(
                "SELECT work_minutes, break_seconds, reminder_text, idle_threshold_seconds,
                        meeting_detection_enabled, meeting_notify_delay_seconds
                 FROM settings WHERE id = 1",
                [],
                |r| {
                    Ok(Settings {
                        work_minutes: r.get(0)?,
                        break_seconds: r.get(1)?,
                        reminder_text: r.get(2)?,
                        idle_threshold_seconds: r.get(3)?,
                        meeting_detection_enabled: r.get(4)?,
                        meeting_notify_delay_seconds: r.get(5)?,
                    })
                },
            )
            .optional()?;
        Ok(row.unwrap_or_default())
    }

    pub fn save_settings(&self, settings: &Settings, now: Timestamp) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (id, work_minutes, break_seconds, reminder_text,
                 idle_threshold_seconds, meeting_detection_enabled, meeting_notify_delay_seconds,
                 updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT (id) DO UPDATE SET
                 work_minutes = excluded.work_minutes,
                 break_seconds = excluded.break_seconds,
                 reminder_text = excluded.reminder_text,
                 idle_threshold_seconds = excluded.idle_threshold_seconds,
                 meeting_detection_enabled = excluded.meeting_detection_enabled,
                 meeting_notify_delay_seconds = excluded.meeting_notify_delay_seconds,
                 updated_at = excluded.updated_at",
            params![
                settings.work_minutes,
                settings.break_seconds,
                settings.reminder_text,
                settings.idle_threshold_seconds,
                settings.meeting_detection_enabled,
                settings.meeting_notify_delay_seconds,
                ms(now),
            ],
        )?;
        Ok(())
    }

    pub fn checkpoint(&self, state: &EngineState, now: Timestamp) -> Result<()> {
        let checkpointed_at = state.last_tick.unwrap_or(now);
        self.conn.execute(
            "INSERT INTO engine_state (id, load_percent, phase, not_before, checkpointed_at,
                 break_chain_started_at, postpone_count)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (id) DO UPDATE SET
                 load_percent = excluded.load_percent,
                 phase = excluded.phase,
                 not_before = excluded.not_before,
                 checkpointed_at = excluded.checkpointed_at,
                 break_chain_started_at = excluded.break_chain_started_at,
                 postpone_count = excluded.postpone_count",
            params![
                state.load_percent,
                state.phase.as_str(),
                opt_ms(state.not_before),
                ms(checkpointed_at),
                opt_ms(state.break_chain.as_ref().map(|c| c.started_at)),
                state.break_chain.as_ref().map_or(0, |c| c.postpone_count),
            ],
        )?;
        Ok(())
    }

    pub fn load_engine_state(&self) -> Result<Option<StoredEngine>> {
        let row = self
            .conn
            .query_row(
                "SELECT load_percent, phase, not_before, checkpointed_at,
                        break_chain_started_at, postpone_count
                 FROM engine_state WHERE id = 1",
                [],
                |r| {
                    let phase: String = r.get(1)?;
                    let phase = Phase::parse(&phase).ok_or_else(|| invalid(1, "phase"))?;
                    let chain_started: Option<i64> = r.get(4)?;
                    let checkpointed_at = from_ms(r.get(3)?);
                    Ok(StoredEngine {
                        state: EngineState {
                            load_percent: r.get(0)?,
                            phase,
                            not_before: r.get::<_, Option<i64>>(2)?.map(from_ms),
                            last_tick: Some(checkpointed_at),
                            pause: None,
                            break_chain: chain_started.map(|started| BreakChain {
                                started_at: from_ms(started),
                                postpone_count: r.get(5).unwrap_or(0),
                            }),
                        },
                        checkpointed_at,
                    })
                },
            )
            .optional()?;
        let Some(mut stored) = row else {
            return Ok(None);
        };
        if stored.state.phase == Phase::Paused {
            stored.state.pause = self.open_pause()?.map(|(_, p)| p);
            if stored.state.pause.is_none() {
                stored.state.phase = Phase::Running;
            }
        }
        Ok(Some(stored))
    }

    pub fn open_segment(&self, kind: ActivityKind, at: Timestamp) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO activity_segment (kind, started_at) VALUES (?1, ?2)",
            params![kind.as_str(), ms(at)],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn close_segment(&self, id: i64, at: Timestamp) -> Result<()> {
        self.conn.execute(
            "UPDATE activity_segment SET ended_at = MAX(started_at, ?2) WHERE id = ?1",
            params![id, ms(at)],
        )?;
        Ok(())
    }

    pub fn close_dangling_segments(&self, at: Timestamp) -> Result<usize> {
        self.conn.execute(
            "UPDATE activity_segment SET ended_at = MAX(started_at, ?1) WHERE ended_at IS NULL",
            params![ms(at)],
        )
    }

    pub fn worked_seconds_between(&self, start: Timestamp, end: Timestamp) -> Result<i64> {
        let lookback = ms(start) - 86_400_000;
        let total_ms: Option<i64> = self.conn.query_row(
            "SELECT SUM(MIN(COALESCE(ended_at, ?2), ?2) - MAX(started_at, ?1))
             FROM activity_segment
             WHERE kind IN ('active', 'watching')
               AND started_at >= ?3 AND started_at < ?2
               AND COALESCE(ended_at, ?2) > ?1",
            params![ms(start), ms(end), lookback],
            |r| r.get(0),
        )?;
        Ok(total_ms.unwrap_or(0).max(0) / 1000)
    }

    pub fn today_worked_seconds<Tz: TimeZone>(&self, tz: &Tz, now: Timestamp) -> Result<i64> {
        self.worked_seconds_between(local_midnight(tz, now), now)
    }

    pub fn insert_break(&self, started_at: Timestamp, planned_seconds: u64) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO break_event (started_at, planned_seconds, postpone_count)
             VALUES (?1, ?2, 0)",
            params![ms(started_at), planned_seconds as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn open_break_id(&self) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT id FROM break_event
                 WHERE outcome IS NULL OR outcome = 'postponed'
                 ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .optional()
    }

    fn reopen_break(&self, planned_seconds: u64, now: Timestamp) -> Result<i64> {
        match self.open_break_id()? {
            Some(id) => {
                self.conn.execute(
                    "UPDATE break_event SET ended_at = NULL, outcome = NULL WHERE id = ?1",
                    params![id],
                )?;
                Ok(id)
            }
            None => self.insert_break(now, planned_seconds),
        }
    }

    fn finish_break(
        &self,
        at: Timestamp,
        outcome: &str,
        postpone_count: u32,
    ) -> Result<Option<i64>> {
        let Some(id) = self.open_break_id()? else {
            return Ok(None);
        };
        self.conn.execute(
            "UPDATE break_event SET ended_at = ?2, outcome = ?3, postpone_count = ?4 WHERE id = ?1",
            params![id, ms(at), outcome, postpone_count],
        )?;
        Ok(Some(id))
    }

    pub fn break_event(&self, id: i64) -> Result<Option<BreakRow>> {
        self.conn
            .query_row(
                "SELECT id, started_at, ended_at, planned_seconds, postpone_count, outcome
                 FROM break_event WHERE id = ?1",
                params![id],
                BreakRow::from_row,
            )
            .optional()
    }

    pub fn latest_break(&self) -> Result<Option<BreakRow>> {
        self.conn
            .query_row(
                "SELECT id, started_at, ended_at, planned_seconds, postpone_count, outcome
                 FROM break_event ORDER BY id DESC LIMIT 1",
                [],
                BreakRow::from_row,
            )
            .optional()
    }

    pub fn insert_pause(&self, pause: &ActivePause, meeting_event_id: Option<i64>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO pause (preset, frozen_load_percent, started_at, until_at, source,
                 meeting_event_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                pause.preset.as_str(),
                pause.frozen_load_percent,
                ms(pause.started_at),
                ms(pause.until_at),
                pause.source.as_str(),
                meeting_event_id,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn open_pause(&self) -> Result<Option<(i64, ActivePause)>> {
        self.conn
            .query_row(
                "SELECT id, preset, frozen_load_percent, started_at, until_at, source
                 FROM pause WHERE cancelled_at IS NULL ORDER BY id DESC LIMIT 1",
                [],
                pause_from_row,
            )
            .optional()
    }

    pub fn active_pause(&self, now: Timestamp) -> Result<Option<(i64, ActivePause)>> {
        self.conn
            .query_row(
                "SELECT id, preset, frozen_load_percent, started_at, until_at, source
                 FROM pause WHERE until_at > ?1 AND cancelled_at IS NULL
                 ORDER BY id DESC LIMIT 1",
                params![ms(now)],
                pause_from_row,
            )
            .optional()
    }

    pub fn pause_row(&self, id: i64) -> Result<Option<PauseRow>> {
        self.conn
            .query_row(
                "SELECT id, preset, frozen_load_percent, started_at, until_at, source,
                        cancelled_at, meeting_event_id
                 FROM pause WHERE id = ?1",
                params![id],
                |r| {
                    let (id, pause) = pause_from_row(r)?;
                    Ok(PauseRow {
                        id,
                        pause,
                        cancelled_at: r.get::<_, Option<i64>>(6)?.map(from_ms),
                        meeting_event_id: r.get(7)?,
                    })
                },
            )
            .optional()
    }

    pub fn latest_pause(&self) -> Result<Option<PauseRow>> {
        let id: Option<i64> = self
            .conn
            .query_row("SELECT MAX(id) FROM pause", [], |r| r.get(0))?;
        match id {
            Some(id) => self.pause_row(id),
            None => Ok(None),
        }
    }

    fn cancel_open_pause(&self, at: Timestamp) -> Result<()> {
        if let Some((id, _)) = self.open_pause()? {
            self.conn.execute(
                "UPDATE pause SET cancelled_at = ?2 WHERE id = ?1",
                params![id, ms(at)],
            )?;
        }
        Ok(())
    }

    pub fn insert_meeting(
        &self,
        started_at: Timestamp,
        mic: bool,
        camera: bool,
        notified_at: Option<Timestamp>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO meeting_event (started_at, mic_used, camera_used, notified_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![ms(started_at), mic, camera, opt_ms(notified_at)],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn end_meeting(
        &self,
        id: i64,
        ended_at: Timestamp,
        mic_used: bool,
        camera_used: bool,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE meeting_event SET ended_at = ?2, mic_used = ?3, camera_used = ?4 WHERE id = ?1",
            params![id, ms(ended_at), mic_used, camera_used],
        )?;
        Ok(())
    }

    pub fn close_dangling_meetings(&self, at: Timestamp) -> Result<usize> {
        self.conn.execute(
            "UPDATE meeting_event SET ended_at = MAX(started_at, ?1) WHERE ended_at IS NULL",
            params![ms(at)],
        )
    }

    pub fn set_meeting_action(&self, id: i64, preset: PausePreset) -> Result<()> {
        let action = match preset {
            PausePreset::M30 => "pause_30m",
            PausePreset::H2 => "pause_2h",
            PausePreset::H4 => "pause_4h",
            PausePreset::D1 => return Ok(()),
        };
        self.conn.execute(
            "UPDATE meeting_event SET user_action = ?2 WHERE id = ?1",
            params![id, action],
        )?;
        Ok(())
    }

    pub fn meeting_row(&self, id: i64) -> Result<Option<MeetingRow>> {
        self.conn
            .query_row(
                "SELECT id, started_at, ended_at, mic_used, camera_used, notified_at, user_action
                 FROM meeting_event WHERE id = ?1",
                params![id],
                |r| {
                    Ok(MeetingRow {
                        id: r.get(0)?,
                        started_at: from_ms(r.get(1)?),
                        ended_at: r.get::<_, Option<i64>>(2)?.map(from_ms),
                        mic_used: r.get(3)?,
                        camera_used: r.get(4)?,
                        notified_at: r.get::<_, Option<i64>>(5)?.map(from_ms),
                        user_action: r.get(6)?,
                    })
                },
            )
            .optional()
    }

    pub fn apply_engine_events(
        &self,
        events: &[EngineEvent],
        pause_meeting_id: Option<i64>,
    ) -> Result<()> {
        for event in events {
            match event {
                EngineEvent::BreakStarted {
                    at,
                    remaining_s,
                    resumed_chain,
                    ..
                } => {
                    if *resumed_chain {
                        self.reopen_break(*remaining_s, *at)?;
                    } else {
                        self.insert_break(*at, *remaining_s)?;
                    }
                }
                EngineEvent::BreakEnded {
                    at,
                    reason,
                    postpone_count,
                } => {
                    let outcome = match reason {
                        BreakEndReason::Completed => "completed",
                        BreakEndReason::Postponed { .. } => "postponed",
                        BreakEndReason::CancelledByPause => "cancelled_by_pause",
                    };
                    self.finish_break(*at, outcome, *postpone_count)?;
                }
                EngineEvent::PauseStarted(pause) => {
                    self.insert_pause(pause, pause_meeting_id)?;
                }
                EngineEvent::PauseEnded { at, early, .. } => {
                    if *early {
                        self.cancel_open_pause(*at)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn recover(&self, settings: Settings, now: Timestamp) -> Result<Recovered> {
        let Some(stored) = self.load_engine_state()? else {
            let closed_segments = self.close_dangling_segments(now)?;
            return Ok(Recovered {
                engine: Engine::new(settings),
                events: Vec::new(),
                closed_segments,
            });
        };
        let closed_segments = self.close_dangling_segments(stored.checkpointed_at)?;
        self.close_dangling_meetings(stored.checkpointed_at)?;
        let mut engine = Engine::with_state(settings, stored.state);
        let events = engine.tick(now, ActivityKind::Away);
        self.apply_engine_events(&events, None)?;
        self.checkpoint(engine.state(), now)?;
        Ok(Recovered {
            engine,
            events,
            closed_segments,
        })
    }
}

pub fn local_midnight<Tz: TimeZone>(tz: &Tz, now: Timestamp) -> Timestamp {
    let local_date = now.with_timezone(tz).date_naive();
    let naive = local_date.and_hms_opt(0, 0, 0).unwrap_or_default();
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now)
}

fn pause_from_row(r: &rusqlite::Row<'_>) -> Result<(i64, ActivePause)> {
    let preset: String = r.get(1)?;
    let source: String = r.get(5)?;
    Ok((
        r.get(0)?,
        ActivePause {
            preset: PausePreset::parse(&preset).ok_or_else(|| invalid(1, "preset"))?,
            frozen_load_percent: r.get(2)?,
            started_at: from_ms(r.get(3)?),
            until_at: from_ms(r.get(4)?),
            source: PauseSource::parse(&source).ok_or_else(|| invalid(5, "source"))?,
        },
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub struct BreakRow {
    pub id: i64,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    pub planned_seconds: i64,
    pub postpone_count: i64,
    pub outcome: Option<String>,
}

impl BreakRow {
    fn from_row(r: &rusqlite::Row<'_>) -> Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            started_at: from_ms(r.get(1)?),
            ended_at: r.get::<_, Option<i64>>(2)?.map(from_ms),
            planned_seconds: r.get(3)?,
            postpone_count: r.get(4)?,
            outcome: r.get(5)?,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PauseRow {
    pub id: i64,
    pub pause: ActivePause,
    pub cancelled_at: Option<Timestamp>,
    pub meeting_event_id: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeetingRow {
    pub id: i64,
    pub started_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    pub mic_used: bool,
    pub camera_used: bool,
    pub notified_at: Option<Timestamp>,
    pub user_action: String,
}
