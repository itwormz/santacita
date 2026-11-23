//! Active time is stored as a few long segments, not one row for every heartbeat: a search that runs for weeks
//! must not grow the database by a row every five seconds, and every statistics call reads all the rows.
use ceh_core::{
    model::{ProfileId, SessionId},
    statistics::EventScope,
};
use ceh_store::{periods::Period, statistics::StatisticsReader, writer::StatisticsWriter};

const BASE: i64 = 1_793_361_600_000; // 2026-10-30T12:00:00Z

fn scope(session: &str) -> EventScope {
    EventScope { profile: ProfileId::new("fixture").unwrap(), session: SessionId::new(session).unwrap() }
}
fn database() -> rusqlite::Connection {
    let db = rusqlite::Connection::open_in_memory().unwrap();
    db.execute_batch(include_str!("../src/schema.sql")).unwrap();
    db
}
fn rows(db: &rusqlite::Connection) -> i64 {
    db.query_row("SELECT COUNT(*) FROM active", [], |r| r.get(0)).unwrap()
}
fn total(db: &rusqlite::Connection) -> f64 {
    db.query_row("SELECT COALESCE(SUM(seconds),0) FROM active", [], |r| r.get(0)).unwrap()
}
fn counted(db: &rusqlite::Connection, at: i64) -> f64 {
    StatisticsReader::new(db)
        .active_seconds("fixture", Period::All, chrono::DateTime::from_timestamp_millis(at).unwrap())
        .unwrap()
}

#[test]
fn heartbeats_that_follow_each_other_extend_one_row() {
    let mut db = database();
    for step in 1..=100 {
        StatisticsWriter::new(&mut db).add_active(&scope("s"), 5.0, BASE + 5_000 * step).unwrap();
    }
    assert_eq!(rows(&db), 1, "100 contiguous heartbeats are one segment");
    assert!((total(&db) - 500.0).abs() < 1e-6);
    let (start, end): (String, String) =
        db.query_row("SELECT start,end FROM active", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    assert_eq!(start, "2026-10-30T12:00:00+00:00");
    assert_eq!(end, "2026-10-30T12:08:20+00:00");
    assert!((counted(&db, BASE + 600_000) - 500.0).abs() < 1e-6, "the statistics read the same time as before");
}

#[test]
fn a_gap_a_new_hour_or_another_session_starts_a_new_row() {
    let mut db = database();
    let mut writer = StatisticsWriter::new(&mut db);
    writer.add_active(&scope("s"), 5.0, BASE + 5_000).unwrap();
    writer.add_active(&scope("s"), 5.0, BASE + 10_000).unwrap(); // contiguous
    writer.add_active(&scope("s"), 5.0, BASE + 17_000).unwrap(); // 2 s gap: the phase was manual or paused
    writer.add_active(&scope("other"), 5.0, BASE + 22_000).unwrap(); // not this session's segment
    writer.add_active(&scope("s"), 5.0, BASE + 3_605_000).unwrap(); // past the hour of the row's start
    assert_eq!(rows(&db), 4);
    assert!((total(&db) - 25.0).abs() < 1e-6);
}

#[test]
fn a_small_gap_starts_a_new_row_so_the_time_before_an_event_stays_exact() {
    let mut db = database();
    let mut writer = StatisticsWriter::new(&mut db);
    writer.add_active(&scope("s"), 5.0, BASE + 5_000).unwrap();
    writer.add_active(&scope("s"), 4.9, BASE + 10_000).unwrap(); // starts 100 ms after the last end: not contiguous
    writer.add_active(&scope("s"), 5.0, BASE + 15_000).unwrap(); // contiguous with the second one
    assert_eq!(rows(&db), 2, "a gap of 100 ms is not hidden inside a row");
    assert!((total(&db) - 14.9).abs() < 1e-6);
}

#[test]
fn a_heartbeat_whose_wall_clock_went_back_still_counts() {
    let mut db = database();
    let mut writer = StatisticsWriter::new(&mut db);
    for step in 1..=6 {
        writer.add_active(&scope("s"), 5.0, BASE + 5_000 * step).unwrap(); // one row up to +30 s
    }
    writer.add_active(&scope("s"), 5.0, BASE + 20_000).unwrap(); // the clock was corrected back
    assert!((total(&db) - 35.0).abs() < 1e-6, "the five seconds are kept: {}", total(&db));
}

#[test]
fn old_one_row_per_heartbeat_data_is_merged_without_losing_time() {
    let mut db = database();
    // The way every earlier version wrote it: a row per heartbeat, with the small overlap of two clocks.
    let mut start = chrono::DateTime::from_timestamp_millis(BASE).unwrap();
    for _ in 0..1_000 {
        let end = start + chrono::Duration::microseconds(5_016_000);
        db.execute(
            "INSERT INTO active(profile,session,start,end,seconds) VALUES ('fixture','s',?1,?2,5.016)",
            rusqlite::params![
                start.to_rfc3339_opts(chrono::SecondsFormat::Micros, false),
                end.to_rfc3339_opts(chrono::SecondsFormat::Micros, false)
            ],
        )
        .unwrap();
        start = end - chrono::Duration::microseconds(35);
    }
    db.execute(
        "INSERT INTO active(profile,session,start,end,seconds) VALUES ('fixture','later','2026-11-02T08:00:00+00:00','2026-11-02T08:30:00+00:00',1800)",
        [],
    )
    .unwrap();
    let before = total(&db);
    let before_counted = counted(&db, BASE + 100_000_000);
    let removed = ceh_store::writer::compact_active(&mut db).unwrap();
    assert!(removed > 900, "removed {removed}");
    assert!(rows(&db) <= 8, "1 000 heartbeats (about 83 minutes) and one long segment, got {}", rows(&db));
    assert!((total(&db) - before).abs() < 1e-6);
    assert!((counted(&db, BASE + 100_000_000) - before_counted).abs() < 1e-3);
    assert_eq!(ceh_store::writer::compact_active(&mut db).unwrap(), 0, "a second pass has nothing to do");
}
