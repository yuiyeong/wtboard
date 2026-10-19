use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

use super::StoreError;

/// `state.sqlite` schema version (`PRAGMA user_version`). 0은 3단계의 `viewed`만 있는 파일이다
pub const SCHEMA_VERSION: i32 = 1;

/// 앱의 busy timeout. hook은 `open_existing`에 더 짧은 값을 넘긴다
const APP_BUSY: Duration = Duration::from_secs(5);

const MIGRATE_V1: &str = "BEGIN;
CREATE TABLE IF NOT EXISTS viewed (
    worktree  TEXT NOT NULL,
    branch    TEXT NOT NULL,
    path      TEXT NOT NULL,
    hash      TEXT NOT NULL,
    viewed_at INTEGER NOT NULL,
    PRIMARY KEY (worktree, branch, path)
);
CREATE TABLE IF NOT EXISTS comments (
    id         INTEGER PRIMARY KEY,
    worktree   TEXT NOT NULL,
    path       TEXT NOT NULL,
    side       TEXT NOT NULL,
    line       INTEGER NOT NULL,
    view       TEXT NOT NULL,
    snippet    TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    sent_at    INTEGER,
    sent_to    TEXT
);
CREATE INDEX IF NOT EXISTS comments_unsent ON comments (worktree, sent_at);
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
PRAGMA user_version = 1;
COMMIT;";

/// 앱이 `state.sqlite`를 연다. 부모 폴더와 파일을 만들고, WAL로 바꾸고, schema를 최신으로 올린다.
/// 이 앱보다 새 version이면 `NewerSchema`
pub fn open_state(path: &Path) -> Result<Connection, StoreError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(APP_BUSY)?;
    // 이미 WAL이면 건드리지 않는다. 설정하려면 쓰기 lock이 필요해서 다른 연결과 부딪힌다
    let mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
    if !mode.eq_ignore_ascii_case("wal") {
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
    }
    migrate(&conn)?;
    Ok(conn)
}

/// hook이 연다. 파일을 만들지 않고 schema도 올리지 않는다. 파일이 없거나 앱이 아직 schema 1로 올리지 않았으면 `None`
pub fn open_existing(path: &Path, busy: Duration) -> Result<Option<Connection>, StoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    conn.busy_timeout(busy)?;
    match user_version(&conn)? {
        v if v < SCHEMA_VERSION => Ok(None),
        v if v > SCHEMA_VERSION => Err(StoreError::NewerSchema(v)),
        _ => Ok(Some(conn)),
    }
}

/// 꺼지면 사라진다. 파일 DB를 열 수 없을 때와 테스트에서 쓴다
pub fn open_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory DB를 열 수 없음");
    migrate(&conn).expect("in-memory DB schema 생성 실패");
    conn
}

fn user_version(conn: &Connection) -> rusqlite::Result<i32> {
    conn.query_row("PRAGMA user_version", [], |r| r.get(0))
}

fn migrate(conn: &Connection) -> Result<(), StoreError> {
    let version = user_version(conn)?;
    if version > SCHEMA_VERSION {
        return Err(StoreError::NewerSchema(version));
    }
    if version < SCHEMA_VERSION {
        conn.execute_batch(MIGRATE_V1)?;
    }
    Ok(())
}
