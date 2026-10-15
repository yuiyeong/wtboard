//! config에 등록한 레포 전체
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::gh::Gh;
use crate::git::worktrees::common_dir;
use crate::git::Git;
use crate::repo::{Repo, RepoOptions, RepoSummary, Sink};
use crate::store::config::Config;
use crate::worktree::Worktree;

/// 열지 못한 config entry, 또는 entry에 대한 경고
#[derive(Debug, Clone, Serialize)]
pub struct EntryError {
    /// config에 적힌 그대로
    pub path: String,
    pub message: String,
}

pub struct Registry {
    repos: Vec<Arc<Repo>>,
    errors: Vec<EntryError>,
}

struct Group {
    common_dir: PathBuf,
    base: Option<String>,
    first_path: String,
}

impl Registry {
    /// config entry를 common dir로 묶고 레포를 병렬로 연다. 실패한 entry는 `errors`에 남기고 나머지는 계속 연다
    pub fn open(config: &Config, gh: Gh, sink: Sink) -> Self {
        let mut errors = Vec::new();
        let mut groups: Vec<Group> = Vec::new();
        for entry in &config.repos {
            let fail = |message: String| EntryError { path: entry.path.clone(), message };
            let path = entry.resolved_path();
            if !path.is_dir() {
                errors.push(fail(format!("경로 없음: {}", path.display())));
                continue;
            }
            let dir = match common_dir(&Git::new(&path)) {
                Ok(d) => d,
                Err(e) => {
                    errors.push(fail(e.to_string()));
                    continue;
                }
            };
            match groups.iter().position(|g| g.common_dir == dir) {
                None => groups.push(Group { common_dir: dir, base: entry.base.clone(), first_path: entry.path.clone() }),
                Some(i) if groups[i].base != entry.base => errors.push(fail(format!(
                    "{}와 같은 레포인데 base가 다릅니다. 앞 entry의 base({})를 씁니다",
                    groups[i].first_path,
                    groups[i].base.as_deref().unwrap_or("자동")
                ))),
                Some(_) => {}
            }
        }

        let poll = config.watch.poll_interval();
        let opened: Vec<Result<Arc<Repo>, EntryError>> = std::thread::scope(|s| {
            let handles: Vec<_> = groups
                .iter()
                .map(|g| {
                    let opts = RepoOptions { base: g.base.clone(), gh: gh.clone(), poll };
                    let sink = sink.clone();
                    s.spawn(move || {
                        Repo::open(&g.common_dir, opts, sink)
                            .map_err(|e| EntryError { path: g.first_path.clone(), message: e.to_string() })
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("레포 열기 thread panic")).collect()
        });
        let mut repos = Vec::new();
        for r in opened {
            match r {
                Ok(repo) => repos.push(repo),
                Err(e) => errors.push(e),
            }
        }
        Self { repos, errors }
    }

    pub fn repos(&self) -> Vec<RepoSummary> {
        self.repos.iter().map(|r| r.summary()).collect()
    }

    pub fn errors(&self) -> &[EntryError] {
        &self.errors
    }

    pub fn worktree(&self, id: &str) -> Option<Arc<Worktree>> {
        self.repos.iter().find_map(|r| r.worktree(id))
    }
}
