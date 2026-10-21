use std::time::Duration;

use rusqlite::Connection;
use wtboard_core::store::comments::CommentStore;
use wtboard_core::store::db::{open_existing, open_state, SCHEMA_VERSION};
use wtboard_core::store::viewed::ViewedStore;
use wtboard_core::store::StoreError;

fn user_version(conn: &Connection) -> i32 {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap()
}

fn column_names(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})")).unwrap();
    stmt.query_map([], |r| r.get::<_, String>(1)).unwrap().collect::<Result<_, _>>().unwrap()
}

/// 5단계 앱이 만든 version 1 파일. comments·settings는 있고 새 column은 없다. 전송된 코멘트 하나
const V1: &str = "CREATE TABLE viewed (worktree TEXT NOT NULL, branch TEXT NOT NULL, path TEXT NOT NULL, hash TEXT NOT NULL, viewed_at INTEGER NOT NULL, PRIMARY KEY (worktree, branch, path));
CREATE TABLE comments (id INTEGER PRIMARY KEY, worktree TEXT NOT NULL, path TEXT NOT NULL, side TEXT NOT NULL, line INTEGER NOT NULL, view TEXT NOT NULL, snippet TEXT NOT NULL, body TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, sent_at INTEGER, sent_to TEXT);
CREATE INDEX comments_unsent ON comments (worktree, sent_at);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO comments (worktree, path, side, line, view, snippet, body, created_at, updated_at, sent_at, sent_to) VALUES ('w', 'a.py', 'new', 1, 'branch', 'x', 'b', 1, 1, 100, 's1');
PRAGMA user_version = 1;";

fn write_v1(path: &std::path::Path) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(V1).unwrap();
    assert_eq!(user_version(&conn), 1);
}

#[test]
fn fresh_file_is_version_2_wal() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sub/state.sqlite");
    let conn = open_state(&path).unwrap();
    assert_eq!(SCHEMA_VERSION, 2);
    assert_eq!(user_version(&conn), 2);
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
    assert_eq!(mode, "wal");
    for table in ["viewed", "comments", "settings"] {
        let n: i64 = conn
            .query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1", [table], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "{table} table");
    }
    let columns = column_names(&conn, "comments");
    assert!(columns.iter().any(|c| c == "sent_turn"), "{columns:?}");
    assert!(columns.iter().any(|c| c == "resolved_at"), "{columns:?}");
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
    assert_eq!(user_version(&conn), 2);
    assert!(column_names(&conn, "comments").iter().any(|c| c == "resolved_at"));
    drop(conn);
    let viewed = ViewedStore::open(&path).unwrap().load("w", "feat").unwrap();
    assert_eq!(viewed.get("a.py").map(String::as_str), Some("h1"));
}

#[test]
fn migrates_v1_keeps_comment_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    write_v1(&path);
    let conn = open_state(&path).unwrap();
    assert_eq!(user_version(&conn), 2);
    drop(conn);
    let list = CommentStore::open(&path).unwrap().list("w").unwrap();
    assert_eq!(list.len(), 1);
    let c = &list[0];
    assert_eq!((c.sent_at, c.sent_to.as_deref(), c.sent_turn, c.resolved_at), (Some(100), Some("s1"), None, None));
}

#[test]
fn newer_version_is_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    Connection::open(&path).unwrap().pragma_update(None, "user_version", 3).unwrap();
    assert!(matches!(open_state(&path), Err(StoreError::NewerSchema(3))));
    assert!(matches!(ViewedStore::open(&path), Err(StoreError::NewerSchema(3))));
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
    let conn = open_existing(&ready, busy).unwrap().expect("schema 2 파일은 열린다");
    assert_eq!(user_version(&conn), 2);
}

/// 옛 hook(schema 1)이 새 DB를 만나는 경우와 새 hook이 옛 DB를 만나는 경우를 hook 쪽 규칙으로 가른다
#[test]
fn open_existing_rejects_other_versions() {
    let dir = tempfile::tempdir().unwrap();
    let busy = Duration::from_secs(1);

    // 새 hook + 아직 앱이 올리지 않은 version 1 파일: 열지 않고 만들지도 않는다
    let v1 = dir.path().join("v1.sqlite");
    write_v1(&v1);
    assert!(open_existing(&v1, busy).unwrap().is_none());
    assert_eq!(user_version(&Connection::open(&v1).unwrap()), 1);

    // 더 새 version: 오류
    let v3 = dir.path().join("v3.sqlite");
    Connection::open(&v3).unwrap().pragma_update(None, "user_version", 3).unwrap();
    assert!(matches!(open_existing(&v3, busy), Err(StoreError::NewerSchema(3))));
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
    assert_eq!(user_version(&second), 2);
    let existing = open_existing(&path, Duration::from_millis(200)).unwrap();
    assert!(existing.is_some());
    first.execute_batch("COMMIT").unwrap();
}
