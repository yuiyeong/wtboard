mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use common::TestRepo;
use wtboard_core::gh::Gh;
use wtboard_core::git::base::BaseSource;
use wtboard_core::registry::{check_new_repo, inspect_entry, AddRepoError, EntryError, Registry};
use wtboard_core::repo::Sink;
use wtboard_core::store::config::Config;
use wtboard_core::store::viewed::ViewedStore;

fn sink() -> Sink {
    Sink { changed: Arc::new(|_| {}), updated: Arc::new(|| {}) }
}

fn config(entries: &[(&Path, Option<&str>)]) -> Config {
    let mut toml = String::new();
    for (path, base) in entries {
        toml += &format!("[[repo]]\npath = {:?}\n", path.to_str().unwrap());
        if let Some(b) = base {
            toml += &format!("base = {b:?}\n");
        }
    }
    Config::parse(&toml).unwrap()
}

fn registry(entries: &[(&Path, Option<&str>)]) -> Registry {
    Registry::open(&config(entries), Gh::disabled(), Arc::new(ViewedStore::in_memory()), sink())
}

fn linked(repo: &TestRepo, name: &str, branch: &str) -> std::path::PathBuf {
    let p = repo.path().parent().unwrap().join(name);
    repo.git(&["worktree", "add", "-q", "-b", branch, p.to_str().unwrap()]);
    p
}

#[test]
fn entries_of_the_same_repo_merge_into_one() {
    let repo = TestRepo::new();
    let wt = linked(&repo, "wt", "feat");
    let reg = registry(&[(repo.path(), Some("main")), (&wt, Some("main"))]);
    let repos = reg.repos();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].worktrees.len(), 2);
    assert!(reg.errors().is_empty(), "{:?}", reg.errors());
}

#[test]
fn conflicting_base_keeps_the_first_and_warns() {
    let repo = TestRepo::new();
    let wt = linked(&repo, "wt", "feat");
    let reg = registry(&[(repo.path(), Some("main")), (&wt, Some("feat"))]);
    assert_eq!(reg.repos()[0].worktrees[0].base, "main");
    assert_eq!(reg.errors().len(), 1);
    assert!(reg.errors()[0].message.contains("base"), "{:?}", reg.errors());
}

#[test]
fn missing_path_is_reported_and_others_still_open() {
    let repo = TestRepo::new();
    let reg = registry(&[(Path::new("/nonexistent/wtboard-x"), None), (repo.path(), Some("main"))]);
    assert_eq!(reg.repos().len(), 1);
    assert_eq!(reg.errors()[0].path, "/nonexistent/wtboard-x");
    assert!(reg.errors()[0].message.contains("경로 없음"), "{:?}", reg.errors());
}

/// Review Focus 2: config가 가리키던 worktree가 지워져도 같은 레포의 나머지는 계속 보인다
#[test]
fn removing_the_configured_worktree_keeps_the_rest_of_the_repo() {
    let repo = TestRepo::new();
    let wt = linked(&repo, "wt", "feat");
    let reg = registry(&[(&wt, Some("main"))]);
    assert_eq!(reg.repos()[0].worktrees.len(), 2);

    repo.git(&["worktree", "remove", wt.to_str().unwrap()]);
    std::thread::sleep(std::time::Duration::from_secs(2)); // repo watcher가 반영할 시간
    let worktrees = &reg.repos()[0].worktrees;
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
}

#[test]
fn worktree_lookup_spans_repos() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let reg = registry(&[(a.path(), Some("main")), (b.path(), Some("main"))]);
    for r in reg.repos() {
        assert!(reg.worktree(&r.worktrees[0].id).is_some());
    }
    assert!(reg.worktree("/nope").is_none());
}

#[test]
fn roots_are_the_opened_worktree_roots() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let wt = linked(&a, "wt", "feat");
    let reg = registry(&[(a.path(), Some("main")), (b.path(), Some("main"))]);
    let mut ids: Vec<String> = reg.repos().iter().flat_map(|r| r.worktrees.iter().map(|w| w.id.clone())).collect();
    let mut roots: Vec<String> = reg.roots().iter().map(|p| p.to_string_lossy().into_owned()).collect();
    ids.sort();
    roots.sort();
    assert_eq!(roots, ids);
    assert_eq!(roots.len(), 3);
    assert!(roots.contains(&wt.canonicalize().unwrap().to_string_lossy().into_owned()));
}

#[test]
fn detached_uses_folder_name() {
    let repo = TestRepo::new();
    let detached = repo.path().parent().unwrap().join("wt-detached");
    repo.git(&["worktree", "add", "-q", "--detach", detached.to_str().unwrap()]);
    let reg = registry(&[(repo.path(), Some("main"))]);
    let tray = reg.tray_worktrees();
    assert_eq!(tray.len(), 2);
    assert_eq!(tray[0].branch, "main");
    assert_eq!(tray[1].branch, "wt-detached");
    assert_eq!(tray[1].repo, tray[0].repo);
    assert_eq!(tray[1].id, detached.canonicalize().unwrap().to_string_lossy());
}

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap()
}

/// 레포 id(common dir)와 그 레포의 worktree id들. `repos()` 순서(= config 순서)
fn ids(reg: &Registry) -> Vec<(String, Vec<String>)> {
    reg.repos().into_iter().map(|r| (r.id, r.worktrees.into_iter().map(|w| w.id).collect())).collect()
}

#[test]
fn empty_config_is_not_an_error() {
    let reg = Registry::open(&Config::parse("").unwrap(), Gh::disabled(), Arc::new(ViewedStore::in_memory()), sink());
    assert!(reg.repos().is_empty());
    assert!(reg.errors().is_empty(), "{:?}", reg.errors());
}

#[test]
fn apply_opens_added_repo() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let reg = registry(&[(a.path(), Some("main"))]);
    let mut opened = reg.apply(&config(&[(a.path(), Some("main")), (b.path(), Some("main"))]));
    let now = ids(&reg);
    assert_eq!(now.len(), 2);
    let mut b_wts = now[1].1.clone();
    opened.sort();
    b_wts.sort();
    assert_eq!(opened, b_wts);
    assert!(now[0].1.iter().all(|id| !opened.contains(id)), "{opened:?}");
}

#[test]
fn apply_keeps_unchanged_repo() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let reg = registry(&[(a.path(), Some("main"))]);
    let a_id = ids(&reg)[0].0.clone();
    let before_a = reg.repo(&a_id).unwrap();
    reg.apply(&config(&[(a.path(), Some("main")), (b.path(), Some("main"))]));
    assert!(Arc::ptr_eq(&before_a, &reg.repo(&a_id).unwrap()));
}

#[test]
fn apply_closes_removed_repo() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let reg = registry(&[(a.path(), Some("main")), (b.path(), Some("main"))]);
    let (b_id, b_wts) = ids(&reg)[1].clone();
    let closed = Arc::downgrade(&reg.repo(&b_id).unwrap());
    let opened = reg.apply(&config(&[(a.path(), Some("main"))]));
    assert!(opened.is_empty(), "{opened:?}");
    assert_eq!(reg.repos().len(), 1);
    assert!(reg.repo(&b_id).is_none());
    assert!(closed.upgrade().is_none(), "닫힌 레포가 drop되지 않으면 watcher가 계속 돈다");
    assert!(reg.worktree(&b_wts[0]).is_none());
    assert!(!reg.roots().contains(&canon(b.path())), "{:?}", reg.roots());
}

#[test]
fn apply_reopens_repo_when_base_changes() {
    // base 없이 열 수 있게 origin/HEAD가 있는 clone
    let upstream = TestRepo::new();
    let a = TestRepo::clone_of(&upstream);
    let b = TestRepo::new();
    let reg = registry(&[(a.path(), None), (b.path(), Some("main"))]);
    let [(a_id, a_wts), (b_id, _)] = <[_; 2]>::try_from(ids(&reg)).unwrap();
    let (before_a, before_b) = (reg.repo(&a_id).unwrap(), reg.repo(&b_id).unwrap());
    assert_eq!(before_a.summary().worktrees[0].base_source, BaseSource::Origin);

    let opened = reg.apply(&config(&[(a.path(), Some("main")), (b.path(), Some("main"))]));
    let after_a = reg.repo(&a_id).unwrap();
    assert!(!Arc::ptr_eq(&before_a, &after_a));
    assert!(Arc::ptr_eq(&before_b, &reg.repo(&b_id).unwrap()));
    assert_eq!(opened, a_wts);
    let wt = &after_a.summary().worktrees[0];
    assert_eq!((wt.base.as_str(), wt.base_source), ("main", BaseSource::Config));
}

#[test]
fn apply_refreshes_entry_errors_and_keeps_warnings() {
    let repo = TestRepo::new();
    let missing = Path::new("/nonexistent/wtboard-x");
    let reg = registry(&[(missing, None), (repo.path(), Some("main"))]);
    let x = EntryError { path: "hook".into(), message: "옛 hook".into() };
    reg.warn(x.clone());
    assert_eq!(reg.errors().len(), 2, "{:?}", reg.errors());

    reg.apply(&config(&[(repo.path(), Some("main"))]));
    assert_eq!(reg.errors(), [x]);
}

#[test]
fn check_new_repo_cases() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let wt = linked(&a, "wt", "feat");
    // 열 수 없는 entry가 섞여 있어도 판단은 같다
    let cfg = config(&[(Path::new("/nonexistent/wtboard-x"), None), (a.path(), Some("main"))]);

    let plain = tempfile::tempdir().unwrap();
    assert_eq!(check_new_repo(&cfg, plain.path()), Err(AddRepoError::NotGitRepo));
    assert_eq!(check_new_repo(&cfg, Path::new("/nonexistent/wtboard-y")), Err(AddRepoError::NotGitRepo));

    let sub = b.path().join("sub");
    std::fs::create_dir(&sub).unwrap();
    assert_eq!(canon(&check_new_repo(&cfg, &sub).unwrap()), canon(b.path()));

    assert_eq!(check_new_repo(&cfg, &wt), Err(AddRepoError::AlreadyRegistered));
    assert_eq!(check_new_repo(&cfg, a.path()), Err(AddRepoError::AlreadyRegistered));
}

#[test]
fn inspect_entry_cases() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let wt = linked(&clone, "clone-wt", "feat");
    let entry = &config(&[(clone.path(), None)]).repos[0];
    let info = inspect_entry(entry);
    assert_eq!(info.error, None);
    assert_eq!(info.name, "clone");
    assert_eq!(info.auto_base, Some(format!("origin/{}", upstream.git(&["branch", "--show-current"]))));
    assert_eq!(info.worktrees, [canon(clone.path()), canon(&wt)]);

    let entry = &config(&[(Path::new("/nonexistent/wtboard-x"), None)]).repos[0];
    let info = inspect_entry(entry);
    assert_eq!(info.error.as_deref(), Some("경로 없음"));
    assert_eq!(info.name, "");
    assert_eq!(info.auto_base, None);
    assert!(info.worktrees.is_empty());
}
