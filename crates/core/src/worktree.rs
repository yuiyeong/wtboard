use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::domain::{ChangedFile, FilePatch};
use crate::gh::Gh;
use crate::git::attrs::linguist_generated;
use crate::git::base::{merge_base, remote_branch, resolve_base, Base, BaseError, BaseSource};
use crate::git::changes::list_changes;
use crate::git::patch::file_patch;
use crate::git::{Git, GitError};
use crate::profile::Profile;
use crate::store::viewed::{StoreError, ViewedStore};
use crate::watch::{Target, WatchLayout};

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("경로가 없음: {0}")]
    Missing(PathBuf),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Base(#[from] BaseError),
    #[error("변경 목록에 없는 파일: {0}")]
    UnknownFile(String),
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// 재계산 범위. 여러 개를 합칠 때는 큰 쪽을 따른다 (`File < List < Base`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeScope {
    File,
    List,
    Base,
}

/// 파일의 봤음 상태. 체크할 때 저장한 hash와 현재 hash를 비교한다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ViewedState {
    /// 체크하지 않음
    #[serde(rename = "none")]
    Unviewed,
    #[serde(rename = "viewed")]
    Viewed,
    /// 체크한 뒤 diff가 바뀜 (다시 바뀜)
    #[serde(rename = "changed")]
    Changed,
}

/// 변경 파일 하나와 그 봤음 상태.
/// `ChangedFile`에 넣지 않는다. 목록 재계산이 `ChangedFile` 비교로 바뀐 파일을 찾는다
#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    #[serde(flatten)]
    pub file: ChangedFile,
    pub viewed: ViewedState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeChanged {
    pub worktree_id: String,
    pub changed_paths: Vec<String>,
    pub scope: ChangeScope,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub id: String,
    pub path: String,
    pub branch: Option<String>,
    pub base: String,
    pub base_source: BaseSource,
    pub changed_files: usize,
    /// 변경 파일 중 봤음이 아닌 수 (미확인 + 다시 바뀜). 접힌 파일도 센다
    pub unviewed_files: usize,
    pub profile: Profile,
    /// 앱이 마지막으로 변경을 알린 시각 (Unix ms). 시작 직후에는 없다
    pub last_changed_at: Option<u64>,
    /// 마지막 전체 목록 계산 시간. 느릴 때만 있다 (fsmonitor 안내용)
    pub slow_list_ms: Option<u64>,
    pub fsmonitor: bool,
    /// 열지 못한 이유. 있으면 나머지 필드는 기본값이다
    pub error: Option<String>,
}

impl WorktreeSummary {
    pub fn failed(path: &Path, branch: Option<String>, error: String) -> Self {
        let path = path.to_string_lossy().into_owned();
        Self {
            id: path.clone(),
            path,
            branch,
            base: String::new(),
            base_source: BaseSource::Origin,
            changed_files: 0,
            unviewed_files: 0,
            profile: Profile::Default,
            last_changed_at: None,
            slow_list_ms: None,
            fsmonitor: false,
            error: Some(error),
        }
    }
}

/// 전체 목록 계산이 이보다 오래 걸리면 fsmonitor 안내를 띄운다. `WTBOARD_SLOW_LIST_MS`로 바꿀 수 있다 (UI 확인용)
const SLOW_LIST_MS: u64 = 500;

/// 한 번의 git 호출에 pathspec으로 넘길 최대 경로 수
const MAX_PATHS_PER_BATCH: usize = 200;

struct State {
    base: Base,
    merge_base: String,
    files: BTreeMap<String, ChangedFile>,
    /// 화면이 patch를 가져간 파일과 봤음 체크한 파일의 마지막 hash.
    /// 불변식: `viewed`에 있고 `files`에도 있는 파일은 여기 현재 hash가 있다
    hashes: HashMap<String, String>,
    /// 봤음 row를 읽은 브랜치. detached HEAD면 `""`
    branch_key: String,
    /// 봤음 row: path → 체크할 때의 hash. 목록에서 빠진 파일의 row도 남겨 둔다
    viewed: HashMap<String, String>,
}

impl State {
    fn viewed_state(&self, path: &str) -> ViewedState {
        match self.viewed.get(path) {
            None => ViewedState::Unviewed,
            Some(h) if self.hashes.get(path) == Some(h) => ViewedState::Viewed,
            Some(_) => ViewedState::Changed,
        }
    }
}

/// 재계산 상태와 따로 쌓이는 정보. lock 순서: state → meta (반대로 잡지 않는다)
#[derive(Default)]
struct Meta {
    list_ms: u64,
    last_changed_at: Option<u64>,
    /// gh가 알려 준 PR base를 로컬 remote 브랜치로 바꾼 것 (`origin/develop`)
    pr_base: Option<String>,
    /// PR base를 마지막으로 물어본 브랜치
    pr_checked: Option<String>,
}

pub struct Worktree {
    id: String,
    git: Git,
    layout: WatchLayout,
    profile: Profile,
    configured_base: Option<String>,
    gh: Gh,
    /// 봤음 상태 저장소. 모든 worktree가 나눠 쓴다
    viewed: Arc<ViewedStore>,
    state: Mutex<State>,
    meta: Mutex<Meta>,
}

impl Worktree {
    pub fn open(path: &Path, configured_base: Option<String>) -> Result<Arc<Self>, WorktreeError> {
        Self::open_with(path, configured_base, Gh::disabled(), Arc::new(ViewedStore::in_memory()))
    }

    /// `gh`가 켜져 있으면 `claim_pr_lookup` / `refresh_pr_base`로 PR base를 반영할 수 있다
    pub fn open_with(
        path: &Path,
        configured_base: Option<String>,
        gh: Gh,
        viewed: Arc<ViewedStore>,
    ) -> Result<Arc<Self>, WorktreeError> {
        if !path.is_dir() {
            return Err(WorktreeError::Missing(path.to_path_buf()));
        }
        let layout = WatchLayout::detect(&Git::new(path))?;
        let profile = Profile::detect(&layout.root);
        let layout = layout.excluding(&profile.excluded_dirs());
        let git = Git::new(&layout.root);
        let base = resolve_base(&git, configured_base.as_deref(), None)?;
        let mb = merge_base(&git, &base.name)?;
        let (files, list_ms) = compute_list(&git, profile, &mb, &[])?;
        let id = layout.root.to_string_lossy().into_owned();
        let branch_key = current_branch(&git).unwrap_or_default();
        let rows = load_rows(&viewed, &id, &branch_key);
        // 앱이 꺼진 사이 바뀐 체크 파일이 여기서 다시 바뀜으로 잡힌다
        let hashes = viewed_hashes(&git, &mb, &rows, files.values())?;
        Ok(Arc::new(Self {
            id,
            git,
            layout,
            profile,
            configured_base,
            gh,
            viewed,
            state: Mutex::new(State { base, merge_base: mb, files, hashes, branch_key, viewed: rows }),
            meta: Mutex::new(Meta { list_ms, ..Default::default() }),
        }))
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn layout(&self) -> &WatchLayout {
        &self.layout
    }

    pub fn files(&self) -> Vec<ChangedFile> {
        self.state.lock().unwrap().files.values().cloned().collect()
    }

    /// `files()`와 같은 순서로 봤음 상태를 붙여 돌려준다
    pub fn entries(&self) -> Vec<FileEntry> {
        let st = self.state.lock().unwrap();
        st.files
            .values()
            .map(|f| FileEntry { file: f.clone(), viewed: st.viewed_state(&f.path) })
            .collect()
    }

    /// 봤음 체크(`viewed = true`)나 해제. 저장할 hash: `hash`(화면이 본 patch의 hash) → 없으면 `hashes`에 있는 hash
    /// → 그것도 없으면 지금 diff의 hash. 화면이 본 hash가 그사이 바뀐 diff의 것이면 바로 다시 바뀜으로 보인다.
    /// `hashes`는 화면이 아는 hash이므로 이미 있으면 덮지 않는다 (덮으면 처리 전 FS event의 변경을 놓친다).
    /// 저장에 실패하면 상태는 그대로다
    pub fn set_viewed(&self, path: &str, viewed: bool, hash: Option<String>) -> Result<(), WorktreeError> {
        let mut st = self.state.lock().unwrap();
        if !viewed {
            self.viewed.remove(&self.id, &st.branch_key, path)?;
            st.viewed.remove(path);
            return Ok(());
        }
        let file = st.files.get(path).ok_or_else(|| WorktreeError::UnknownFile(path.to_owned()))?;
        let known = match st.hashes.get(path) {
            Some(h) => h.clone(),
            None => file_patch(&self.git, &st.merge_base, file)?.hash,
        };
        let stored = hash.unwrap_or_else(|| known.clone());
        self.viewed.set(&self.id, &st.branch_key, path, &stored)?;
        st.hashes.entry(path.to_owned()).or_insert(known);
        st.viewed.insert(path.to_owned(), stored);
        Ok(())
    }

    pub fn summary(&self) -> WorktreeSummary {
        let branch = self.branch();
        let fsmonitor = self
            .git
            .run_str(&["config", "--get", "core.fsmonitor"])
            .is_ok_and(|v| !matches!(v.as_str(), "" | "false" | "no" | "off" | "0"));
        static SLOW_MS: OnceLock<u64> = OnceLock::new();
        let slow_ms = *SLOW_MS.get_or_init(|| {
            std::env::var("WTBOARD_SLOW_LIST_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(SLOW_LIST_MS)
        });
        let (list_ms, last_changed_at) = {
            let meta = self.meta.lock().unwrap();
            (meta.list_ms, meta.last_changed_at)
        };
        let st = self.state.lock().unwrap();
        WorktreeSummary {
            id: self.id.clone(),
            path: self.id.clone(),
            branch,
            base: st.base.name.clone(),
            base_source: st.base.source,
            changed_files: st.files.len(),
            unviewed_files: st.files.keys().filter(|p| st.viewed_state(p) != ViewedState::Viewed).count(),
            profile: self.profile,
            last_changed_at,
            slow_list_ms: (list_ms > slow_ms).then_some(list_ms),
            fsmonitor,
            error: None,
        }
    }

    fn branch(&self) -> Option<String> {
        current_branch(&self.git)
    }

    /// PR base를 물어야 하면 이 브랜치를 "물었음"으로 표시하고 그 브랜치를 돌려준다.
    /// config base가 있거나, gh가 없거나, detached HEAD거나, 이 브랜치를 이미 물었으면 `None`
    pub fn claim_pr_lookup(&self) -> Option<String> {
        if self.configured_base.is_some() || !self.gh.enabled() {
            return None;
        }
        let branch = self.branch()?;
        let mut meta = self.meta.lock().unwrap();
        if meta.pr_checked.as_deref() == Some(branch.as_str()) {
            return None;
        }
        meta.pr_checked = Some(branch.clone());
        Some(branch)
    }

    /// `branch`를 위해 gh로 PR base를 물어 base에 반영한다. network 호출이라 수 초 걸릴 수 있다.
    /// 그사이 다른 브랜치로 바뀌었으면 결과를 버린다 (새 브랜치의 조회가 따로 돈다). base가 바뀌면 Base event
    pub fn refresh_pr_base(&self, branch: &str) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let pr = self.gh.pr_base(self.git.cwd()).and_then(|b| remote_branch(&self.git, &b));
        {
            let mut meta = self.meta.lock().unwrap();
            if meta.pr_checked.as_deref() != Some(branch) || meta.pr_base == pr {
                return Ok(None);
            }
            meta.pr_base = pr;
        }
        self.refresh_base()
    }

    pub fn patch(&self, path: &str) -> Result<FilePatch, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        let file = st.files.get(path).cloned().ok_or_else(|| WorktreeError::UnknownFile(path.to_owned()))?;
        let patch = file_patch(&self.git, &st.merge_base, &file)?;
        st.hashes.insert(path.to_owned(), patch.hash.clone());
        Ok(patch)
    }

    /// debounce된 event 묶음 하나를 처리한다. 화면에 알릴 변화가 없으면 `None`.
    pub fn handle(&self, targets: &[Target]) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut paths: Vec<String> = targets
            .iter()
            .filter_map(|t| match t {
                Target::File(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        paths.sort();
        paths.dedup();
        // 브랜치 전환 등으로 경로가 아주 많으면 argv 한계(E2BIG)에 걸린다. 목록 전체를 다시 계산하고,
        // hash 비교는 화면이 본 파일로만 좁힌다.
        let too_many = paths.len() > MAX_PATHS_PER_BATCH;
        if too_many {
            let st = self.state.lock().unwrap();
            paths.retain(|p| st.hashes.contains_key(p));
        }

        let rules_changed = paths.iter().any(|p| affects_other_files(p));

        let mut events = Vec::new();
        if targets.contains(&Target::Base) {
            events.push(self.refresh_base()?);
        } else if too_many || rules_changed || targets.contains(&Target::List) {
            events.push(self.refresh_list()?);
        }
        // 목록 재계산은 status/+- 만 비교하므로, 같은 묶음의 파일 event는 hash 비교를 따로 한다
        if !paths.is_empty() {
            events.push(self.refresh_paths(&paths)?);
        }
        let ev = merge(events.into_iter().flatten());
        if ev.is_some() {
            self.meta.lock().unwrap().last_changed_at = Some(now_ms());
        }
        Ok(ev)
    }

    /// base·merge-base·브랜치가 그대로면 목록만 다시 계산한다. 하나라도 바뀌면 state를 통째로 바꾸고 Base event.
    /// 브랜치가 바뀌면 그 브랜치의 봤음 row를 읽는다 (merge-base가 같은 브랜치로 옮겨도 체크가 따라오지 않게)
    fn refresh_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let branch_key = current_branch(&self.git).unwrap_or_default();
        let pr = self.meta.lock().unwrap().pr_base.clone();
        let base = resolve_base(&self.git, self.configured_base.as_deref(), pr.as_deref())?;
        let mb = merge_base(&self.git, &base.name)?;
        let mut st = self.state.lock().unwrap();
        let same_branch = st.branch_key == branch_key;
        if same_branch && st.base == base && st.merge_base == mb {
            drop(st);
            return self.refresh_list();
        }
        let loaded = (!same_branch).then(|| load_rows(&self.viewed, &self.id, &branch_key));
        let files = self.list_all(&mb)?;
        let hashes = viewed_hashes(&self.git, &mb, loaded.as_ref().unwrap_or(&st.viewed), files.values())?;
        let viewed = loaded.unwrap_or_else(|| std::mem::take(&mut st.viewed));
        *st = State { base, merge_base: mb, files, hashes, branch_key, viewed };
        Ok(Some(self.event(ChangeScope::Base, Vec::new())))
    }

    fn refresh_list(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        let fresh = self.list_all(&st.merge_base)?;
        let changed: Vec<String> = st
            .files
            .keys()
            .chain(fresh.keys())
            .filter(|p| st.files.get(*p) != fresh.get(*p))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if changed.is_empty() {
            return Ok(None);
        }
        // 바뀐 파일 중 체크한 파일은 hash를 다시 채운다 (불변식). 같은 event로 화면도 다시 가져간다
        let changed_files = changed.iter().filter_map(|p| fresh.get(p));
        let rehashed = viewed_hashes(&self.git, &st.merge_base, &st.viewed, changed_files)?;
        for p in &changed {
            st.hashes.remove(p);
        }
        st.hashes.extend(rehashed);
        st.files = fresh;
        Ok(Some(self.event(ChangeScope::List, changed)))
    }

    fn refresh_paths(&self, paths: &[String]) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        // 폴더 경로 event (rm -rf로 사라진 폴더)
        let has_children = paths.iter().any(|p| {
            let prefix = format!("{p}/");
            st.files.keys().any(|k| k.starts_with(&prefix))
        });
        if has_children {
            drop(st);
            return self.refresh_list();
        }

        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let fresh = compute_list(&self.git, self.profile, &st.merge_base, &refs)?.0;
        // 폴더 경로 event (새로 생긴 폴더): pathspec이 그 아래 파일들을 돌려준다
        if fresh.keys().any(|k| !paths.contains(k)) {
            drop(st);
            return self.refresh_list();
        }

        // 계산을 모두 끝낸 뒤에 state를 바꾼다. 도중에 git이 실패해도 state와 화면이 어긋나지 않는다.
        let mut updates = Vec::new();
        for p in paths {
            let after = fresh.get(p);
            // 봤음 체크한 파일은 바뀐 뒤에도 새 hash를 들고 있는다 (불변식). 체크한 diff로 돌아오는 것을 잡는다
            let checked = st.viewed.contains_key(p);
            let mut differs = st.files.get(p) != after;
            let mut kept = None;
            if let Some(file) = after.filter(|_| checked || (!differs && st.hashes.contains_key(p))) {
                let hash = file_patch(&self.git, &st.merge_base, file)?.hash;
                differs |= st.hashes.get(p).is_some_and(|known| *known != hash);
                kept = checked.then_some(hash);
            }
            if differs {
                updates.push((p.clone(), after.cloned(), kept));
            }
        }
        let mut changed = Vec::new();
        for (p, after, kept) in updates {
            match kept {
                Some(h) => st.hashes.insert(p.clone(), h),
                None => st.hashes.remove(&p),
            };
            match after {
                Some(f) => {
                    st.files.insert(p.clone(), f);
                }
                None => {
                    st.files.remove(&p);
                }
            }
            changed.push(p);
        }
        Ok((!changed.is_empty()).then(|| self.event(ChangeScope::File, changed)))
    }

    /// 전체 목록을 다시 계산하고 걸린 시간을 기록한다 (fsmonitor 안내용)
    fn list_all(&self, merge_base: &str) -> Result<BTreeMap<String, ChangedFile>, WorktreeError> {
        let (files, list_ms) = compute_list(&self.git, self.profile, merge_base, &[])?;
        self.meta.lock().unwrap().list_ms = list_ms;
        Ok(files)
    }

    fn event(&self, scope: ChangeScope, changed_paths: Vec<String>) -> WorktreeChanged {
        WorktreeChanged { worktree_id: self.id.clone(), changed_paths, scope }
    }
}

/// 변경 목록을 계산하고 접기를 표시한다. 걸린 시간(ms)도 돌려준다
fn compute_list(
    git: &Git,
    profile: Profile,
    merge_base: &str,
    paths: &[&str],
) -> Result<(BTreeMap<String, ChangedFile>, u64), WorktreeError> {
    let started = Instant::now();
    let mut files = list_changes(git, merge_base, paths)?;
    let all: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    // check-attr가 실패해도 목록은 보여 준다. 생성 파일이 아닌 것으로 본다
    let generated = linguist_generated(git, &all).unwrap_or_default();
    for f in &mut files {
        f.collapsed = profile.collapses(&f.path) || generated.contains(&f.path);
    }
    Ok((to_map(files), started.elapsed().as_millis() as u64))
}

/// `branch_key`의 봤음 row. 읽지 못해도 diff는 보여 준다. 체크가 없는 것으로 본다
fn load_rows(store: &ViewedStore, id: &str, branch_key: &str) -> HashMap<String, String> {
    store.load(id, branch_key).unwrap_or_else(|e| {
        eprintln!("[wtboard] {id}: 봤음 상태를 읽을 수 없음: {e}");
        HashMap::new()
    })
}

/// `files` 중 봤음 row가 있는 파일의 현재 hash. 결과는 호출한 쪽이 `hashes`에 넣는다
fn viewed_hashes<'a>(
    git: &Git,
    merge_base: &str,
    rows: &HashMap<String, String>,
    files: impl IntoIterator<Item = &'a ChangedFile>,
) -> Result<HashMap<String, String>, WorktreeError> {
    files
        .into_iter()
        .filter(|f| rows.contains_key(&f.path))
        .map(|f| Ok((f.path.clone(), file_patch(git, merge_base, f)?.hash)))
        .collect()
}

/// 현재 브랜치 이름. detached HEAD면 `None`
fn current_branch(git: &Git) -> Option<String> {
    git.run_str(&["rev-parse", "--abbrev-ref", "HEAD"]).ok().filter(|b| b != "HEAD")
}

/// 바뀌면 다른 파일의 판정이 달라지는 파일: 접기(`.gitattributes`), untracked 목록(`.gitignore`)
fn affects_other_files(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, ".gitattributes" | ".gitignore")
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn to_map(files: Vec<ChangedFile>) -> BTreeMap<String, ChangedFile> {
    files.into_iter().map(|f| (f.path.clone(), f)).collect()
}

fn merge(events: impl Iterator<Item = WorktreeChanged>) -> Option<WorktreeChanged> {
    events.reduce(|mut acc, ev| {
        acc.scope = acc.scope.max(ev.scope);
        for p in ev.changed_paths {
            if !acc.changed_paths.contains(&p) {
                acc.changed_paths.push(p);
            }
        }
        acc
    })
}
