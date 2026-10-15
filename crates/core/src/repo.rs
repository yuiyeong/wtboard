//! 레포 하나(common dir 하나)와 그 worktree들
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;

use crate::gh::Gh;
use crate::git::worktrees::{list_worktrees, WorktreeEntry};
use crate::git::{Git, GitError};
use crate::profile::Profile;
use crate::watch::{spawn_repo_watcher, spawn_watcher_with, EmitFn, GoneFn, RepoWatchHandle, WatchHandle};
use crate::worktree::{Worktree, WorktreeSummary};

/// worktree 목록이 바뀌었을 때 (추가, 삭제, 실패했던 worktree가 열림)
pub type UpdatedFn = Arc<dyn Fn() + Send + Sync>;

/// 화면으로 보내는 알림 두 종류
#[derive(Clone)]
pub struct Sink {
    pub changed: EmitFn,
    pub updated: UpdatedFn,
}

#[derive(Debug, Clone)]
pub struct RepoOptions {
    /// config의 `base`. 모든 worktree에 같이 적용한다
    pub base: Option<String>,
    pub gh: Gh,
    pub poll: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("worktree 목록 감시를 시작할 수 없음: {0}")]
    Watch(#[from] notify::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub id: String,
    pub name: String,
    /// 첫 번째로 열린 worktree(보통 main)의 profile
    pub profile: Profile,
    pub worktrees: Vec<WorktreeSummary>,
}

struct Opened {
    path: PathBuf,
    branch: Option<String>,
    /// 연 worktree와 그 watcher (drop하면 감시가 멈춘다), 또는 열지 못한 이유
    result: Result<(Arc<Worktree>, WatchHandle), String>,
}

pub struct Repo {
    id: String,
    name: String,
    /// common dir에서 실행한다. config에 적힌 worktree가 지워져도 목록을 읽을 수 있다
    git: Git,
    opts: RepoOptions,
    sink: Sink,
    worktrees: Mutex<Vec<Opened>>,
    watch: OnceLock<RepoWatchHandle>,
}

impl Repo {
    pub fn open(common_dir: &Path, opts: RepoOptions, sink: Sink) -> Result<Arc<Self>, RepoError> {
        let repo = Arc::new(Self {
            id: common_dir.to_string_lossy().into_owned(),
            name: repo_name(common_dir),
            git: Git::new(common_dir),
            opts,
            sink,
            worktrees: Mutex::new(Vec::new()),
            watch: OnceLock::new(),
        });
        repo.rediscover()?;
        let weak = Arc::downgrade(&repo);
        let handle = spawn_repo_watcher(common_dir, repo.opts.poll, Arc::new(move || {
            if let Some(r) = weak.upgrade() {
                r.rediscover_and_notify();
            }
        }))?;
        let _ = repo.watch.set(handle);
        Ok(repo)
    }

    /// worktree 목록을 다시 읽는다. 새 worktree와 전에 열지 못한 worktree는 (다시) 열고, 사라진 worktree는 닫는다.
    /// 화면에 보일 목록(경로와 성공 여부)이 바뀌었으면 true
    pub fn rediscover(self: &Arc<Self>) -> Result<bool, GitError> {
        let entries = list_worktrees(&self.git)?;
        let mut opened = self.worktrees.lock().unwrap();
        let before = signature(&opened);
        opened.retain(|o| o.result.is_ok() && entries.iter().any(|e| e.path == o.path));
        let missing: Vec<&WorktreeEntry> =
            entries.iter().filter(|e| !opened.iter().any(|o| o.path == e.path)).collect();
        let fresh: Vec<Opened> = std::thread::scope(|s| {
            let handles: Vec<_> = missing.iter().map(|e| s.spawn(move || self.open_worktree(e))).collect();
            handles.into_iter().map(|h| h.join().expect("worktree 열기 thread panic")).collect()
        });
        opened.extend(fresh);
        opened.sort_by_key(|o| entries.iter().position(|e| e.path == o.path));
        Ok(signature(&opened) != before)
    }

    pub fn summary(&self) -> RepoSummary {
        // summary는 git을 부르므로 lock 밖에서 한다 (rediscover를 막지 않게)
        let snapshot: Vec<Result<Arc<Worktree>, WorktreeSummary>> = self
            .worktrees
            .lock()
            .unwrap()
            .iter()
            .map(|o| match &o.result {
                Ok((wt, _)) => Ok(wt.clone()),
                Err(e) => Err(WorktreeSummary::failed(&o.path, o.branch.clone(), e.clone())),
            })
            .collect();
        let worktrees: Vec<WorktreeSummary> =
            snapshot.into_iter().map(|r| r.map_or_else(|failed| failed, |wt| wt.summary())).collect();
        let profile = worktrees.iter().find(|w| w.error.is_none()).map_or(Profile::Default, |w| w.profile);
        RepoSummary { id: self.id.clone(), name: self.name.clone(), profile, worktrees }
    }

    pub fn worktree(&self, id: &str) -> Option<Arc<Worktree>> {
        self.worktrees
            .lock()
            .unwrap()
            .iter()
            .filter_map(|o| o.result.as_ref().ok())
            .find(|(wt, _)| wt.id() == id)
            .map(|(wt, _)| wt.clone())
    }

    fn open_worktree(self: &Arc<Self>, entry: &WorktreeEntry) -> Opened {
        let opened = Worktree::open_with(&entry.path, self.opts.base.clone(), self.opts.gh.clone());
        let result = opened.map_err(|e| e.to_string()).and_then(|wt| {
            spawn_watcher_with(wt.clone(), self.opts.poll, self.sink.changed.clone(), self.on_gone())
                .map(|h| (wt, h))
                .map_err(|e| format!("파일 감시를 시작할 수 없음: {e}"))
        });
        Opened { path: entry.path.clone(), branch: entry.branch.clone(), result }
    }

    /// worktree 폴더가 사라지면 목록을 다시 읽는다. watcher worker 안에서 불리므로 새 thread에서 한다
    fn on_gone(self: &Arc<Self>) -> GoneFn {
        let weak = Arc::downgrade(self);
        Arc::new(move || {
            if let Some(repo) = weak.upgrade() {
                std::thread::spawn(move || repo.rediscover_and_notify());
            }
        })
    }

    fn rediscover_and_notify(self: &Arc<Self>) {
        match self.rediscover() {
            Ok(true) => (self.sink.updated)(),
            Ok(false) => {}
            Err(e) => eprintln!("[wtboard] {}: worktree 목록을 읽을 수 없음: {e}", self.name),
        }
    }
}

fn signature(opened: &[Opened]) -> Vec<(PathBuf, bool)> {
    opened.iter().map(|o| (o.path.clone(), o.result.is_ok())).collect()
}

/// `~/dev/my-api/.git` → `my-api`, bare `~/dev/my-api.git` → `my-api`
fn repo_name(common_dir: &Path) -> String {
    let dir = if common_dir.file_name().is_some_and(|n| n == ".git") {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    };
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.strip_suffix(".git").map(str::to_owned).unwrap_or(name)
}
