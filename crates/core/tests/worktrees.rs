mod common;

use common::{git_at, TestRepo};
use wtboard_core::git::worktrees::{common_dir, list_worktrees};
use wtboard_core::git::Git;

fn names(entries: &[wtboard_core::git::worktrees::WorktreeEntry]) -> Vec<(String, Option<String>)> {
    entries
        .iter()
        .map(|e| (e.path.file_name().unwrap().to_string_lossy().into_owned(), e.branch.clone()))
        .collect()
}

/// Review Focus 3: 공백·한글 경로
#[test]
fn lists_main_and_linked_worktrees_skipping_prunable() {
    let repo = TestRepo::new();
    let parent = repo.path().parent().unwrap();
    let feat = parent.join("wt 기능");
    let gone = parent.join("wt-gone");
    let detached = parent.join("wt-detached");
    repo.git(&["worktree", "add", "-q", "-b", "feat", feat.to_str().unwrap()]);
    repo.git(&["worktree", "add", "-q", "-b", "gone", gone.to_str().unwrap()]);
    repo.git(&["worktree", "add", "-q", "--detach", detached.to_str().unwrap()]);
    std::fs::remove_dir_all(&gone).unwrap();

    let common = common_dir(&Git::new(&feat)).unwrap();
    let entries = list_worktrees(&Git::new(&common)).unwrap();
    assert_eq!(
        names(&entries),
        [
            ("repo".to_string(), Some("main".to_string())),
            ("wt 기능".to_string(), Some("feat".to_string())),
            ("wt-detached".to_string(), None),
        ]
    );
    assert!(entries.iter().all(|e| e.path.is_absolute() && e.path.is_dir()));
}

#[test]
fn common_dir_is_shared_by_all_worktrees() {
    let repo = TestRepo::new();
    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let main = common_dir(&Git::new(repo.path())).unwrap();
    assert_eq!(common_dir(&Git::new(&wt)).unwrap(), main);
    assert!(main.ends_with(".git"));
}

#[test]
fn bare_main_is_skipped() {
    let upstream = TestRepo::new();
    let d = tempfile::tempdir().unwrap();
    git_at(d.path(), &["clone", "-q", "--bare", upstream.path().to_str().unwrap(), "bare.git"]);
    let bare = d.path().join("bare.git");
    git_at(&bare, &["worktree", "add", "-q", "../wt", "main"]);
    assert_eq!(names(&list_worktrees(&Git::new(&bare)).unwrap()), [("wt".to_string(), Some("main".to_string()))]);
}
