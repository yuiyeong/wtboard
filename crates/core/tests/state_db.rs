use std::time::Duration;

use rusqlite::Connection;
use wtboard_core::store::db::{open_existing, open_state, SCHEMA_VERSION};
use wtboard_core::store::viewed::ViewedStore;
use wtboard_core::store::StoreError;

fn user_version(conn: &Connection) -> i32 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

#[test]
fn fresh_file_is_version_1_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/state.sqlite");
    let conn = open_state(&path).unwrap();
    assert_eq!(SCHEMA_VERSION, 1);
    assert_eq!(user_version(&conn), 1);
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    for table in ["viewed", "comments", "settings"] {
        let n: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "{table} table");
    }
}

#[test]
fn migrates_v0_keeps_viewed_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    {
        // 3단계가 만든 파일: viewed만 있고 user_version은 0
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE viewed (
                worktree  TEXT NOT NULL,
                branch    TEXT NOT NULL,
                path      TEXT NOT NULL,
                hash      TEXT NOT NULL,
                viewed_at INTEGER NOT NULL,
                PRIMARY KEY (worktree, branch, path)
            );
            INSERT INTO viewed VALUES ('w', 'feat', 'a.py', 'h1', 1);",
        )
        .unwrap();
        assert_eq!(user_version(&conn), 0);
    }
    let conn = open_state(&path).unwrap();
    assert_eq!(user_version(&conn), 1);
    drop(conn);
    let viewed = ViewedStore::open(&path).unwrap().load("w", "feat").unwrap();
    assert_eq!(viewed.get("a.py").map(String::as_str), Some("h1"));
}

#[test]
fn newer_version_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    Connection::open(&path).unwrap().pragma_update(None, "user_version", 2).unwrap();
    assert!(matches!(open_state(&path), Err(StoreError::NewerSchema(2))));
    assert!(matches!(ViewedStore::open(&path), Err(StoreError::NewerSchema(2))));
}

#[test]
fn open_existing_never_creates() {
    let dir = tempfile::tempdir().unwrap();
    let busy = Duration::from_secs(1);

    let missing = dir.path().join("none/state.sqlite");
    assert!(open_existing(&missing, busy).unwrap().is_none());
    assert!(!missing.exists());
    assert!(!missing.parent().unwrap().exists());

    let v0 = dir.path().join("v0.sqlite");
    Connection::open(&v0).unwrap().execute_batch("CREATE TABLE viewed (x)").unwrap();
    assert!(open_existing(&v0, busy).unwrap().is_none());

    let ready = dir.path().join("ready.sqlite");
    drop(open_state(&ready).unwrap());
    let conn = open_existing(&ready, busy).unwrap().expect("schema 1 파일은 열린다");
    assert_eq!(user_version(&conn), 1);
}

#[test]
fn reopening_wal_file_needs_no_write_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let first = open_state(&path).unwrap();
    // 다른 연결이 쓰기 lock을 쥔 동안에도 열린다 (hook이 앱과 동시에 열 때)
    first.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    let second = open_state(&path).unwrap();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(user_version(&second), 1);
    let existing = open_existing(&path, Duration::from_millis(200)).unwrap();
    assert!(existing.is_some());
    first.execute_batch("COMMIT").unwrap();
}
