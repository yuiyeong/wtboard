mod common;

use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use common::{clone_on_feat_with_develop, gh_stub, TestRepo};
use wtboard_core::git::base::BaseSource;
use wtboard_core::watch::{spawn_watcher, spawn_watcher_with, WatchHandle};
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

#[test]
fn pr_base_lookup_runs_in_background_after_start() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh).unwrap();
    let (tx, rx) = mpsc::channel();
    let _h = spawn_watcher(wt.clone(), Duration::from_secs(30), Arc::new(move |ev| {
        let _ = tx.send(ev);
    }))
    .unwrap();
    let ev = rx.recv_timeout(Duration::from_secs(3)).expect("no base event");
    assert_eq!(ev.scope, ChangeScope::Base);
    assert_eq!(wt.summary().base_source, BaseSource::Pr);
}

#[test]
fn deleted_worktree_folder_calls_on_gone() {
    let repo = TestRepo::new();
    let wt_path = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    let wt = Worktree::open(&wt_path, Some("main".into())).unwrap();
    let (tx, rx) = mpsc::channel::<()>();
    let _h = spawn_watcher_with(wt, Duration::from_secs(30), Arc::new(|_| {}), Arc::new(move || {
        let _ = tx.send(());
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    std::fs::remove_dir_all(&wt_path).unwrap();
    rx.recv_timeout(Duration::from_secs(3)).expect("on_gone not called");
}
