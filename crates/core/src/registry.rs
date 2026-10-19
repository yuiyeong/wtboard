//! config에 등록한 레포 전체
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::gh::Gh;
use crate::git::worktrees::common_dir;
use crate::git::Git;
use crate::repo::{Repo, RepoOptions, RepoSummary, Sink, TrayWorktree};
use crate::store::config::Config;
use crate::store::viewed::ViewedStore;
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
    pub fn open(config: &Config, gh: Gh, viewed: Arc<ViewedStore>, sink: Sink) -> Self {
        let mut errors = Vec::new();
        let mut groups: Vec<Group> = Vec::new();
        for entry in &config.repos {
            let fail = |message: String| EntryError { path: entry.path.clone(), message };
            let path = entry.resolved_path();
            if !path.is_dir() {
                errors.push(fail("경로 없음".into())); // path는 EntryError.path로 이미 보인다
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
        let mut repos = Vec::new();
        std::thread::scope(|s| {
            let handles: Vec<_> = groups
                .iter()
                .map(|g| {
                    let opts = RepoOptions { base: g.base.clone(), gh: gh.clone(), poll, viewed: viewed.clone() };
                    let sink = sink.clone();
                    s.spawn(move || {
                        Repo::open(&g.common_dir, opts, sink)
                            .map_err(|e| EntryError { path: g.first_path.clone(), message: e.to_string() })
                    })
                })
                .collect();
            for h in handles {
                match h.join().expect("레포 열기 thread panic") {
                    Ok(repo) => repos.push(repo),
                    Err(e) => errors.push(e),
                }
            }
        });
        Self { repos, errors }
    }

    /// 화면이 파일 event마다 부른다. worktree마다 git을 부르므로 레포끼리 병렬로 한다
    pub fn repos(&self) -> Vec<RepoSummary> {
        std::thread::scope(|s| {
            let handles: Vec<_> = self.repos.iter().map(|r| s.spawn(|| r.summary())).collect();
            handles.into_iter().map(|h| h.join().expect("레포 summary thread panic")).collect()
        })
    }

    pub fn errors(&self) -> &[EntryError] {
        &self.errors
    }

    /// 열 때 말고 나중에 알게 된 문제를 오류 목록 끝에 붙인다
    pub fn warn(&mut self, e: EntryError) {
        self.errors.push(e);
    }

    pub fn worktree(&self, id: &str) -> Option<Arc<Worktree>> {
        self.repos.iter().find_map(|r| r.worktree(id))
    }

    /// 메뉴바에 보일 열린 worktree. sidebar 순서, git을 부르지 않는다
    pub fn tray_worktrees(&self) -> Vec<TrayWorktree> {
        self.repos.iter().flat_map(|r| r.tray_worktrees()).collect()
    }

    /// 열린 모든 worktree의 root. 세션 경로를 가장 깊은 worktree에 귀속시키는 데 쓴다
    pub fn roots(&self) -> Vec<PathBuf> {
        self.repos.iter().flat_map(|r| r.roots()).collect()
    }
}
