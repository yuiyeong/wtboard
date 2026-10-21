//! 리뷰 코멘트 → Claude에게 붙일 markdown 텍스트. hook(`attach`)과 앱("마크다운으로 복사")이 같은 `render`를 쓴다.
//! 전송된 코멘트에 Claude의 응답(turn)을 잇는 `reply_turns`도 여기 있다.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior};

use crate::session::list::touched_turns;
use crate::session::SessionDir;
use crate::store::comments::{unsent_in, Comment, Side};
use crate::store::StoreError;

/// 첨부 텍스트 길이 상한 (글자 수, byte 아님)
pub const ATTACH_CAP: usize = 9_000;

const CLOSE_TAG: &str = "</wtboard-review-comments>";
const ESCAPED_CLOSE_TAG: &str = "<\\/wtboard-review-comments>";
const GUIDE: &str =
    "사용자가 wtboard에서 diff를 보며 남긴 리뷰 코멘트입니다. 각 코멘트가 가리키는 코드를 확인하고 반영해 주세요.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    pub text: String,
    /// 텍스트에 들어간 코멘트 id (입력 순서)
    pub included: Vec<i64>,
    /// cap 때문에 들어가지 못한 코멘트 수
    pub remaining: usize,
}

/// 코멘트를 입력 순서대로 텍스트로 만든다. `cap`이 있으면 그 글자 수를 넘기기 직전에 멈춘다.
/// 코멘트가 없으면 `text`는 빈 문자열이다
pub fn render(worktree_root: &str, comments: &[Comment], cap: Option<usize>) -> Rendered {
    if comments.is_empty() {
        return Rendered { text: String::new(), included: Vec::new(), remaining: 0 };
    }
    let mut text = format!(
        "<wtboard-review-comments worktree=\"{}\">\n{GUIDE}\n",
        escape(worktree_root).replace('"', "&quot;")
    );
    let mut len = text.chars().count();
    let mut included = Vec::new();
    for (i, comment) in comments.iter().enumerate() {
        let entry = format!("\n{}. {}", i + 1, render_entry(comment));
        let entry_len = entry.chars().count();
        let left = comments.len() - i - 1;
        // 이 코멘트를 넣고도 남은 수 안내와 닫는 tag까지 cap 안에 들어가야 한다.
        // 하나도 못 넣으면 진행이 없으므로 첫 코멘트는 cap과 무관하게 넣는다
        let fits = cap.is_none_or(|cap| len + entry_len + tail(left).chars().count() <= cap);
        if !fits && !included.is_empty() {
            break;
        }
        text.push_str(&entry);
        len += entry_len;
        included.push(comment.id);
    }
    let remaining = comments.len() - included.len();
    text.push_str(&tail(remaining));
    Rendered { text, included, remaining }
}

/// hook이 사용자에게 보이는 `systemMessage`
pub fn system_message(included: usize, remaining: usize) -> String {
    let mut msg = format!("wtboard: 리뷰 코멘트 {included}개를 첨부했습니다");
    if remaining > 0 {
        msg.push_str(&format!(" ({remaining}개 남음)"));
    }
    msg
}

/// hook의 첨부. worktree의 미전송 코멘트를 transaction 안에서 읽고, `emit`이 성공한 뒤에만
/// 들어간 코멘트를 `sent_to = session_id`, `sent_turn = turn`(이 prompt가 여는 turn)으로 바꾼다.
/// 미전송이 없으면 `emit`을 부르지 않고 `None`. `emit`이 실패하면 아무것도 바꾸지 않는다.
/// 출력 뒤에 commit하므로 commit이 실패하면 다음에 다시 붙는다 (중복이 유실보다 낫다)
pub fn attach(
    conn: &mut Connection,
    worktree: &str,
    session_id: &str,
    turn: u32,
    now: u64,
    emit: impl FnOnce(&Rendered) -> std::io::Result<()>,
) -> Result<Option<Rendered>, StoreError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let unsent = unsent_in(&tx, worktree)?;
    if unsent.is_empty() {
        return Ok(None);
    }
    let rendered = render(worktree, &unsent, Some(ATTACH_CAP));
    emit(&rendered).map_err(StoreError::Emit)?;
    {
        let mut stmt = tx.prepare("UPDATE comments SET sent_at = ?2, sent_to = ?3, sent_turn = ?4 WHERE id = ?1")?;
        for id in &rendered.included {
            stmt.execute(rusqlite::params![id, now as i64, session_id, turn])?;
        }
    }
    tx.commit()?;
    Ok(Some(rendered))
}

/// 코멘트별 "응답 turn": `sent_to` 세션의 Edit·Bash event 중 `turn >= sent_turn`이고 이 worktree의 그 path를
/// 건드린 가장 큰 turn. `sent_turn`이 없는 것(복사로 보낸 것 포함), 해결됨(화면이 응답을 보이지 않는다),
/// 세션 폴더가 없거나 id가 잘못된 것은 빠진다. 세션마다 event를 한 번만 읽는다
pub fn reply_turns(
    data_dir: &Path,
    worktree_root: &Path,
    all_roots: &[PathBuf],
    comments: &[Comment],
) -> HashMap<i64, u32> {
    let mut by_session: HashMap<&str, Vec<(&Comment, u32)>> = HashMap::new();
    for c in comments.iter().filter(|c| c.resolved_at.is_none()) {
        if let (Some(session), Some(turn)) = (c.sent_to.as_deref(), c.sent_turn) {
            by_session.entry(session).or_default().push((c, turn));
        }
    }
    let mut out = HashMap::new();
    for (session, comments) in by_session {
        let Some(dir) = SessionDir::open(data_dir, session) else { continue };
        let turns = touched_turns(&dir, worktree_root, all_roots);
        for (c, sent_turn) in comments {
            if let Some(turn) = turns.get(&c.path).and_then(|t| t.range(sent_turn..).next_back()) {
                out.insert(c.id, *turn);
            }
        }
    }
    out
}

/// 코멘트 하나. 번호 뒤부터 (`path:line`, fence된 snippet, body)
fn render_entry(c: &Comment) -> String {
    let location = match c.side {
        Side::New => format!("{}:{}", escape(&c.path), c.line),
        Side::Old => format!("{} (삭제된 줄, 원래 {}번째 줄)", escape(&c.path), c.line),
    };
    let snippet = escape(&c.snippet);
    let fence = "`".repeat(longest_backtick_run(&snippet).max(2) + 1);
    format!("{location}\n{fence}\n{snippet}\n{fence}\n{}\n", escape(&c.body))
}

/// 남은 수 안내(있으면)와 닫는 tag
fn tail(remaining: usize) -> String {
    if remaining == 0 {
        CLOSE_TAG.to_string()
    } else {
        format!("\n(코멘트 {remaining}개가 더 있습니다. 다음 메시지에 붙습니다)\n{CLOSE_TAG}")
    }
}

/// 사용자 입력이 닫는 tag로 끝을 속이지 못하게 한다
fn escape(s: &str) -> String {
    s.replace(CLOSE_TAG, ESCAPED_CLOSE_TAG)
}

fn longest_backtick_run(s: &str) -> usize {
    s.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}
