mod common;

use common::TestRepo;
use wtboard_core::domain::{FilePatch, PatchBody, MAX_DIFF_BYTES};
use wtboard_core::git::changes::list_changes;
use wtboard_core::git::patch::{file_patch, no_index_numstat, no_index_patch};
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

/// 결과가 5초 안에 오지 않으면 실패 (무한 읽기 회귀 방지)
fn within_5s<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(std::time::Duration::from_secs(5)).expect("hung reading a special file")
}

#[test]
fn untracked_symlink_to_special_file_does_not_hang() {
    let repo = TestRepo::new();
    let mb = repo.head();
    // 링크 대상이 FIFO면 따라가서 읽는 순간 영원히 멈춘다 (/dev/zero면 메모리를 끝없이 쓴다)
    let fifo = repo.path().parent().unwrap().join("pipe");
    assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
    std::os::unix::fs::symlink(&fifo, repo.path().join("link")).unwrap();
    let root = repo.path().to_path_buf();
    let patch = within_5s(move || {
        let git = Git::new(&root);
        let file = list_changes(&git, &mb, &["link"]).unwrap().into_iter().next().expect("symlink listed");
        file_patch(&git, &mb, &file).unwrap()
    });
    let target = fifo.to_string_lossy().into_owned();
    assert!(text(&patch).contains(&format!("+{target}")), "symlink은 git처럼 링크 대상을 보여 준다: {patch:?}");
}

#[test]
fn no_index_patch_relabels_header_to_worktree_path() {
    use std::os::unix::fs::PermissionsExt;
    let repo = TestRepo::new();
    let other = tempfile::tempdir().unwrap();
    let before = other.path().join("baseline");
    std::fs::write(&before, "a\nb\n").unwrap();
    std::fs::set_permissions(&before, std::fs::Permissions::from_mode(0o600)).unwrap();
    repo.write("dir/x y.sh", "a\nc\n");
    let after = repo.path().join("dir/x y.sh");
    std::fs::set_permissions(&after, std::fs::Permissions::from_mode(0o755)).unwrap();
    let git = Git::new(repo.path());

    let p = no_index_patch(&git, "dir/x y.sh", Some(&before), Some(&after)).unwrap();
    let d = text(&p);
    assert!(d.starts_with("diff --git a/dir/x y.sh b/dir/x y.sh\n"), "{d}");
    assert!(d.contains("\n--- a/dir/x y.sh\t\n+++ b/dir/x y.sh\t\n@@"), "{d}");
    // 세션 폴더 내용 파일의 권한(0600)은 mode 변화로 보이지 않는다
    assert!(!d.contains("old mode") && !d.contains("new mode"), "{d}");
    assert!(!d.contains(other.path().file_name().unwrap().to_str().unwrap()), "{d}");
    assert_eq!(p.hash, blake3::hash(d.as_bytes()).to_hex().to_string());
    assert_eq!(no_index_numstat(&git, Some(&before), Some(&after)).unwrap(), (Some(1), Some(1)));
}

#[test]
fn no_index_patch_matches_git_for_quoted_new_file() {
    let repo = TestRepo::new();
    let rel = "q\"uote d.txt";
    repo.write(rel, "x\n");
    let git = Git::new(repo.path());
    // 상대 경로로 부른 git 출력과 같아야 한다 (quote, 공백 뒤 tab)
    let expected = git.run_allow(&["diff", "--no-color", "--no-index", "--", "/dev/null", rel], &[1]).unwrap();
    let p = no_index_patch(&git, rel, None, Some(&repo.path().join(rel))).unwrap();
    assert_eq!(text(&p), String::from_utf8(expected).unwrap());
    assert_eq!(no_index_numstat(&git, None, Some(&repo.path().join(rel))).unwrap(), (Some(1), Some(0)));
}

#[test]
fn no_index_patch_binary_and_too_large() {
    let repo = TestRepo::new();
    repo.write("b.bin", [0u8, 1, 2]);
    let bin = repo.path().join("b.bin");
    let git = Git::new(repo.path());
    let p = no_index_patch(&git, "b.bin", None, Some(&bin)).unwrap();
    assert_eq!(p.body, PatchBody::Binary { old_size: None, new_size: Some(3) });
    assert_eq!(no_index_numstat(&git, None, Some(&bin)).unwrap(), (None, None));

    repo.write("big.txt", "x".repeat(MAX_DIFF_BYTES as usize + 1));
    let big = repo.path().join("big.txt");
    let p = no_index_patch(&git, "big.txt", Some(&bin), Some(&big)).unwrap();
    assert_eq!(p.body, PatchBody::TooLarge { old_size: Some(3), new_size: Some(MAX_DIFF_BYTES + 1) });
    repo.write("big.txt", "y".repeat(MAX_DIFF_BYTES as usize + 1));
    let again = no_index_patch(&git, "big.txt", Some(&bin), Some(&big)).unwrap();
    assert_ne!(p.hash, again.hash);
}
