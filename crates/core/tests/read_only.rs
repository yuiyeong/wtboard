mod common;

use std::time::{Duration, SystemTime};

use common::TestRepo;
use wtboard_core::domain::FileStatus;
use wtboard_core::git::changes::list_changes;
use wtboard_core::git::patch::file_patch;
use wtboard_core::git::attrs::linguist_generated;
use wtboard_core::git::Git;
use wtboard_core::worktree::Worktree;

/// 앱은 레포에 아무것도 쓰지 않는다 (spec 1절). env가 아니라 .git/index 바이트로 확인한다.
#[test]
fn reading_changes_never_rewrites_the_index() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("a.txt", "a\n");
    repo.commit_all("add a");
    repo.write("b.txt", "old\n");
    repo.commit_all("add b");
    repo.write("b.txt", "new\n"); // 진짜 변경
    // a.txt는 내용 그대로, mtime만 바뀜 → index 기준 stat-dirty
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options().write(true).open(repo.path().join("a.txt")).unwrap().set_modified(later).unwrap();

    let index = repo.path().join(".git/index");
    let before = std::fs::read(&index).unwrap();
    let git = Git::new(repo.path());
    let files = list_changes(&git, &mb, &[]).unwrap();
    for f in &files {
        file_patch(&git, &mb, f).unwrap();
    }

    assert_eq!(std::fs::read(&index).unwrap(), before, ".git/index was rewritten");
    let paths: Vec<_> = files.iter().map(|f| (f.path.as_str(), f.status)).collect();
    // a.txt는 merge-base 이후 추가된 파일이라 Added로 보여야 하고, stat-dirty 때문에 다른 항목이 생기면 안 된다
    assert_eq!(paths, [("a.txt", FileStatus::Added), ("b.txt", FileStatus::Added)]);
}

#[test]
fn stat_dirty_but_unchanged_base_file_is_not_listed_and_index_is_untouched() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("other.txt", "x\n"); // 진짜 변경 하나 (patch까지 계산하게)
    // merge-base에 있는 README.md: 내용 그대로, mtime만 바뀜 → git diff가 index refresh를 시도하는 경우
    let later = SystemTime::now() + Duration::from_secs(5);
    std::fs::File::options().write(true).open(repo.path().join("README.md")).unwrap().set_modified(later).unwrap();

    let index = repo.path().join(".git/index");
    let before = std::fs::read(&index).unwrap();
    let git = Git::new(repo.path());
    let files = list_changes(&git, &mb, &[]).unwrap();
    for f in &files {
        file_patch(&git, &mb, f).unwrap();
    }
    file_patch(
        &git,
        &mb,
        &wtboard_core::domain::ChangedFile {
            path: "README.md".into(),
            status: FileStatus::Modified,
            additions: Some(0),
            deletions: Some(0),
            collapsed: false,
        },
    )
    .unwrap();

    assert_eq!(std::fs::read(&index).unwrap(), before, ".git/index was rewritten");
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["other.txt"]);
}

#[test]
fn linked_worktree_reads_and_check_attr_leave_both_indexes_untouched() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "gen/** linguist-generated\n");
    repo.commit_all("attrs");
    let wt_path = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    std::fs::create_dir_all(wt_path.join("gen")).unwrap();
    std::fs::write(wt_path.join("gen/a.ts"), "x\n").unwrap();
    std::fs::write(wt_path.join("README.md"), "changed\n").unwrap();

    let main_index = repo.path().join(".git/index");
    let wt_index = repo.path().join(".git/worktrees/wt/index");
    let (main_before, wt_before) = (std::fs::read(&main_index).unwrap(), std::fs::read(&wt_index).unwrap());

    let wt = Worktree::open(&wt_path, Some("main".into())).unwrap();
    for f in wt.files() {
        wt.patch(&f.path).unwrap();
    }
    linguist_generated(&Git::new(&wt_path), &["gen/a.ts", "README.md"]).unwrap();

    assert_eq!(std::fs::read(&main_index).unwrap(), main_before, "main index was rewritten");
    assert_eq!(std::fs::read(&wt_index).unwrap(), wt_before, "worktree index was rewritten");
}
