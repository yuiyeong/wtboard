mod common;

use common::TestRepo;
use wtboard_core::domain::{FilePatch, PatchBody, MAX_DIFF_BYTES};
use wtboard_core::git::changes::list_changes;
use wtboard_core::git::patch::file_patch;
use wtboard_core::git::Git;

fn patch_for(repo: &TestRepo, mb: &str, path: &str) -> FilePatch {
    let git = Git::new(repo.path());
    let file = list_changes(&git, mb, &[path]).unwrap().into_iter().next().expect("file should be changed");
    file_patch(&git, mb, &file).unwrap()
}

fn text(p: &FilePatch) -> String {
    match &p.body {
        PatchBody::Text { diff } => diff.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn tracked_diff_includes_committed_and_uncommitted() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("a.py", "one\n");
    repo.commit_all("add");
    repo.write("a.py", "one\ntwo\n");
    let d = text(&patch_for(&repo, &mb, "a.py"));
    assert!(d.contains("+one") && d.contains("+two"), "{d}");
}

#[test]
fn untracked_diff_is_against_dev_null() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("new file.py", "hi\n");
    let d = text(&patch_for(&repo, &mb, "new file.py"));
    assert!(d.contains("--- /dev/null") && d.contains("+hi"), "{d}");
}

#[test]
fn deleted_file_diff() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.remove("README.md");
    assert!(text(&patch_for(&repo, &mb, "README.md")).contains("-hello"));
}

#[test]
fn binary_and_large_files_skip_text_diff() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("img.bin", [0u8, 1, 0, 2]);
    repo.write("big.txt", "x\n".repeat(MAX_DIFF_BYTES as usize / 2 + 1));
    assert!(matches!(
        patch_for(&repo, &mb, "img.bin").body,
        PatchBody::Binary { old_size: None, new_size: Some(4) }
    ));
    assert!(matches!(patch_for(&repo, &mb, "big.txt").body, PatchBody::TooLarge { .. }));
}

#[test]
fn hash_changes_only_when_diff_changes() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("README.md", "hello\nworld\n");
    let h1 = patch_for(&repo, &mb, "README.md").hash;
    assert_eq!(patch_for(&repo, &mb, "README.md").hash, h1);
    repo.write("README.md", "hello\nWORLD\n"); // +1/-0, status M 그대로. 내용만 다름
    assert_ne!(patch_for(&repo, &mb, "README.md").hash, h1);
}

#[test]
fn user_git_config_does_not_change_output() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.git(&["config", "color.ui", "always"]);
    repo.git(&["config", "color.diff", "always"]);
    repo.git(&["config", "diff.noprefix", "true"]);
    repo.git(&["config", "diff.mnemonicPrefix", "true"]);
    repo.write("README.md", "hello\nworld\n");
    let d = text(&patch_for(&repo, &mb, "README.md"));
    assert!(!d.contains('\u{1b}'), "color codes leaked: {d:?}");
    assert!(d.contains("--- a/README.md") && d.contains("+++ b/README.md"), "{d}");
}

#[test]
fn serializes_for_the_ui() {
    let body = PatchBody::TooLarge { old_size: Some(1), new_size: None };
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        serde_json::json!({"kind": "tooLarge", "oldSize": 1, "newSize": null})
    );
}
