mod common;

use std::path::Path;
use std::sync::Arc;

use common::TestRepo;
use wtboard_core::gh::Gh;
use wtboard_core::registry::Registry;
use wtboard_core::repo::Sink;
use wtboard_core::store::config::Config;

fn sink() -> Sink {
    Sink { changed: Arc::new(|_| {}), updated: Arc::new(|| {}) }
}

fn registry(entries: &[(&Path, Option<&str>)]) -> Registry {
    let mut toml = String::new();
    for (path, base) in entries {
        toml += &format!("[[repo]]\npath = {:?}\n", path.to_str().unwrap());
        if let Some(b) = base {
            toml += &format!("base = {b:?}\n");
        }
    }
    Registry::open(&Config::parse(&toml).unwrap(), Gh::disabled(), sink())
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
