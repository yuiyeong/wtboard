use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::worktree::now_ms;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("저장 폴더를 만들 수 없음: {0}")]
    Io(#[from] std::io::Error),
    #[error("봤음 DB 오류: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS viewed (
    worktree  TEXT NOT NULL,
    branch    TEXT NOT NULL,
    path      TEXT NOT NULL,
    hash      TEXT NOT NULL,
    viewed_at INTEGER NOT NULL,
    PRIMARY KEY (worktree, branch, path)
)";

/// 파일별 "봤음" 상태. 체크한 시점의 diff hash를 (worktree, branch, path)마다 하나 둔다.
/// 연결 하나를 Mutex로 감싸 모든 `Worktree`가 `Arc`로 나눠 쓴다. DB는 레포 밖에 있다
#[derive(Debug)]
pub struct ViewedStore {
    conn: Mutex<Connection>,
}

impl ViewedStore {
    /// 부모 폴더가 없으면 만든다
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    /// 꺼지면 사라진다. 파일 DB를 열 수 없을 때와 테스트에서 쓴다
    pub fn in_memory() -> Self {
        let conn = Connection::open_in_memory().expect("in-memory DB를 열 수 없음");
        Self::init(conn).expect("in-memory DB schema 생성 실패")
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// path → 체크할 때의 hash
    pub fn load(&self, worktree: &str, branch: &str) -> Result<HashMap<String, String>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path, hash FROM viewed WHERE worktree = ?1 AND branch = ?2")?;
        let rows = stmt.query_map(params![worktree, branch], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set(&self, worktree: &str, branch: &str, path: &str, hash: &str) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "INSERT INTO viewed (worktree, branch, path, hash, viewed_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (worktree, branch, path) DO UPDATE SET hash = excluded.hash, viewed_at = excluded.viewed_at",
            params![worktree, branch, path, hash, now_ms() as i64],
        )?;
        Ok(())
    }

    pub fn remove(&self, worktree: &str, branch: &str, path: &str) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute(
            "DELETE FROM viewed WHERE worktree = ?1 AND branch = ?2 AND path = ?3",
            params![worktree, branch, path],
        )?;
        Ok(())
    }
}
