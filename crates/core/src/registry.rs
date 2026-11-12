//! config에 등록한 레포 전체. 열린 뒤에도 `apply`로 config에 맞춘다
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use serde::Serialize;

use crate::gh::Gh;
use crate::git::base::origin_head;
use crate::git::branches::toplevel;
use crate::git::worktrees::{common_dir, list_worktrees};
use crate::git::Git;
use crate::repo::{Repo, RepoOptions, RepoSummary, Sink, TrayWorktree};
use crate::store::config::{Config, RepoConfig};
use crate::store::viewed::ViewedStore;
use crate::worktree::Worktree;

/// 열지 못한 config entry, 또는 entry에 대한 경고
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EntryError {
    /// config에 적힌 그대로
    pub path: String,
    pub message: String,
}

pub struct Registry {
    gh: Gh,
    viewed: Arc<ViewedStore>,
    sink: Sink,
    /// config 순서. 읽는 쪽은 `Arc`만 복사하고 lock 밖에서 쓴다
    repos: RwLock<Vec<Opened>>,
    /// 마지막 `apply`의 entry 오류
    entry_errors: Mutex<Vec<EntryError>>,
    /// 마지막 `apply`의 config에 적힌 `[[repo]]` 수. 열지 못한 것도 센다
    configured: AtomicUsize,
    /// `warn`으로 붙인 경고. `apply`가 지우지 않는다
    warnings: Mutex<Vec<EntryError>>,
    /// `apply`를 하나씩만 돌린다. 레포를 여는 동안(수 초) `repos`는 잡지 않는다
    apply_lock: Mutex<()>,
}

/// 열린 레포와 그것을 연 기준. 둘 다 같으면 `apply`가 다시 열지 않는다
#[derive(Clone)]
struct Opened {
    common_dir: PathBuf,
    base: Option<String>,
    repo: Arc<Repo>,
}

struct Group {
    common_dir: PathBuf,
    base: Option<String>,
    first_path: String,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AddRepoError {
    #[error("git repository가 아닙니다")]
    NotGitRepo,
    #[error("이미 등록된 repository입니다")]
    AlreadyRegistered,
}

/// 설정창의 한 줄. 열 수 없는 entry는 `error`만 채운다
#[derive(Debug, Clone, Default)]
pub struct EntryInfo {
    /// entry 경로의 폴더 이름
    pub name: String,
    /// `origin/HEAD`가 가리키는 branch (`origin/develop`). base를 적지 않았을 때 쓰는 것
    pub auto_base: Option<String>,
    /// 그 레포의 모든 worktree root (= worktree id)
    pub worktrees: Vec<PathBuf>,
    pub error: Option<String>,
}

impl Registry {
    /// 빈 registry를 만들고 config에 맞춘다
    pub fn open(config: &Config, gh: Gh, viewed: Arc<ViewedStore>, sink: Sink) -> Self {
        let reg = Self {
            gh,
            viewed,
            sink,
            repos: RwLock::new(Vec::new()),
            entry_errors: Mutex::new(Vec::new()),
            configured: AtomicUsize::new(0),
            warnings: Mutex::new(Vec::new()),
            apply_lock: Mutex::new(()),
        };
        reg.apply(config);
        reg
    }

    /// config에 맞춘다. common dir와 base가 같은 레포는 그대로 두고, 새 레포는 병렬로 열고, 없어진 레포는 닫는다.
    /// base가 바뀐 레포는 닫고 다시 연다. 실패한 entry는 `errors`에 남기고 나머지는 계속 연다.
    /// 돌려주는 것: 이번에 (다시) 연 레포의 worktree id
    pub fn apply(&self, config: &Config) -> Vec<String> {
        // 지키는 값이 `()`라 앞 apply가 panic했어도 이어서 써도 된다
        let _serial = self.apply_lock.lock().unwrap_or_else(PoisonError::into_inner);
        let (groups, mut errors) = group(config);
        // apply만 목록을 바꾸므로 이 복사본이 apply 동안 그대로 맞다
        let current = self.repos.read().unwrap().clone();
        let poll = config.watch.poll_interval();
        let mut next = Vec::new();
        let mut reopened = Vec::new();
        std::thread::scope(|s| {
            let pending: Vec<_> = groups
                .iter()
                .map(|g| match current.iter().find(|o| o.common_dir == g.common_dir && o.base == g.base) {
                    Some(kept) => Ok(kept.clone()),
                    None => Err(s.spawn(move || self.open_group(g, poll))),
                })
                .collect();
            for p in pending {
                match p {
                    Ok(kept) => next.push(kept),
                    Err(h) => match h.join().expect("레포 열기 thread panic") {
                        Ok(o) => {
                            reopened.extend(o.repo.roots().iter().map(|r| r.to_string_lossy().into_owned()));
                            next.push(o);
                        }
                        Err(e) => errors.push(e),
                    },
                }
            }
        });
        *self.repos.write().unwrap() = next;
        *self.entry_errors.lock().unwrap() = errors;
        self.configured.store(config.repos.len(), Ordering::Relaxed);
        // 닫힌 레포의 Arc를 목록 lock 밖에서 놓는다. 보통 이것이 마지막이라 여기서 watcher가 멈추지만,
        // watcher callback이나 `snapshot`을 든 reader가 잠깐 더 쥐고 있으면 그쪽이 놓을 때 멈춘다
        drop(current);
        reopened
    }

    fn open_group(&self, g: &Group, poll: std::time::Duration) -> Result<Opened, EntryError> {
        let opts = RepoOptions { base: g.base.clone(), gh: self.gh.clone(), poll, viewed: self.viewed.clone() };
        Repo::open(&g.common_dir, opts, self.sink.clone())
            .map(|repo| Opened { common_dir: g.common_dir.clone(), base: g.base.clone(), repo })
            .map_err(|e| EntryError { path: g.first_path.clone(), message: e.to_string() })
    }

    /// 지금 열린 레포. `apply`를 막지 않게 lock 밖에서 쓴다
    fn snapshot(&self) -> Vec<Arc<Repo>> {
        self.repos.read().unwrap().iter().map(|o| o.repo.clone()).collect()
    }

    /// 화면이 파일 event마다 부른다. worktree마다 git을 부르므로 레포끼리 병렬로 한다
    pub fn repos(&self) -> Vec<RepoSummary> {
        let repos = self.snapshot();
        std::thread::scope(|s| {
            let handles: Vec<_> = repos.iter().map(|r| s.spawn(|| r.summary())).collect();
            handles.into_iter().map(|h| h.join().expect("레포 summary thread panic")).collect()
        })
    }

    /// `id` = common dir 문자열 (`RepoSummary.id`)
    pub fn repo(&self, id: &str) -> Option<Arc<Repo>> {
        self.repos.read().unwrap().iter().find(|o| o.common_dir.to_string_lossy() == id).map(|o| o.repo.clone())
    }

    /// 마지막 `apply`의 entry 오류 다음 `warn`으로 붙인 경고
    pub fn errors(&self) -> Vec<EntryError> {
        let mut errors = self.entry_errors.lock().unwrap().clone();
        errors.extend(self.warnings.lock().unwrap().iter().cloned());
        errors
    }

    /// 마지막 `apply`의 config에 적힌 `[[repo]]` 수. 열린 레포가 0개일 때 "등록이 없다"와 "열지 못했다"를 가른다
    pub fn configured_repos(&self) -> usize {
        self.configured.load(Ordering::Relaxed)
    }

    /// 열 때 말고 나중에 알게 된 문제를 오류 목록 끝에 붙인다. `apply` 뒤에도 남는다
    pub fn warn(&self, e: EntryError) {
        self.warnings.lock().unwrap().push(e);
    }

    pub fn worktree(&self, id: &str) -> Option<Arc<Worktree>> {
        self.snapshot().iter().find_map(|r| r.worktree(id))
    }

    /// 메뉴바에 보일 열린 worktree. sidebar 순서, git을 부르지 않는다
    pub fn tray_worktrees(&self) -> Vec<TrayWorktree> {
        self.snapshot().iter().flat_map(|r| r.tray_worktrees()).collect()
    }

    /// 열린 모든 worktree의 root. 세션 경로를 가장 깊은 worktree에 귀속시키는 데 쓴다
    pub fn roots(&self) -> Vec<PathBuf> {
        self.snapshot().iter().flat_map(|r| r.roots()).collect()
    }
}

/// config entry를 common dir로 묶는다. 같은 레포인데 base가 다르면 앞 entry의 base를 쓰고 경고한다
fn group(config: &Config) -> (Vec<Group>, Vec<EntryError>) {
    let mut errors = Vec::new();
    let mut groups: Vec<Group> = Vec::new();
    for entry in &config.repos {
        let fail = |message: String| EntryError { path: entry.path.clone(), message };
        let dir = match entry_common_dir(entry) {
            Ok(d) => d,
            Err(message) => {
                errors.push(fail(message));
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
    (groups, errors)
}

/// entry가 가리키는 레포의 common dir. 실패하면 화면에 보일 이유
fn entry_common_dir(entry: &RepoConfig) -> Result<PathBuf, String> {
    let path = entry.resolved_path();
    if !path.is_dir() {
        return Err("경로 없음".into()); // path는 EntryError.path로 이미 보인다
    }
    common_dir(&Git::new(&path)).map_err(|e| e.to_string())
}

/// 고른 폴더를 등록해도 되는지 보고, 등록할 경로(그 worktree의 root)를 돌려준다.
/// 같은 common dir의 entry가 이미 있으면 거절한다. 열 수 없는 entry는 판단에 쓰지 않는다
pub fn check_new_repo(config: &Config, picked: &Path) -> Result<PathBuf, AddRepoError> {
    let git = Git::new(picked);
    let root = toplevel(&git).map_err(|_| AddRepoError::NotGitRepo)?;
    let dir = common_dir(&git).map_err(|_| AddRepoError::NotGitRepo)?;
    if config.repos.iter().any(|e| entry_common_dir(e).is_ok_and(|d| d == dir)) {
        return Err(AddRepoError::AlreadyRegistered);
    }
    Ok(root)
}

/// 설정창의 한 줄을 채운다. registry가 열지 않은 entry도 볼 수 있게 git을 직접 부른다
pub fn inspect_entry(entry: &RepoConfig) -> EntryInfo {
    let inspect = || -> Result<EntryInfo, String> {
        let dir = entry_common_dir(entry)?;
        let worktrees = list_worktrees(&Git::new(&dir)).map_err(|e| e.to_string())?;
        let path = entry.resolved_path();
        Ok(EntryInfo {
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            auto_base: origin_head(&Git::new(&path)),
            worktrees: worktrees.into_iter().map(|w| w.path).collect(),
            error: None,
        })
    };
    inspect().unwrap_or_else(|e| EntryInfo { error: Some(e), ..Default::default() })
}
