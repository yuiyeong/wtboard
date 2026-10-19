//! 세션 목록. 세션을 worktree에 귀속시키고 활성 여부를 정한다. 세션 폴더는 읽기만 한다 (`cleanup` 제외).
//!
//! 세션은 그 세션 event의 경로 중 하나가 worktree root 아래이거나, `project_dir`가 root 아래인 worktree에 속한다.
//! 한 세션이 여러 worktree에 속할 수 있다.
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::format::mtime_ms;
use super::{Event, EventKind, Meta, SessionDir};
use crate::paths::{canonical, nfc};

/// 끝나지 않았고 마지막 활동이 이 시간 안이면 활성 세션이다.
/// SessionEnd 없이 죽은 Claude Code 세션을 끝난 것으로 보기 위한 것이다
pub const ACTIVE_WINDOW_MS: u64 = 30 * 60 * 1000;

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub started_at: u64,
    /// 마지막 활동 시각 (`events.jsonl` mtime)
    pub last_event_at: u64,
    pub active: bool,
    /// "마지막 응답" 탭이 보여 주는 turn (`target_turn`). 탭 label이 목록과 어긋나지 않게 meta가 아니라 이것을 쓴다
    pub turn: u32,
    /// `turn`이 진행 중인 turn인지
    pub turn_open: bool,
    /// 이 worktree 아래 event 경로 수
    pub touched_files: usize,
}

/// 세션 event가 건드린 worktree 파일 하나
#[derive(Debug, Clone)]
pub(crate) struct Touch {
    /// event의 turn. 첫 prompt 전(turn 0)은 turn 1에 합친다
    pub turn: u32,
    /// worktree 상대 경로 (NFC)
    pub rel: String,
    /// 기록된 절대 경로. 원본·스냅샷의 key다
    pub abs: PathBuf,
}

/// `worktree_root`에 속한 세션들. 최근 활동 순
pub fn sessions_for(data_dir: &Path, worktree_root: &Path, now: u64) -> Vec<SessionSummary> {
    let mut out: Vec<SessionSummary> = session_dirs(data_dir)
        .into_iter()
        .filter_map(|dir| {
            let last = last_activity(&dir)?;
            let meta = dir.read_meta();
            let events = dir.events();
            let touches = touched(&events, worktree_root);
            let in_project =
                meta.as_ref().is_some_and(|m| rel_under(worktree_root, &canonical(&m.project_dir)).is_some());
            if touches.is_empty() && !in_project {
                return None;
            }
            let (turn, turn_open) = target_turn(meta.as_ref(), &events, &touches);
            let touched_files = touches.iter().map(|t| &t.rel).collect::<HashSet<_>>().len();
            let started_at = meta.as_ref().map(|m| m.started_at).or_else(|| events.first().map(|e| e.t));
            Some(SessionSummary {
                id: dir.id().to_owned(),
                started_at: started_at.unwrap_or(last),
                last_event_at: last,
                active: is_active(meta.as_ref(), last, now),
                turn,
                turn_open,
                touched_files,
            })
        })
        .collect();
    out.sort_by(|a, b| b.last_event_at.cmp(&a.last_event_at).then_with(|| a.id.cmp(&b.id)));
    out
}

/// `roots` 중 세션이 속한 worktree. `roots` 순서대로
pub fn worktrees_of(dir: &SessionDir, roots: &[PathBuf]) -> Vec<PathBuf> {
    let events = dir.events();
    // 경로마다 symlink를 한 번만 푼다 (root 수만큼 풀지 않게). project_dir은 폴더 자체가 symlink일 수 있다
    let files: BTreeSet<&Path> = file_paths(&events).collect();
    let mut spellings: Vec<PathBuf> =
        files.into_iter().flat_map(|p| std::iter::once(p.to_path_buf()).chain(real_path(p))).collect();
    if let Some(meta) = dir.read_meta() {
        spellings.push(canonical(&meta.project_dir));
        spellings.push(meta.project_dir);
    }
    roots.iter().filter(|root| spellings.iter().any(|p| p.starts_with(root))).cloned().collect()
}

/// 활성 세션이 있는 worktree. meta와 mtime만 먼저 보고 활성인 세션만 events를 읽는다
pub fn active_roots(data_dir: &Path, roots: &[PathBuf], now: u64) -> HashSet<PathBuf> {
    session_dirs(data_dir)
        .iter()
        .filter(|dir| last_activity(dir).is_some_and(|last| is_active(dir.read_meta().as_ref(), last, now)))
        .flat_map(|dir| worktrees_of(dir, roots))
        .collect()
}

/// 마지막 활동이 `keep_days`보다 오래된 세션 폴더를 지운다. 지운 수
pub fn cleanup(data_dir: &Path, keep_days: u64, now: u64) -> usize {
    let cutoff = now.saturating_sub(keep_days.saturating_mul(DAY_MS));
    session_dirs(data_dir)
        .into_iter()
        .filter(|dir| last_activity(dir).is_some_and(|last| last < cutoff))
        .filter(|dir| match fs::remove_dir_all(dir.root()) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("[wtboard] 세션 기록을 지울 수 없음: {}: {e}", dir.root().display());
                false
            }
        })
        .count()
}

/// `<data dir>/sessions/` 아래의 세션 폴더. 폴더가 아니거나 (symlink 포함) 이름이 session id가 아니면 뺀다
fn session_dirs(data_dir: &Path) -> Vec<SessionDir> {
    let Ok(entries) = fs::read_dir(data_dir.join("sessions")) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| SessionDir::open(data_dir, e.file_name().to_str()?))
        .collect()
}

/// `events.jsonl` mtime. 아직 아무것도 없는 폴더는 폴더 자체의 mtime (막 만들어진 세션을 지우지 않게)
fn last_activity(dir: &SessionDir) -> Option<u64> {
    dir.last_activity().or_else(|| mtime_ms(dir.root()))
}

fn is_active(meta: Option<&Meta>, last: u64, now: u64) -> bool {
    meta.is_none_or(|m| m.ended_at.is_none()) && now.saturating_sub(last) <= ACTIVE_WINDOW_MS
}

/// 파일 event(`Edit`, `Bash`)의 경로
fn file_paths(events: &[Event]) -> impl Iterator<Item = &Path> {
    events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Edit | EventKind::Bash))
        .flat_map(|e| e.paths.iter().map(PathBuf::as_path))
}

/// 파일 event 경로 중 `root` 아래인 것. event 순서대로, 중복 포함
pub(crate) fn touched(events: &[Event], root: &Path) -> Vec<Touch> {
    events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Edit | EventKind::Bash))
        .flat_map(|e| e.paths.iter().map(move |p| (e.turn.max(1), p)))
        .filter_map(|(turn, abs)| {
            let rel = rel_under(root, abs).filter(|r| !r.is_empty())?;
            Some(Touch { turn, rel, abs: abs.clone() })
        })
        .collect()
}

/// "마지막 응답"이 보여 줄 turn과 진행 중 여부. 열린 turn에 이 worktree의 event(`touches`)가 있으면
/// 그 turn, 아니면 마지막으로 닫힌 turn (첫 prompt 전이면 0). meta가 없으면 event에 나온 가장 큰 turn을 닫힌 것으로 본다
pub(crate) fn target_turn(meta: Option<&Meta>, events: &[Event], touches: &[Touch]) -> (u32, bool) {
    let (turn, open) = match meta {
        Some(m) => (m.turn, m.turn_open),
        None => (events.iter().map(|e| e.turn).max().unwrap_or(0), false),
    };
    if !open {
        (turn, false)
    } else if touches.iter().any(|t| t.turn == turn) {
        (turn, true)
    } else {
        (turn.saturating_sub(1), false)
    }
}

/// `root` 아래면 상대 경로 (`root` 자신이면 `""`). 기록된 경로가 symlink를 거쳤으면
/// (`/var/...` ↔ `/private/var/...`) 푼 경로로 다시 본다. worktree root는 canonical이다
pub(crate) fn rel_under(root: &Path, path: &Path) -> Option<String> {
    let rel = |p: &Path| p.strip_prefix(root).ok().map(|r| r.to_string_lossy().into_owned());
    rel(path).or_else(|| rel(&real_path(path)?))
}

/// 경로의 폴더 부분 symlink를 푼 NFC 경로. 파일 자체의 symlink는 풀지 않는다 (worktree 안의 symlink가
/// 밖을 가리켜도 worktree 파일이다). 폴더가 없어졌으면 있는 조상까지 올라가서 푼다
fn real_path(path: &Path) -> Option<PathBuf> {
    let mut tail = Vec::new();
    let mut base = path;
    loop {
        tail.push(base.file_name()?);
        base = base.parent()?;
        if let Ok(mut real) = base.canonicalize() {
            real.extend(tail.iter().rev());
            return Some(nfc(&real));
        }
    }
}
