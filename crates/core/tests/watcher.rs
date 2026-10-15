mod common;

use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use common::TestRepo;
use wtboard_core::watch::{spawn_watcher, WatchHandle};
use wtboard_core::worktree::{ChangeScope, Worktree, WorktreeChanged};

fn start(path: &Path) -> (WatchHandle, mpsc::Receiver<WorktreeChanged>) {
    let wt = Worktree::open(path, Some("main".into())).unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = spawn_watcher(wt, Duration::from_secs(30), Arc::new(move |ev| {
        let _ = tx.send(ev);
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300)); // FSEvents stream 시작 대기
    (handle, rx)
}

#[test]
fn file_edit_reaches_emit_within_one_second() {
    let repo = TestRepo::new();
    repo.git(&["checkout", "-q", "-b", "feat"]);
    let (_h, rx) = start(repo.path());

    let started = Instant::now();
    repo.write("README.md", "hello\nchanged\n");
    let ev = rx.recv_timeout(Duration::from_secs(3)).expect("no event");
    let elapsed = started.elapsed();

    assert_eq!((ev.scope, ev.changed_paths), (ChangeScope::File, vec!["README.md".to_string()]));
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}

#[test]
fn excluded_and_gitignored_paths_stay_silent() {
    let repo = TestRepo::new();
    repo.write(".gitignore", "dist/\n");
    repo.commit_all("ignore");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    let (_h, rx) = start(repo.path());

    repo.write("node_modules/pkg/index.js", "x\n");
    repo.write("dist/bundle.js", "x\n");
    assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err());
}

#[test]
fn linked_worktree_edit_is_detected() {
    let repo = TestRepo::new();
    let wt_path = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    let (_h, rx) = start(&wt_path);

    std::fs::write(wt_path.join("new.txt"), "n\n").unwrap();
    let ev = rx.recv_timeout(Duration::from_secs(3)).expect("no event");
    assert_eq!(ev.changed_paths, vec!["new.txt".to_string()]);
}
