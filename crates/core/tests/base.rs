mod common;

use common::TestRepo;
use wtboard_core::git::base::{merge_base, remote_branch, resolve_base, BaseSource};
use wtboard_core::git::Git;

#[test]
fn configured_base_wins() {
    let repo = TestRepo::new();
    let b = resolve_base(&Git::new(repo.path()), Some("main"), Some("origin/develop")).unwrap();
    assert_eq!((b.name.as_str(), b.source), ("main", BaseSource::Config));
}

#[test]
fn pr_base_beats_origin_head() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let b = resolve_base(&Git::new(clone.path()), None, Some("origin/develop")).unwrap();
    assert_eq!((b.name.as_str(), b.source), ("origin/develop", BaseSource::Pr));
}

#[test]
fn falls_back_to_origin_head() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let b = resolve_base(&Git::new(clone.path()), None, None).unwrap();
    assert_eq!((b.name.as_str(), b.source), ("origin/main", BaseSource::Origin));
}

#[test]
fn explains_how_to_fix_when_no_base_can_be_found() {
    let repo = TestRepo::new(); // origin 없음, config base 없음
    let err = resolve_base(&Git::new(repo.path()), None, None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("base") && msg.contains("config.toml"), "{msg}");
}

#[test]
fn remote_branch_requires_a_local_remote_ref() {
    let upstream = TestRepo::new();
    upstream.git(&["branch", "develop"]);
    let clone = TestRepo::clone_of(&upstream);
    let git = Git::new(clone.path());
    assert_eq!(remote_branch(&git, "develop").as_deref(), Some("origin/develop"));
    assert_eq!(remote_branch(&git, "release"), None);
}

#[test]
fn merge_base_is_the_fork_point() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let fork = clone.head();
    clone.git(&["checkout", "-q", "-b", "feat"]);
    clone.write("a.txt", "a\n");
    clone.commit_all("feat");
    assert_eq!(merge_base(&Git::new(clone.path()), "origin/main").unwrap(), fork);
}
