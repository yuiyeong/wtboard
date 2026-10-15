mod repo_watcher;
mod watcher;

pub use repo_watcher::{spawn_repo_watcher, RediscoverFn, RepoWatchHandle};
pub use watcher::{spawn_watcher, spawn_watcher_with, EmitFn, GoneFn, WatchHandle};

use std::path::{Component, Path, PathBuf};

use notify::RecursiveMode;

use crate::git::{Git, GitError};
use crate::paths::{canonical, nfc};
use crate::profile::Excludes;

/// FS event 경로 하나가 요구하는 재계산 범위
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// 작업 폴더 파일. 그 파일만 다시 계산
    File(String),
    /// index 변경. 파일 목록 전체 다시 계산
    List,
    /// HEAD / refs 변경 (브랜치 전환, fetch, 커밋). merge-base부터 다시 계산
    Base,
    Ignore,
}

#[derive(Debug, Clone)]
pub struct WatchLayout {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    excluded: Excludes,
}

impl WatchLayout {
    /// 제외 폴더 없이 만든다. profile은 root를 알아야 정할 수 있으므로 `excluding`으로 붙인다
    pub fn detect(git: &Git) -> Result<Self, GitError> {
        let out = git.run_str(&[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ])?;
        let mut lines = out.lines().map(canonical);
        let (Some(root), Some(git_dir), Some(common_dir)) = (lines.next(), lines.next(), lines.next()) else {
            return Err(GitError::Unexpected(format!("rev-parse: {out}")));
        };
        Ok(Self {
            root,
            git_dir,
            common_dir,
            excluded: Excludes::default(),
        })
    }

    pub fn excluding(mut self, dirs: &[&str]) -> Self {
        self.excluded = Excludes::new(dirs);
        self
    }

    pub fn classify(&self, path: &Path) -> Target {
        let path = nfc(path);
        if let Ok(rel) = path.strip_prefix(&self.git_dir) {
            return classify_git_dir(rel, self.git_dir == self.common_dir);
        }
        if let Ok(rel) = path.strip_prefix(&self.common_dir) {
            return classify_common_dir(rel);
        }
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return Target::Ignore;
        };
        let parts: Vec<&str> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        if parts.is_empty() || parts.contains(&".git") || self.excluded.matches(&parts) {
            return Target::Ignore;
        }
        Target::File(parts.join("/"))
    }

    /// 감시할 위치. root 안에 들어 있는 위치는 root 감시로 충분하므로 뺀다.
    pub fn watch_roots(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut roots = vec![(self.root.clone(), RecursiveMode::Recursive)];
        if !self.git_dir.starts_with(&self.root) {
            roots.push((self.git_dir.clone(), RecursiveMode::Recursive));
        }
        if !self.common_dir.starts_with(&self.root) {
            roots.push((self.common_dir.join("refs"), RecursiveMode::Recursive));
            roots.push((self.common_dir.clone(), RecursiveMode::NonRecursive)); // packed-refs
        }
        roots
    }
}

/// git-dir 안의 경로. 메인 worktree는 git-dir == common-dir이라 refs도 여기서 판단한다.
fn classify_git_dir(rel: &Path, is_common: bool) -> Target {
    if is_lock(rel) {
        return Target::Ignore;
    }
    match first(rel) {
        // HEAD가 바뀌면 (브랜치 전환) merge-base가 달라질 수 있다. 그대로면 refresh_base가 목록 재계산으로 끝난다
        Some("HEAD") => Target::Base,
        Some("index") => Target::List,
        Some("refs" | "packed-refs") if is_common => Target::Base,
        _ => Target::Ignore,
    }
}

fn classify_common_dir(rel: &Path) -> Target {
    if is_lock(rel) {
        return Target::Ignore;
    }
    match first(rel) {
        Some("refs" | "packed-refs") => Target::Base,
        _ => Target::Ignore,
    }
}

fn first(rel: &Path) -> Option<&str> {
    rel.components().next().and_then(|c| c.as_os_str().to_str())
}

/// git 내부 lock 파일 (`index.lock`, `refs/heads/x.lock`). 작업 폴더에는 적용하지 않는다.
fn is_lock(rel: &Path) -> bool {
    rel.extension().is_some_and(|e| e == "lock")
}
