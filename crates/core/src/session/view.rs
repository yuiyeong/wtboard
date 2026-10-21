//! "이번 세션"·"마지막 응답" view의 변경 목록과 patch. git이 아니라 세션 기록(원본, turn 스냅샷)과
//! 비교하므로 세션 중 커밋·브랜치 전환과 무관하다. 세션 폴더는 읽기만 하고 임시 파일도 만들지 않는다.
//!
//! - 이번 세션: 원본(baseline) ↔ 현재 파일
//! - 마지막 응답: 그 파일의 이전 turn 스냅샷(없으면 원본) ↔ 대상 turn 스냅샷 (진행 중이면 현재 파일)
//!
//! 양쪽이 같으면 (되돌린 파일) 목록에서 뺀다. 파일 하나를 읽지 못해도 (권한, symlink loop 등) 목록은 보여 준다.
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::SystemTime;

use super::list::{target_turn, touched, turns_by_rel, with_root, Touch};
use super::{Baseline, BaselineKind, SessionDir, Snapshot, TurnIndex};
use crate::domain::{ChangedFile, FilePatch, FileStatus, PatchBody, MAX_DIFF_BYTES};
use crate::git::attrs::linguist_generated;
use crate::git::{Git, GitError};
use crate::git::patch::{file_digest, hash_reader, no_index_numstat, no_index_patch, sides_hash};
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

    /// 지금 worktree 파일. 일반 파일이 아니면 (상위 폴더가 파일로 바뀐 경우 포함) 없는 것으로 본다. symlink는 대상 파일로 본다
    /// (hook의 원본·스냅샷 복사도 대상을 따른다. `git diff --no-index`는 링크 자체를 읽으므로 푼 경로를 넘긴다)
    fn current(path: &Path) -> io::Result<Self> {
        let meta = match fs::metadata(path) {
            Ok(m) if m.is_file() => m,
            Ok(_) => return Ok(Self::default()),
            Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {
                return Ok(Self::default())
            }
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

    /// 내용의 (크기, blake3 hash). 세션 기록의 `too_large`와 같은 방식이다
    fn digest(&self) -> io::Result<Option<(u64, String)>> {
        match (&self.content, self.size, &self.hash) {
            (Some(p), ..) => file_digest(p),
            (None, Some(size), Some(hash)) => Ok(Some((size, hash.clone()))),
            _ => Ok(None),
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

/// 내용 파일 하나의 (경로, 크기, mtime). 이것이 그대로면 내용도 그대로라고 본다
type FileKey = (PathBuf, u64, SystemTime);

/// (before, after). `None`은 파일 없음
type PairKey = (Option<FileKey>, Option<FileKey>);

/// cache가 이보다 커지면 비운다 (오래된 세션·worktree의 항목이 쌓이지 않게)
const CACHE_CAP: usize = 4096;

/// before·after 비교 결과
#[derive(Debug, Clone, Copy)]
struct Compared {
    same: bool,
    /// `same`이 아닐 때 처음 물으면 채운다
    numstat: Option<Lines>,
}

type Lines = (Option<u32>, Option<u32>);

/// 파일 쌍의 비교 결과 (같은지, 줄 수). 앱이 하나를 들고 매번 `changes`에 넘긴다.
/// 양쪽 내용 파일의 (경로, 크기, mtime)이 그대로면 파일을 다시 읽거나 `git diff`를 부르지 않는다
/// (목록은 파일이 바뀔 때마다 다시 계산하는데, 바뀐 파일은 대개 하나다)
#[derive(Debug, Default)]
pub struct SessionViewCache {
    entries: Mutex<HashMap<PairKey, Compared>>,
    computed: AtomicUsize,
}

impl SessionViewCache {
    /// 지금까지 실제로 한 비교 (내용 읽기, `git diff`) 수. cache를 거친 것은 세지 않는다
    pub fn computed(&self) -> usize {
        self.computed.load(Ordering::Relaxed)
    }

    /// `same_as`. 1MB 초과 쪽이 있으면 크기·hash만 비교하므로 cache를 쓰지 않는다
    fn same(&self, before: &Side, after: &Side) -> io::Result<bool> {
        let Some(key) = key(before, after) else { return before.same_as(after) };
        if let Some(c) = self.get(&key) {
            return Ok(c.same);
        }
        let same = before.same_as(after)?;
        self.put(key, Compared { same, numstat: None });
        Ok(same)
    }

    /// 두 내용 파일의 `git diff --no-index --numstat`. 오류는 cache하지 않는다
    fn numstat(&self, git: &Git, before: &Side, after: &Side) -> Result<Lines, GitError> {
        let run = || no_index_numstat(git, before.content.as_deref(), after.content.as_deref());
        let Some(key) = key(before, after) else { return run() };
        if let Some(numstat) = self.get(&key).and_then(|c| c.numstat) {
            return Ok(numstat);
        }
        let numstat = run()?;
        self.put(key, Compared { same: false, numstat: Some(numstat) });
        Ok(numstat)
    }

    fn get(&self, key: &PairKey) -> Option<Compared> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).get(key).copied()
    }

    fn put(&self, key: PairKey, value: Compared) {
        self.computed.fetch_add(1, Ordering::Relaxed);
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if entries.len() >= CACHE_CAP {
            entries.clear();
        }
        entries.insert(key, value);
    }
}

/// cache key. 1MB 초과 쪽이 있거나 파일 정보를 못 읽으면 `None` (cache하지 않는다)
fn key(before: &Side, after: &Side) -> Option<PairKey> {
    if before.too_large || after.too_large {
        return None;
    }
    let file = |side: &Side| -> io::Result<Option<FileKey>> {
        let Some(path) = &side.content else { return Ok(None) };
        let meta = fs::metadata(path)?;
        Ok(Some((path.clone(), meta.len(), meta.modified()?)))
    };
    Some((file(before).ok()?, file(after).ok()?))
}

/// view에 들어가는 파일 하나
struct Item {
    before: Side,
    after: Side,
    uncertain: bool,
}

/// 경로순 변경 목록. status는 양쪽 유무로, 줄 수는 `git diff --no-index --numstat`으로 정한다.
/// 접기는 브랜치 view와 같은 규칙 (profile pattern, `linguist-generated`).
/// 읽지 못한 파일은 줄 수 없이 `uncertain`으로 남긴다 (patch를 열면 그 오류가 보인다).
/// `all_roots`는 열려 있는 모든 worktree root (안쪽 worktree의 파일을 빼는 데 쓴다).
/// `cache`는 호출 사이에 같은 것을 넘긴다
pub fn changes(
    dir: &SessionDir,
    view: SessionView,
    wt: &Worktree,
    all_roots: &[PathBuf],
    cache: &SessionViewCache,
) -> Result<Vec<FileEntry>, WorktreeError> {
    let items = items(dir, view, wt, all_roots, None, cache);
    let git = wt.git();
    let rels: Vec<&str> = items.iter().map(|(rel, _)| rel.as_str()).collect();
    // check-attr가 실패해도 목록은 보여 준다. 생성 파일이 아닌 것으로 본다
    let generated = linguist_generated(git, &rels).unwrap_or_default();
    let warn = |rel: &str, e: &dyn std::fmt::Display| {
        eprintln!("[wtboard] {}: 세션 변경을 계산할 수 없음: {e}", wt.layout().root.join(rel).display());
    };
    Ok(items
        .into_iter()
        .map(|(rel, item)| {
            let (status, additions, deletions, uncertain) = match item {
                Ok(item) => {
                    let (additions, deletions) = if item.before.too_large || item.after.too_large {
                        (None, None)
                    } else {
                        cache.numstat(git, &item.before, &item.after).unwrap_or_else(|e| {
                            warn(&rel, &e);
                            (None, None)
                        })
                    };
                    let status = match (item.before.exists(), item.after.exists()) {
                        (false, _) => FileStatus::Added,
                        (_, false) => FileStatus::Deleted,
                        _ => FileStatus::Modified,
                    };
                    (status, additions, deletions, item.uncertain)
                }
                Err(e) => {
                    warn(&rel, &e);
                    (FileStatus::Modified, None, None, true)
                }
            };
            let collapsed = wt.profile().collapses(&rel) || generated.contains(&rel);
            FileEntry {
                file: ChangedFile { path: rel, status, additions, deletions, collapsed },
                viewed: ViewedState::Unviewed,
                uncertain,
            }
        })
        .collect())
}

/// 파일 하나의 patch. 목록에 없는 경로(건드리지 않음, 되돌림)면 `UnknownFile`, 읽지 못하면 그 오류
pub fn patch(
    dir: &SessionDir,
    view: SessionView,
    wt: &Worktree,
    rel: &str,
    all_roots: &[PathBuf],
) -> Result<FilePatch, WorktreeError> {
    // 파일 하나라 cache하지 않는다
    let (_, item) = items(dir, view, wt, all_roots, Some(rel), &SessionViewCache::default())
        .pop()
        .ok_or_else(|| WorktreeError::UnknownFile(rel.to_owned()))?;
    let item = item?;
    if !(item.before.too_large || item.after.too_large) {
        let (before, after) = (item.before.content.as_deref(), item.after.content.as_deref());
        return Ok(no_index_patch(wt.git(), rel, before, after)?);
    }
    let body = PatchBody::TooLarge { old_size: item.before.len()?, new_size: item.after.len()? };
    let hash = sides_hash([item.before.digest()?, item.after.digest()?]);
    Ok(FilePatch { path: rel.to_owned(), body, hash })
}

/// view의 파일들 (`only`면 그 경로만). 경로순, 양쪽이 같은 파일은 뺀다. 파일마다 따로 읽어 한 파일의 오류가 목록을 막지 않는다
fn items(
    dir: &SessionDir,
    view: SessionView,
    wt: &Worktree,
    all_roots: &[PathBuf],
    only: Option<&str>,
    cache: &SessionViewCache,
) -> Vec<(String, io::Result<Item>)> {
    let root = &wt.layout().root;
    let events = dir.events();
    let touches = touched(&events, root, &with_root(root, all_roots));
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
    let turns_of = turns_by_rel(&touches);
    let mut cx = Context { dir, root, turns_of, baselines: dir.baselines(), turns: HashMap::new(), cache };
    targets
        .into_iter()
        .filter_map(|(rel, touch)| match cx.item(turn, touch) {
            Ok(Some(item)) => Some((rel.to_owned(), Ok(item))),
            Ok(None) => None,
            Err(e) => Some((rel.to_owned(), Err(e))),
        })
        .collect()
}

/// 한 번의 계산 동안 읽어 둔 세션 기록
struct Context<'a> {
    dir: &'a SessionDir,
    root: &'a Path,
    /// 파일마다 건드린 turn들
    turns_of: HashMap<&'a str, BTreeSet<u32>>,
    baselines: HashMap<PathBuf, Baseline>,
    /// turn별 index는 한 번만 읽는다
    turns: HashMap<u32, TurnIndex>,
    cache: &'a SessionViewCache,
}

impl Context<'_> {
    /// 파일 하나의 before·after. 양쪽이 같으면 `None`. `turn`은 마지막 응답 view의 (대상 turn, 진행 중)
    fn item(&mut self, turn: Option<(u32, bool)>, touch: &Touch) -> io::Result<Option<Item>> {
        let (before, uncertain) = match turn {
            None => self.baseline(&touch.abs),
            Some((n, _)) => {
                // 이전 turn 스냅샷은 그 turn에 이 파일을 건드렸을 때만 있다. 가장 최근 것부터 본다
                let older: Vec<u32> = self.turns_of[touch.rel.as_str()].range(..n).rev().copied().collect();
                match older.into_iter().find_map(|k| self.snapshot(k, &touch.abs)) {
                    Some(s) => (Side::snapshot(s), false),
                    None => self.baseline(&touch.abs),
                }
            }
        };
        // 닫힌 turn은 그 turn의 스냅샷. 없으면 (스냅샷 뒤에 온 event, 경로 오류) 현재 파일로 본다
        let after = match turn {
            Some((n, false)) => self.snapshot(n, &touch.abs),
            _ => None,
        };
        let after = match after {
            Some(s) => Side::snapshot(s),
            None => Side::current(&self.root.join(&touch.rel))?,
        };
        Ok((!self.cache.same(&before, &after)?).then_some(Item { before, after, uncertain }))
    }

    fn snapshot(&mut self, turn: u32, abs: &Path) -> Option<Snapshot> {
        self.turns.entry(turn).or_insert_with(|| self.dir.turn_index(turn)).get(abs)
    }

    /// 원본과 그것이 불확실한지. `new`는 없는 파일, `uncertain`은 hook이 저장한 세션 시작 HEAD 내용.
    /// 원본 기록이 없으면 (hook이 늦게 설치됐거나 실패, 기록 전에 죽어 내용 파일만 남음) 세션 전 내용을
    /// 모르므로 없는 파일로 보고 불확실로 표시한다
    fn baseline(&self, abs: &Path) -> (Side, bool) {
        let Some(b) = self.baselines.get(abs) else { return (Side::default(), true) };
        let file = self.dir.baseline_file(abs);
        match (b.kind, b.size, &b.hash) {
            (BaselineKind::New, ..) => (Side::default(), false),
            (BaselineKind::TooLarge, Some(size), Some(hash)) => (Side::large(size, hash.clone()), false),
            (BaselineKind::File | BaselineKind::Uncertain, ..) if file.is_file() => {
                (Side::file(file), b.kind == BaselineKind::Uncertain)
            }
            _ => (Side::default(), true),
        }
    }
}
