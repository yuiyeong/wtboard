use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use super::{db, StoreError};
use crate::worktree::now_ms;

/// 본문 길이 상한 (글자 수). 코멘트 하나가 첨부 cap 안에 들어가게 한다
const MAX_BODY_CHARS: usize = 4000;
const MAX_SNIPPET_CHARS: usize = 500;

const COLUMNS: &str =
    "id, worktree, path, side, line, view, snippet, body, created_at, updated_at, sent_at, sent_to";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Old,
    New,
}

impl Side {
    fn as_str(self) -> &'static str {
        match self {
            Side::Old => "old",
            Side::New => "new",
        }
    }
}

impl ToSql for Side {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

impl FromSql for Side {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value.as_str()? {
            "old" => Ok(Side::Old),
            "new" => Ok(Side::New),
            other => Err(FromSqlError::Other(format!("알 수 없는 side: {other}").into())),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: i64,
    pub worktree: String,
    pub path: String,
    pub side: Side,
    pub line: u32,
    pub view: String,
    pub snippet: String,
    pub body: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub sent_at: Option<u64>,
    pub sent_to: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewComment {
    pub worktree: String,
    pub path: String,
    pub side: Side,
    pub line: u32,
    pub view: String,
    pub snippet: String,
    pub body: String,
}

fn from_row(r: &Row) -> rusqlite::Result<Comment> {
    Ok(Comment {
        id: r.get(0)?,
        worktree: r.get(1)?,
        path: r.get(2)?,
        side: r.get(3)?,
        line: r.get(4)?,
        view: r.get(5)?,
        snippet: r.get(6)?,
        body: r.get(7)?,
        created_at: r.get::<_, i64>(8)? as u64,
        updated_at: r.get::<_, i64>(9)? as u64,
        sent_at: r.get::<_, Option<i64>>(10)?.map(|v| v as u64),
        sent_to: r.get(11)?,
    })
}

fn check_body(body: &str) -> Result<(), StoreError> {
    if body.trim().is_empty() {
        return Err(StoreError::InvalidBody("내용이 비어 있음".into()));
    }
    let chars = body.chars().count();
    if chars > MAX_BODY_CHARS {
        return Err(StoreError::InvalidBody(format!("{MAX_BODY_CHARS}자를 넘음 ({chars}자)")));
    }
    Ok(())
}

fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Comment>> {
    conn.query_row(&format!("SELECT {COLUMNS} FROM comments WHERE id = ?1"), params![id], from_row).optional()
}

/// worktree의 미전송 코멘트를 `path, line, id` 순으로 읽는다. 첨부(`review::attach`)도 이 순서를 쓴다
// review::attach가 쓰기 전까지는 호출처가 없다
#[allow(dead_code)]
pub(crate) fn unsent_in(conn: &Connection, worktree: &str) -> rusqlite::Result<Vec<Comment>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM comments WHERE worktree = ?1 AND sent_at IS NULL ORDER BY path, line, id"
    ))?;
    let rows = stmt.query_map(params![worktree], from_row)?;
    rows.collect()
}

/// worktree별 리뷰 코멘트. 연결 하나를 Mutex로 감싼다. 같은 파일을 hook도 다른 연결로 연다 (WAL)
#[derive(Debug)]
pub struct CommentStore {
    conn: Mutex<Connection>,
}

impl CommentStore {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Ok(Self { conn: Mutex::new(db::open_state(path)?) })
    }

    pub fn in_memory() -> Self {
        Self { conn: Mutex::new(db::open_memory()) }
    }

    pub fn add(&self, new: &NewComment) -> Result<Comment, StoreError> {
        check_body(&new.body)?;
        let snippet: String = new.snippet.chars().take(MAX_SNIPPET_CHARS).collect();
        let now = now_ms() as i64;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO comments (worktree, path, side, line, view, snippet, body, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![new.worktree, new.path, new.side, new.line, new.view, snippet, new.body, now],
        )?;
        let id = conn.last_insert_rowid();
        Ok(get(&conn, id)?.expect("방금 넣은 row"))
    }

    /// 미전송 먼저, 그 안에서 `path, line, id` 순
    pub fn list(&self, worktree: &str) -> Result<Vec<Comment>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM comments WHERE worktree = ?1
             ORDER BY sent_at IS NOT NULL, path, line, id"
        ))?;
        let rows = stmt.query_map(params![worktree], from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// 미전송 코멘트만 고칠 수 있다
    pub fn update_body(&self, id: i64, body: &str) -> Result<Comment, StoreError> {
        check_body(body)?;
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE comments SET body = ?2, updated_at = ?3 WHERE id = ?1 AND sent_at IS NULL",
            params![id, body, now_ms() as i64],
        )?;
        match (changed, get(&conn, id)?) {
            (0, Some(_)) => Err(StoreError::AlreadySent),
            (_, Some(comment)) => Ok(comment),
            (_, None) => Err(StoreError::NotFound(id)),
        }
    }

    /// 이미 없어도 오류가 아니다
    pub fn delete(&self, id: i64) -> Result<(), StoreError> {
        self.conn.lock().unwrap().execute("DELETE FROM comments WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 전송된 코멘트만 지운다. 지운 개수를 돌려준다
    pub fn clear_sent(&self, worktree: &str) -> Result<usize, StoreError> {
        Ok(self.conn.lock().unwrap().execute(
            "DELETE FROM comments WHERE worktree = ?1 AND sent_at IS NOT NULL",
            params![worktree],
        )?)
    }

    /// 아직 미전송인 것만 전송됨으로 바꾼다. 바뀐 개수를 돌려준다
    pub fn mark_sent(&self, ids: &[i64], sent_to: &str, now: u64) -> Result<usize, StoreError> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let mut changed = 0;
        {
            let mut stmt =
                tx.prepare("UPDATE comments SET sent_at = ?2, sent_to = ?3 WHERE id = ?1 AND sent_at IS NULL")?;
            for id in ids {
                changed += stmt.execute(params![id, now as i64, sent_to])?;
            }
        }
        tx.commit()?;
        Ok(changed)
    }

    /// worktree id → 미전송 코멘트 수 (0인 worktree는 없다)
    pub fn unsent_counts(&self) -> Result<HashMap<String, usize>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT worktree, count(*) FROM comments WHERE sent_at IS NULL GROUP BY worktree")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as usize)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// `sent_at`이 cutoff(Unix ms)보다 오래된 코멘트를 지운다. 지운 개수를 돌려준다
    pub fn purge_sent_before(&self, cutoff_ms: u64) -> Result<usize, StoreError> {
        Ok(self.conn.lock().unwrap().execute(
            "DELETE FROM comments WHERE sent_at IS NOT NULL AND sent_at < ?1",
            params![cutoff_ms as i64],
        )?)
    }
}
