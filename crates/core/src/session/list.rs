//! 세션 목록. 세션을 worktree에 귀속시키고 활성 여부를 정한다. 세션 폴더는 읽기만 한다 (`cleanup` 제외).
//!
//! 세션은 그 세션 event의 경로 중 하나나 `project_dir`를 가진 worktree에 속한다. 경로는 그것을 담는 가장 깊은
//! worktree만 갖는다 (`<repo>/.claude/worktrees/<name>` 같은 안쪽 worktree의 파일은 바깥 worktree 것이 아니다).
//! 그래서 귀속을 정하는 함수는 모두 전체 worktree root 목록을 받는다. 한 세션이 여러 worktree에 속할 수 있다.
use std::collections::{BTreeSet, HashMap, HashSet};
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

/// `worktree_root`에 속한 세션들. 최근 활동 순. `all_roots`는 열려 있는 모든 worktree root
pub fn sessions_for(data_dir: &Path, worktree_root: &Path, all_roots: &[PathBuf], now: u64) -> Vec<SessionSummary> {
    let roots = with_root(worktree_root, all_roots);
    let mut out: Vec<SessionSummary> = session_dirs(data_dir)
        .into_iter()
        .filter_map(|dir| {
            let last = last_activity(&dir)?;
            let meta = dir.read_meta();
            let events = dir.events();
            let touches = touched(&events, worktree_root, &roots);
            let in_project = meta.as_ref().is_some_and(|m| project_owner(&roots, m) == Some(worktree_root));
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
    let files: BTreeSet<&Path> = file_paths(&events).collect();
    let mut owned: HashSet<&Path> = files.into_iter().filter_map(|p| owner(roots, p)).map(|(r, _)| r).collect();
    owned.extend(dir.read_meta().and_then(|m| project_owner(roots, &m)));
    roots.iter().filter(|r| owned.contains(r.as_path())).cloned().collect()
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
    events.iter().filter(|e| is_file_event(e)).flat_map(|e| e.paths.iter().map(PathBuf::as_path))
}

fn is_file_event(e: &Event) -> bool {
    matches!(e.kind, EventKind::Edit | EventKind::Bash)
}

/// 파일 event 경로 중 `root`가 가진 것 (`roots`에서 가장 깊은 root가 `root`). event 순서대로, 중복 포함.
/// 귀속은 경로마다 한 번만 정한다 (symlink를 푸는 경로가 있다)
pub(crate) fn touched(events: &[Event], root: &Path, roots: &[PathBuf]) -> Vec<Touch> {
    let mut rels: HashMap<&Path, Option<String>> = HashMap::new();
    let mut out = Vec::new();
    for e in events.iter().filter(|e| is_file_event(e)) {
        for abs in &e.paths {
            let rel = rels.entry(abs).or_insert_with(|| {
                owner(roots, abs).filter(|(r, rel)| *r == root && !rel.is_empty()).map(|(_, rel)| rel)
            });
            if let Some(rel) = rel {
                out.push(Touch { turn: e.turn.max(1), rel: rel.clone(), abs: abs.clone() });
            }
        }
    }
    out
}

/// `roots`에 `root`가 없으면 더한다
pub(crate) fn with_root(root: &Path, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = roots.to_vec();
    if !out.iter().any(|r| r == root) {
        out.push(root.to_path_buf());
    }
    out
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

/// 경로를 가진 worktree root와 그 상대 경로 (root 자신이면 `""`): `roots` 중 경로를 담는 가장 깊은 root.
/// 기록된 경로가 어느 root 아래도 아니면 symlink를 푼 경로로 다시 본다 (`/var/...` → `/private/var/...`).
/// worktree root는 canonical이다
fn owner<'a>(roots: &'a [PathBuf], path: &Path) -> Option<(&'a Path, String)> {
    deepest(roots, path).or_else(|| deepest(roots, &real_path(path)?))
}

fn deepest<'a>(roots: &'a [PathBuf], path: &Path) -> Option<(&'a Path, String)> {
    let root = roots.iter().filter(|r| path.starts_with(r)).max_by_key(|r| r.components().count())?;
    Some((root, path.strip_prefix(root).ok()?.to_string_lossy().into_owned()))
}

/// `project_dir`를 가진 worktree. 폴더 자체가 symlink일 수 있어 끝까지 푼다
fn project_owner<'a>(roots: &'a [PathBuf], meta: &Meta) -> Option<&'a Path> {
    owner(roots, &canonical(&meta.project_dir)).map(|(r, _)| r)
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
