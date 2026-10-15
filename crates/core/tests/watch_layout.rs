mod common;

use std::path::Path;

use common::TestRepo;
use unicode_normalization::UnicodeNormalization;
use wtboard_core::git::Git;
use wtboard_core::watch::{Target, WatchLayout};

fn layout(path: &Path) -> WatchLayout {
    WatchLayout::detect(&Git::new(path), &["node_modules"]).unwrap()
}

fn file(p: &str) -> Target {
    Target::File(p.into())
}

#[test]
fn main_worktree_classification() {
    let repo = TestRepo::new();
    let l = layout(repo.path());
    let r = &l.root;
    assert_eq!(l.classify(&r.join("src/app.py")), file("src/app.py"));
    assert_eq!(l.classify(&r.join("Cargo.lock")), file("Cargo.lock")); // 작업 폴더의 .lock은 일반 파일
    assert_eq!(l.classify(&r.join("web/node_modules/x/index.js")), Target::Ignore);
    assert_eq!(l.classify(&r.join(".git/index")), Target::List);
    assert_eq!(l.classify(&r.join(".git/HEAD")), Target::List);
    assert_eq!(l.classify(&r.join(".git/index.lock")), Target::Ignore);
    assert_eq!(l.classify(&r.join(".git/refs/heads/main")), Target::Base);
    assert_eq!(l.classify(&r.join(".git/refs/remotes/origin/main")), Target::Base);
    assert_eq!(l.classify(&r.join(".git/packed-refs")), Target::Base);
    assert_eq!(l.classify(&r.join(".git/objects/ab/cdef")), Target::Ignore);
    assert_eq!(l.classify(&r.join(".git/logs/HEAD")), Target::Ignore);
    assert_eq!(l.classify(Path::new("/somewhere/else.txt")), Target::Ignore);
    assert_eq!(l.watch_roots().len(), 1); // .git이 root 안에 있으므로 root 하나로 충분
}

#[test]
fn linked_worktree_uses_its_own_git_dir_and_shared_refs() {
    let repo = TestRepo::new();
    let wt_path = repo.path().parent().unwrap().join("wt-feat");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    let l = layout(&wt_path);

    assert_ne!(l.git_dir, l.common_dir);
    assert_eq!(l.classify(&l.git_dir.join("HEAD")), Target::List);
    assert_eq!(l.classify(&l.git_dir.join("index")), Target::List);
    assert_eq!(l.classify(&l.common_dir.join("refs/heads/feat")), Target::Base);
    assert_eq!(l.classify(&l.common_dir.join("refs/heads/feat.lock")), Target::Ignore);
    assert_eq!(l.classify(&l.common_dir.join("objects/ab/cd")), Target::Ignore);
    assert_eq!(l.classify(&l.root.join(".git")), Target::Ignore); // linked worktree의 .git 파일
    assert_eq!(l.classify(&l.root.join("a.txt")), file("a.txt"));

    let roots: Vec<_> = l.watch_roots().into_iter().map(|(p, _)| p).collect();
    assert!(roots.contains(&l.root));
    assert!(roots.contains(&l.git_dir));
    assert!(roots.contains(&l.common_dir.join("refs")));
}

#[test]
fn paths_are_canonical_and_nfc() {
    let repo = TestRepo::new();
    let l = layout(repo.path());
    // macOS tempdir은 /var/... → /private/var/... symlink
    assert_eq!(l.root, repo.path().canonicalize().unwrap());
    let nfd: String = "한글.txt".nfd().collect();
    assert_ne!(nfd, "한글.txt");
    assert_eq!(l.classify(&l.root.join(&nfd)), file("한글.txt"));
}
