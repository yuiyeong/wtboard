mod common;

use common::TestRepo;
use wtboard_core::domain::{ChangedFile, FileStatus};
use wtboard_core::git::changes::list_changes;
use wtboard_core::git::Git;

/// main에 keep.txt, gone.txt, .gitignore를 커밋하고 feat 브랜치로 옮긴다. merge-base sha를 돌려준다.
fn setup() -> (TestRepo, String) {
    let repo = TestRepo::new();
    repo.write("keep.txt", "1\n2\n3\n");
    repo.write("gone.txt", "bye\n");
    repo.write(".gitignore", "ignored/\n");
    repo.commit_all("base");
    let mb = repo.head();
    repo.git(&["checkout", "-q", "-b", "feat"]);
    (repo, mb)
}

fn find<'a>(files: &'a [ChangedFile], path: &str) -> &'a ChangedFile {
    files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} not in {files:?}"))
}

#[test]
fn combines_committed_uncommitted_and_untracked() {
    let (repo, mb) = setup();
    repo.write("committed.txt", "c\n");
    repo.commit_all("feat commit");
    repo.write("keep.txt", "1\nTWO\n3\n4\n"); // 커밋 안 된 수정
    repo.remove("gone.txt");
    repo.write("new.txt", "a\nb\n"); // untracked
    repo.write("ignored/x.txt", "x\n"); // gitignore 대상

    let files = list_changes(&Git::new(repo.path()), &mb, &[]).unwrap();

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["committed.txt", "gone.txt", "keep.txt", "new.txt"]);
    assert_eq!(find(&files, "committed.txt").status, FileStatus::Added);
    assert_eq!(find(&files, "gone.txt").status, FileStatus::Deleted);
    let keep = find(&files, "keep.txt");
    assert_eq!((keep.status, keep.additions, keep.deletions), (FileStatus::Modified, Some(2), Some(1)));
    let new = find(&files, "new.txt");
    assert_eq!((new.status, new.additions, new.deletions), (FileStatus::Untracked, Some(2), Some(0)));
}

#[test]
fn binary_files_have_no_line_counts() {
    let (repo, mb) = setup();
    repo.write("img.bin", [0u8, 1, 2, 0, 255]);
    repo.commit_all("bin");
    repo.write("untracked.bin", [0u8, 9, 0]);
    let files = list_changes(&Git::new(repo.path()), &mb, &[]).unwrap();
    assert_eq!(find(&files, "img.bin").additions, None);
    assert_eq!(find(&files, "untracked.bin").additions, None);
}

#[test]
fn unusual_file_names_are_listed_and_matched_literally() {
    let (repo, mb) = setup();
    let names = ["with space.txt", "한글.txt", "star*.txt", "[brackets].txt"];
    for name in names {
        repo.write(name, "x\n");
    }
    repo.commit_all("names");
    repo.write("star.txt", "glob이었다면 star*.txt에 걸린다\n");
    let git = Git::new(repo.path());

    let all = list_changes(&git, &mb, &[]).unwrap();
    for name in names {
        find(&all, name);
    }
    let only = list_changes(&git, &mb, &["star*.txt"]).unwrap();
    assert_eq!(only.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["star*.txt"]);
}

#[test]
fn pathspec_returns_nothing_for_clean_or_ignored_paths() {
    let (repo, mb) = setup();
    repo.write("keep.txt", "changed\n");
    repo.write("ignored/x.txt", "x\n");
    let git = Git::new(repo.path());
    assert_eq!(list_changes(&git, &mb, &["keep.txt"]).unwrap().len(), 1);
    assert!(list_changes(&git, &mb, &["README.md"]).unwrap().is_empty());
    assert!(list_changes(&git, &mb, &["ignored/x.txt"]).unwrap().is_empty());
}
