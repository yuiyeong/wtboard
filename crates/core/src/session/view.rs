//! "이번 세션"·"마지막 응답" view의 변경 목록과 patch. git이 아니라 세션 기록(원본, turn 스냅샷)과
//! 비교하므로 세션 중 커밋·브랜치 전환과 무관하다. 세션 폴더는 읽기만 하고 임시 파일도 만들지 않는다.
//!
//! - 이번 세션: 원본(baseline) ↔ 현재 파일
//! - 마지막 응답: 그 파일의 이전 turn 스냅샷(없으면 원본) ↔ 대상 turn 스냅샷 (진행 중이면 현재 파일)
//!
//! 양쪽이 같으면 (되돌린 파일) 목록에서 뺀다.
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::format::hash_reader;
use super::list::{target_turn, touched, Touch};
use super::{Baseline, BaselineKind, SessionDir, Snapshot};
use crate::domain::{ChangedFile, FilePatch, FileStatus, PatchBody, MAX_DIFF_BYTES};
use crate::git::attrs::linguist_generated;
use crate::git::patch::{no_index_numstat, no_index_patch};
use crate::worktree::{FileEntry, ViewedState, Worktree, WorktreeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionView {
    /// 이번 세션
    Session,
    /// 마지막 응답
    Turn,
}

/// 비교의 한쪽 (before 또는 after)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Side {
    /// 내용 파일. `None`이면 파일 없음 (`too_large`면 내용 대신 `size`·`hash`)
    pub content: Option<PathBuf>,
    pub too_large: bool,
    /// `too_large`일 때 크기와 blake3 hash
    pub size: Option<u64>,
    pub hash: Option<String>,
}

impl Side {
    fn file(path: PathBuf) -> Self {
        Self { content: Some(path), ..Self::default() }
    }

    fn large(size: u64, hash: String) -> Self {
        Self { content: None, too_large: true, size: Some(size), hash: Some(hash) }
    }

    fn snapshot(snapshot: Snapshot) -> Self {
        match snapshot {
            Snapshot::Content(path) => Self::file(path),
            Snapshot::Deleted => Self::default(),
            Snapshot::TooLarge { size, hash } => Self::large(size, hash),
        }
    }

    /// 지금 worktree 파일. 일반 파일이 아니면 없는 것으로 본다. symlink는 대상 파일로 본다
    /// (hook의 원본·스냅샷 복사도 대상을 따른다. `git diff --no-index`는 링크 자체를 읽으므로 푼 경로를 넘긴다)
    fn current(path: &Path) -> io::Result<Self> {
        let meta = match fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            Ok(_) => return Ok(Self::default()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        if meta.len() > MAX_DIFF_BYTES {
            let (size, hash) = hash_reader(fs::File::open(path)?)?;
            return Ok(Self::large(size, hash));
        }
        let path = if fs::symlink_metadata(path)?.is_symlink() { path.canonicalize()? } else { path.to_path_buf() };
        Ok(Self::file(path))
    }

    fn exists(&self) -> bool {
        self.content.is_some() || self.too_large
    }

    fn len(&self) -> io::Result<Option<u64>> {
        match &self.content {
            Some(p) => Ok(Some(fs::metadata(p)?.len())),
            None => Ok(self.size),
        }
    }

    /// 내용 파일은 바이트로, 1MB 초과는 크기·hash로 비교한다
    fn same_as(&self, other: &Side) -> io::Result<bool> {
        Ok(match (&self.content, &other.content) {
            (Some(a), Some(b)) => self.len()? == other.len()? && fs::read(a)? == fs::read(b)?,
            (None, None) => self == other,
            _ => false,
        })
    }
}

/// view에 들어가는 파일 하나
struct Item {
    rel: String,
    before: Side,
    after: Side,
    uncertain: bool,
}

/// 경로순 변경 목록. status는 양쪽 유무로, 줄 수는 `git diff --no-index --numstat`으로 정한다.
/// 접기는 브랜치 view와 같은 규칙 (profile pattern, `linguist-generated`)
pub fn changes(dir: &SessionDir, view: SessionView, wt: &Worktree) -> Result<Vec<FileEntry>, WorktreeError> {
    let items = items(dir, view, wt, None)?;
    let git = wt.git();
    let rels: Vec<&str> = items.iter().map(|i| i.rel.as_str()).collect();
    // check-attr가 실패해도 목록은 보여 준다. 생성 파일이 아닌 것으로 본다
    let generated = linguist_generated(git, &rels).unwrap_or_default();
    items
        .into_iter()
        .map(|item| {
            let (additions, deletions) = if item.before.too_large || item.after.too_large {
                (None, None)
            } else {
                no_index_numstat(git, item.before.content.as_deref(), item.after.content.as_deref())?
            };
            let status = match (item.before.exists(), item.after.exists()) {
                (false, _) => FileStatus::Added,
                (_, false) => FileStatus::Deleted,
                _ => FileStatus::Modified,
            };
            let collapsed = wt.profile().collapses(&item.rel) || generated.contains(&item.rel);
            Ok(FileEntry {
                file: ChangedFile { path: item.rel, status, additions, deletions, collapsed },
                viewed: ViewedState::Unviewed,
                uncertain: item.uncertain,
            })
        })
        .collect()
}

/// 파일 하나의 patch. 목록에 없는 경로(건드리지 않음, 되돌림)면 `UnknownFile`
pub fn patch(dir: &SessionDir, view: SessionView, wt: &Worktree, rel: &str) -> Result<FilePatch, WorktreeError> {
    let item = items(dir, view, wt, Some(rel))?.pop().ok_or_else(|| WorktreeError::UnknownFile(rel.to_owned()))?;
    if !(item.before.too_large || item.after.too_large) {
        let (before, after) = (item.before.content.as_deref(), item.after.content.as_deref());
        return Ok(no_index_patch(wt.git(), rel, before, after)?);
    }
    let body = PatchBody::TooLarge { old_size: item.before.len()?, new_size: item.after.len()? };
    Ok(FilePatch { path: rel.to_owned(), body, hash: large_hash(&item)? })
}

/// view의 파일들 (`only`면 그 경로만). 경로순, 양쪽이 같은 파일은 뺀다
fn items(dir: &SessionDir, view: SessionView, wt: &Worktree, only: Option<&str>) -> io::Result<Vec<Item>> {
    let root = &wt.layout().root;
    let events = dir.events();
    let touches = touched(&events, root);
    let turn = match view {
        SessionView::Session => None,
        SessionView::Turn => Some(target_turn(dir.read_meta().as_ref(), &events, &touches)),
    };
    // 같은 파일을 여러 번 건드렸으면 처음 기록된 절대 경로를 쓴다
    let mut targets: BTreeMap<&str, &Touch> = BTreeMap::new();
    for t in &touches {
        if turn.is_none_or(|(n, _)| t.turn == n) && only.is_none_or(|rel| rel == t.rel) {
            targets.entry(t.rel.as_str()).or_insert(t);
        }
    }
    let baselines = dir.baselines();
    let mut out = Vec::new();
    for (rel, touch) in targets {
        let current = || Side::current(&root.join(rel));
        let (before, after, uncertain) = match turn {
            None => {
                let (before, uncertain) = baseline_side(dir, &baselines, &touch.abs);
                (before, current()?, uncertain)
            }
            Some((n, open)) => {
                // 이전 turn 스냅샷은 그 turn에 이 파일을 건드렸을 때만 있다. 가장 최근 것부터 본다
                let mut older: Vec<u32> =
                    touches.iter().filter(|t| t.rel == rel && t.turn < n).map(|t| t.turn).collect();
                older.sort_unstable_by(|a, b| b.cmp(a));
                older.dedup();
                let (before, uncertain) = match older.iter().find_map(|k| dir.turn_snapshot(*k, &touch.abs)) {
                    Some(s) => (Side::snapshot(s), false),
                    None => baseline_side(dir, &baselines, &touch.abs),
                };
                // 닫힌 turn에 스냅샷이 없으면 (스냅샷 뒤에 온 event, 경로 오류) 현재 파일로 본다
                let after = match dir.turn_snapshot(n, &touch.abs).filter(|_| !open) {
                    Some(s) => Side::snapshot(s),
                    None => current()?,
                };
                (before, after, uncertain)
            }
        };
        if !before.same_as(&after)? {
            out.push(Item { rel: rel.to_owned(), before, after, uncertain });
        }
    }
    Ok(out)
}

/// 원본과 그것이 불확실한지. `new`는 없는 파일, `uncertain`은 hook이 저장한 세션 시작 HEAD 내용.
/// 원본 기록이 없으면 (hook이 늦게 설치됐거나 실패, 기록 전에 죽어 내용 파일만 남음) 세션 전 내용을
/// 모르므로 없는 파일로 보고 불확실로 표시한다
fn baseline_side(dir: &SessionDir, baselines: &HashMap<PathBuf, Baseline>, abs: &Path) -> (Side, bool) {
    let Some(b) = baselines.get(abs) else { return (Side::default(), true) };
    match (b.kind, b.size, &b.hash) {
        (BaselineKind::New, ..) => (Side::default(), false),
        (BaselineKind::TooLarge, Some(size), Some(hash)) => (Side::large(size, hash.clone()), false),
        (BaselineKind::File | BaselineKind::Uncertain, ..) if dir.baseline_file(abs).is_file() => {
            (Side::file(dir.baseline_file(abs)), b.kind == BaselineKind::Uncertain)
        }
        _ => (Side::default(), true),
    }
}

/// 한쪽이라도 1MB를 넘는 파일의 hash: 양쪽 크기와 hash (내용 파일이면 그 내용)
fn large_hash(item: &Item) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    for side in [&item.before, &item.after] {
        hasher.update(&side.len()?.unwrap_or(u64::MAX).to_le_bytes());
        match &side.content {
            Some(p) => {
                hasher.update_reader(fs::File::open(p)?)?;
            }
            None => {
                hasher.update(side.hash.as_deref().unwrap_or_default().as_bytes());
            }
        }
    }
    Ok(hasher.finalize().to_hex().to_string())
}
