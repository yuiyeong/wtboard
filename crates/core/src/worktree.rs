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
}

/// 재계산 범위. 여러 개를 합칠 때는 큰 쪽을 따른다 (`File < List < Base`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeScope {
    File,
    List,
    Base,
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
    /// 화면이 patch를 가져간 파일의 마지막 hash
    hashes: HashMap<String, String>,
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
    state: Mutex<State>,
    meta: Mutex<Meta>,
}

impl Worktree {
    pub fn open(path: &Path, configured_base: Option<String>) -> Result<Arc<Self>, WorktreeError> {
        Self::open_with(path, configured_base, Gh::disabled())
    }

    /// `gh`가 켜져 있으면 `claim_pr_lookup` / `refresh_pr_base`로 PR base를 반영할 수 있다
    pub fn open_with(path: &Path, configured_base: Option<String>, gh: Gh) -> Result<Arc<Self>, WorktreeError> {
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
        Ok(Arc::new(Self {
            id: layout.root.to_string_lossy().into_owned(),
            git,
            layout,
            profile,
            configured_base,
            gh,
            state: Mutex::new(State { base, merge_base: mb, files, hashes: HashMap::new() }),
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
            profile: self.profile,
            last_changed_at,
            slow_list_ms: (list_ms > slow_ms).then_some(list_ms),
            fsmonitor,
            error: None,
        }
    }

    fn branch(&self) -> Option<String> {
        self.git
            .run_str(&["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .filter(|b| b != "HEAD") // detached HEAD
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

    fn refresh_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let pr = self.meta.lock().unwrap().pr_base.clone();
        let base = resolve_base(&self.git, self.configured_base.as_deref(), pr.as_deref())?;
        let mb = merge_base(&self.git, &base.name)?;
        let unchanged = {
            let st = self.state.lock().unwrap();
            st.base == base && st.merge_base == mb
        };
        if unchanged {
            return self.refresh_list();
        }
        let files = self.list_all(&mb)?;
        *self.state.lock().unwrap() = State { base, merge_base: mb, files, hashes: HashMap::new() };
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
        for p in &changed {
            st.hashes.remove(p);
        }
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
            let mut differs = st.files.get(p) != after;
            if !differs {
                if let (Some(file), Some(known)) = (after, st.hashes.get(p)) {
                    differs = file_patch(&self.git, &st.merge_base, file)?.hash != *known;
                }
            }
            if differs {
                updates.push((p.clone(), after.cloned()));
            }
        }
        let mut changed = Vec::new();
        for (p, after) in updates {
            st.hashes.remove(&p);
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

/// 바뀌면 다른 파일의 판정이 달라지는 파일: 접기(`.gitattributes`), untracked 목록(`.gitignore`)
fn affects_other_files(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    matches!(name, ".gitattributes" | ".gitignore")
}

fn now_ms() -> u64 {
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
