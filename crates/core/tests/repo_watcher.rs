mod common;

use std::sync::{mpsc, Arc};
use std::time::Duration;

use common::TestRepo;
use wtboard_core::git::worktrees::common_dir;
use wtboard_core::git::Git;
use wtboard_core::watch::{spawn_repo_watcher, RepoWatchHandle};

fn start(repo: &TestRepo, poll: Duration) -> (RepoWatchHandle, mpsc::Receiver<()>) {
    let common = common_dir(&Git::new(repo.path())).unwrap();
    let (tx, rx) = mpsc::channel();
    let h = spawn_repo_watcher(&common, poll, Arc::new(move || {
        let _ = tx.send(());
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300)); // FSEvents stream 시작 대기
    (h, rx)
}

fn drain(rx: &mpsc::Receiver<()>) {
    std::thread::sleep(Duration::from_millis(600));
    while rx.try_recv().is_ok() {}
}

#[test]
fn worktree_add_and_remove_trigger_rediscover() {
    let repo = TestRepo::new();
    let (_h, rx) = start(&repo, Duration::from_secs(30));
    let wt = repo.path().parent().unwrap().join("wt");

    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("add not seen");
    drain(&rx);

    repo.git(&["worktree", "remove", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("remove not seen");
}

#[test]
fn ordinary_git_activity_is_ignored() {
    let repo = TestRepo::new();
    let (_h, rx) = start(&repo, Duration::from_secs(30));
    repo.write("a.txt", "a\n");
    repo.commit_all("a"); // objects, refs, index, logs
    assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err());
}

#[test]
fn poll_triggers_rediscover() {
    let repo = TestRepo::new();
    let (_h, rx) = start(&repo, Duration::from_millis(500));
    rx.recv_timeout(Duration::from_secs(2)).expect("poll not fired");
}
