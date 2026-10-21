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

const COLUMNS: &str = "id, worktree, path, side, line, view, snippet, body, created_at, updated_at, sent_at, sent_to, \
                       sent_turn, resolved_at";

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
    /// 첨부한 prompt가 연 turn. 복사(`copy`)로 보냈으면 없음
    pub sent_turn: Option<u32>,
    /// 사용자가 해결됨으로 바꾼 시각. 전송된 코멘트만 해결될 수 있다
    pub resolved_at: Option<u64>,
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
        sent_turn: r.get(12)?,
        resolved_at: r.get::<_, Option<i64>>(13)?.map(|v| v as u64),
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

/// 없으면 `NotFound`
fn fetch(conn: &Connection, id: i64) -> Result<Comment, StoreError> {
    get(conn, id)?.ok_or(StoreError::NotFound(id))
}

/// worktree의 미전송 코멘트를 `path, line, id` 순으로 읽는다. 첨부(`review::attach`)도 이 순서를 쓴다
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

    /// 미전송 → 전송됨 → 해결됨, 그 안에서 `path, line, id` 순
    pub fn list(&self, worktree: &str) -> Result<Vec<Comment>, StoreError> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(&format!(
            "SELECT {COLUMNS} FROM comments WHERE worktree = ?1
             ORDER BY resolved_at IS NOT NULL, sent_at IS NOT NULL, path, line, id"
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
        let comment = fetch(&conn, id)?;
        if changed == 0 {
            return Err(StoreError::AlreadySent);
        }
        Ok(comment)
    }

    /// 전송된 코멘트만 해결됨으로 바꾼다. 이미 해결됨이면 그대로 둔다
    pub fn resolve(&self, id: i64, now: u64) -> Result<Comment, StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE comments SET resolved_at = ?2 WHERE id = ?1 AND sent_at IS NOT NULL AND resolved_at IS NULL",
            params![id, now as i64],
        )?;
        let comment = fetch(&conn, id)?;
        if comment.sent_at.is_none() {
            return Err(StoreError::NotSent);
        }
        Ok(comment)
    }

    /// 해결됨을 되돌린다. 해결되지 않은 것이면 그대로
    pub fn unresolve(&self, id: i64) -> Result<Comment, StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute("UPDATE comments SET resolved_at = NULL WHERE id = ?1", params![id])?;
        fetch(&conn, id)
    }

    /// 전송된 코멘트를 미전송으로 되돌린다 (다음 prompt에 다시 붙는다). 이미 미전송이면 그대로
    pub fn resend(&self, id: i64, now: u64) -> Result<Comment, StoreError> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE comments SET sent_at = NULL, sent_to = NULL, sent_turn = NULL, resolved_at = NULL, updated_at = ?2
             WHERE id = ?1 AND sent_at IS NOT NULL",
            params![id, now as i64],
        )?;
        fetch(&conn, id)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(conn: &Connection, worktree: &str, path: &str, line: u32, sent_at: Option<i64>) -> i64 {
        conn.execute(
            "INSERT INTO comments (worktree, path, side, line, view, snippet, body, created_at, updated_at, sent_at)
             VALUES (?1, ?2, 'new', ?3, 'branch', '', 'b', 1, 1, ?4)",
            params![worktree, path, line, sent_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn unsent_in_is_unsent_only_ordered_by_path_line_id() {
        let conn = db::open_memory();
        let b3 = insert(&conn, "w", "b.rs", 3, None);
        let a9 = insert(&conn, "w", "a.rs", 9, None);
        let a2_first = insert(&conn, "w", "a.rs", 2, None);
        let a2_second = insert(&conn, "w", "a.rs", 2, None);
        insert(&conn, "w", "a.rs", 1, Some(100));
        insert(&conn, "other", "a.rs", 1, None);
        let ids: Vec<i64> = unsent_in(&conn, "w").unwrap().iter().map(|c| c.id).collect();
        assert_eq!(ids, vec![a2_first, a2_second, a9, b3]);
    }
}
