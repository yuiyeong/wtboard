mod common;

use common::TestRepo;
use wtboard_core::git::base::{merge_base, resolve_base};
use wtboard_core::git::Git;

#[test]
fn configured_base_wins() {
    let repo = TestRepo::new();
    assert_eq!(resolve_base(&Git::new(repo.path()), Some("main")).unwrap(), "main");
}

#[test]
fn falls_back_to_origin_head() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    assert_eq!(resolve_base(&Git::new(clone.path()), None).unwrap(), "origin/main");
}

#[test]
fn explains_how_to_fix_when_no_base_can_be_found() {
    let repo = TestRepo::new(); // origin 없음, config base 없음
    let err = resolve_base(&Git::new(repo.path()), None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("base") && msg.contains("config.toml"), "{msg}");
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
