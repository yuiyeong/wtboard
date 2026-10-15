use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::domain::{ChangedFile, FilePatch};
use crate::git::base::{merge_base, resolve_base, BaseError};
use crate::git::changes::list_changes;
use crate::git::patch::file_patch;
use crate::git::{Git, GitError};
use crate::profile::COMMON_EXCLUDED_DIRS;
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
    pub changed_files: usize,
}

/// 한 번의 git 호출에 pathspec으로 넘길 최대 경로 수
const MAX_PATHS_PER_BATCH: usize = 200;

struct State {
    base: String,
    merge_base: String,
    files: BTreeMap<String, ChangedFile>,
    /// 화면이 patch를 가져간 파일의 마지막 hash
    hashes: HashMap<String, String>,
}

pub struct Worktree {
    id: String,
    git: Git,
    layout: WatchLayout,
    configured_base: Option<String>,
    state: Mutex<State>,
}

impl Worktree {
    pub fn open(path: &Path, configured_base: Option<String>) -> Result<Arc<Self>, WorktreeError> {
        if !path.is_dir() {
            return Err(WorktreeError::Missing(path.to_path_buf()));
        }
        let layout = WatchLayout::detect(&Git::new(path))?.excluding(COMMON_EXCLUDED_DIRS);
        let git = Git::new(&layout.root);
        let base = resolve_base(&git, configured_base.as_deref())?;
        let mb = merge_base(&git, &base)?;
        let files = to_map(list_changes(&git, &mb, &[])?);
        Ok(Arc::new(Self {
            id: layout.root.to_string_lossy().into_owned(),
            git,
            layout,
            configured_base,
            state: Mutex::new(State { base, merge_base: mb, files, hashes: HashMap::new() }),
        }))
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
        let branch = self
            .git
            .run_str(&["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .filter(|b| b != "HEAD"); // detached HEAD
        let st = self.state.lock().unwrap();
        WorktreeSummary {
            id: self.id.clone(),
            path: self.id.clone(),
            branch,
            base: st.base.clone(),
            changed_files: st.files.len(),
        }
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

        let mut events = Vec::new();
        if targets.contains(&Target::Base) {
            events.push(self.refresh_base()?);
        } else if too_many || targets.contains(&Target::List) {
            events.push(self.refresh_list()?);
        }
        // 목록 재계산은 status/+- 만 비교하므로, 같은 묶음의 파일 event는 hash 비교를 따로 한다
        if !paths.is_empty() {
            events.push(self.refresh_paths(&paths)?);
        }
        Ok(merge(events.into_iter().flatten()))
    }

    fn refresh_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let base = resolve_base(&self.git, self.configured_base.as_deref())?;
        let mb = merge_base(&self.git, &base)?;
        let unchanged = {
            let st = self.state.lock().unwrap();
            st.base == base && st.merge_base == mb
        };
        if unchanged {
            return self.refresh_list();
        }
        let files = to_map(list_changes(&self.git, &mb, &[])?);
        *self.state.lock().unwrap() = State { base, merge_base: mb, files, hashes: HashMap::new() };
        Ok(Some(self.event(ChangeScope::Base, Vec::new())))
    }

    fn refresh_list(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        let fresh = to_map(list_changes(&self.git, &st.merge_base, &[])?);
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
        let fresh = to_map(list_changes(&self.git, &st.merge_base, &refs)?);
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

    fn event(&self, scope: ChangeScope, changed_paths: Vec<String>) -> WorktreeChanged {
        WorktreeChanged { worktree_id: self.id.clone(), changed_paths, scope }
    }
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
