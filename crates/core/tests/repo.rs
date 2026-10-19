mod common;

use std::sync::{mpsc, Arc};
use std::time::Duration;

use common::TestRepo;
use wtboard_core::gh::Gh;
use wtboard_core::git::worktrees::common_dir;
use wtboard_core::git::Git;
use wtboard_core::repo::{Repo, RepoOptions, Sink};
use wtboard_core::store::viewed::ViewedStore;

fn open(repo: &TestRepo, base: Option<&str>) -> (Arc<Repo>, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel();
    let sink = Sink {
        changed: Arc::new(|_| {}),
        updated: Arc::new(move || {
            let _ = tx.send(());
        }),
    };
    let opts = RepoOptions { base: base.map(String::from), gh: Gh::disabled(), poll: Duration::from_secs(30), viewed: Arc::new(ViewedStore::in_memory()) };
    let r = Repo::open(&common_dir(&Git::new(repo.path())).unwrap(), opts, sink).unwrap();
    std::thread::sleep(Duration::from_millis(300)); // FSEvents stream 시작 대기
    (r, rx)
}

fn branches(r: &Repo) -> Vec<Option<String>> {
    r.summary().worktrees.into_iter().map(|w| w.branch).collect()
}

#[test]
fn opens_every_worktree_in_git_order() {
    let repo = TestRepo::new();
    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    std::fs::write(wt.join("new.txt"), "n\n").unwrap();

    let (r, _rx) = open(&repo, Some("main"));
    let s = r.summary();
    assert_eq!(s.name, "repo");
    let got: Vec<_> = s.worktrees.iter().map(|w| (w.branch.clone(), w.changed_files)).collect();
    assert_eq!(got, [(Some("main".to_string()), 0), (Some("feat".to_string()), 1)]);
    assert!(r.worktree(&s.worktrees[1].id).is_some());
}

#[test]
fn worktree_add_and_remove_update_the_list_and_notify() {
    let repo = TestRepo::new();
    let (r, rx) = open(&repo, Some("main"));
    let wt = repo.path().parent().unwrap().join("wt");

    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("add not notified");
    assert_eq!(branches(&r), [Some("main".to_string()), Some("feat".to_string())]);

    repo.git(&["worktree", "remove", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("remove not notified");
    assert_eq!(branches(&r), [Some("main".to_string())]);
}

#[test]
fn rm_rf_of_a_worktree_folder_drops_it() {
    let repo = TestRepo::new();
    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let (r, rx) = open(&repo, Some("main"));
    assert_eq!(r.summary().worktrees.len(), 2);

    std::fs::remove_dir_all(&wt).unwrap();
    rx.recv_timeout(Duration::from_secs(3)).expect("removal not notified");
    assert_eq!(branches(&r), [Some("main".to_string())]);
}

#[test]
fn failed_worktree_is_listed_with_error_and_retried() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    clone.git(&["remote", "set-head", "origin", "-d"]); // origin/HEAD 없음 → base를 정할 수 없음
    let (r, _rx) = open(&clone, None);
    let err = r.summary().worktrees[0].error.clone().expect("should fail");
    assert!(err.contains("base"), "{err}");

    clone.git(&["remote", "set-head", "origin", "main"]);
    assert!(r.rediscover().unwrap(), "다시 열렸으면 목록이 바뀐 것");
    assert_eq!(r.summary().worktrees[0].error, None);
}
