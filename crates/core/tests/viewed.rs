mod common;

use std::path::Path;
use std::sync::Arc;

use common::{feat_repo, TestRepo};
use wtboard_core::gh::Gh;
use wtboard_core::store::viewed::ViewedStore;
use wtboard_core::watch::Target;
use wtboard_core::worktree::{ChangeScope, ViewedState, Worktree, WorktreeError};

fn open(repo: &TestRepo) -> Arc<Worktree> {
    Worktree::open(repo.path(), Some("main".into())).unwrap()
}

fn open_with(repo: &TestRepo, store: &Arc<ViewedStore>) -> Arc<Worktree> {
    Worktree::open_with(repo.path(), Some("main".into()), Gh::disabled(), store.clone()).unwrap()
}

/// 앱을 껐다 켜도 남는 store (tempdir의 SQLite 파일)
fn file_store(dir: &Path) -> Arc<ViewedStore> {
    Arc::new(ViewedStore::open(&dir.join("state.sqlite")).unwrap())
}

fn state(wt: &Worktree, p: &str) -> ViewedState {
    wt.entries().into_iter().find(|e| e.file.path == p).expect("file in list").viewed
}

#[test]
fn unchecked_file_is_none() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Unviewed);
    assert_eq!(wt.summary().unviewed_files, 1);
}

#[test]
fn check_with_screen_hash_is_viewed() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    let h = wt.patch("src/a.py").unwrap().hash;
    wt.set_viewed("src/a.py", true, Some(h)).unwrap();
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Viewed);
    assert_eq!(wt.summary().unviewed_files, 0);
}

#[test]
fn check_without_hash_uses_current_diff() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.set_viewed("src/a.py", true, None).unwrap();
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Viewed);
}

#[test]
fn check_with_stale_hash_is_changed() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.set_viewed("src/a.py", true, Some("stale".into())).unwrap();
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Changed);
    assert_eq!(wt.summary().unviewed_files, 1);
}

/// 화면이 본 뒤 바뀌었는데 FS event보다 체크가 먼저 온 경우: 체크가 화면이 아는 hash를 덮지 않으므로 event가 나간다
#[test]
fn check_before_pending_event_still_emits() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    let h1 = wt.patch("src/a.py").unwrap().hash;
    repo.write("src/a.py", "a = 3\n"); // +1/-1, status M 그대로
    wt.set_viewed("src/a.py", true, Some(h1)).unwrap();
    let ev = wt.handle(&[Target::File("src/a.py".into())]).unwrap().expect("hash change should emit");
    assert_eq!((ev.scope, ev.changed_paths), (ChangeScope::File, vec!["src/a.py".to_string()]));
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Changed);
}

#[test]
fn uncheck_returns_to_none() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.set_viewed("src/a.py", true, None).unwrap();
    wt.set_viewed("src/a.py", false, None).unwrap();
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Unviewed);
}

#[test]
fn set_viewed_unknown_file_errors() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    let err = wt.set_viewed("nope.py", true, None).unwrap_err();
    assert!(matches!(err, WorktreeError::UnknownFile(p) if p == "nope.py"));
}

#[test]
fn edit_after_check_is_changed_and_emits() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.set_viewed("src/a.py", true, None).unwrap(); // patch를 가져가지 않았어도 hash 비교 대상이 된다
    repo.write("src/a.py", "a = 3\n"); // +1/-1, status M 그대로
    let ev = wt.handle(&[Target::File("src/a.py".into())]).unwrap().expect("hash change should emit");
    assert_eq!((ev.scope, ev.changed_paths), (ChangeScope::File, vec!["src/a.py".to_string()]));
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Changed);

    wt.set_viewed("src/a.py", true, None).unwrap();
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Viewed);
}

/// 다시 바뀐 뒤에도 hash를 들고 있으므로, 체크한 diff로 되돌리면 다시 봤음이 된다
#[test]
fn edit_back_to_checked_diff_is_viewed_again() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.set_viewed("src/a.py", true, None).unwrap();
    repo.write("src/a.py", "a = 3\n");
    wt.handle(&[Target::File("src/a.py".into())]).unwrap();
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Changed);

    repo.write("src/a.py", "a = 2\n"); // +1/-1 그대로
    let ev = wt.handle(&[Target::File("src/a.py".into())]).unwrap().expect("hash change should emit");
    assert_eq!(ev.changed_paths, vec!["src/a.py".to_string()]);
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Viewed);
}

#[test]
fn checks_survive_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let store = file_store(dir.path());
    open_with(&repo, &store).set_viewed("src/a.py", true, None).unwrap();
    drop(store);

    let wt = open_with(&repo, &file_store(dir.path()));
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Viewed);
}

#[test]
fn change_while_closed_is_changed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let store = file_store(dir.path());
    open_with(&repo, &store).set_viewed("src/a.py", true, None).unwrap();
    drop(store);

    repo.write("src/a.py", "a = 3\n");
    let wt = open_with(&repo, &file_store(dir.path()));
    assert_eq!(state(&wt, "src/a.py"), ViewedState::Changed);
}
