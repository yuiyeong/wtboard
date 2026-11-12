mod common;

use common::TestRepo;
use wtboard_core::git::base::origin_head;
use wtboard_core::git::branches::{base_candidates, toplevel};
use wtboard_core::git::Git;

#[test]
fn candidates_list_remote_then_local() {
    let (_upstream, clone) = common::clone_on_feat_with_develop();
    let names = base_candidates(&Git::new(clone.path())).unwrap();

    let first_local = names.iter().position(|n| !n.starts_with("origin/")).unwrap();
    let (remote, local) = names.split_at(first_local);
    assert!(remote.iter().all(|n| n.starts_with("origin/")), "{names:?}");
    assert!(local.iter().all(|n| !n.starts_with("origin/")), "{names:?}");
    assert!(remote.is_sorted() && local.is_sorted(), "{names:?}");
    for want in ["origin/develop", "origin/main", "feat"] {
        assert!(names.iter().any(|n| n == want), "{want} missing from {names:?}");
    }
    assert!(names.iter().all(|n| !n.ends_with("HEAD") && n != "origin"), "{names:?}");
}

#[test]
fn candidates_without_remote() {
    let repo = TestRepo::new();
    repo.git(&["branch", "dev"]);
    let names = base_candidates(&Git::new(repo.path())).unwrap();
    assert_eq!(names, ["dev", "main"]);
}

#[test]
fn toplevel_from_subdirectory() {
    let repo = TestRepo::new();
    let sub = repo.path().join("sub/dir");
    std::fs::create_dir_all(&sub).unwrap();
    let top = toplevel(&Git::new(&sub)).unwrap();
    assert_eq!(top.canonicalize().unwrap(), repo.path().canonicalize().unwrap());
}

#[test]
fn toplevel_outside_repo_is_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(toplevel(&Git::new(dir.path())).is_err());
}

#[test]
fn origin_head_some_and_none() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let branch = upstream.git(&["branch", "--show-current"]);
    assert_eq!(origin_head(&Git::new(clone.path())), Some(format!("origin/{branch}")));

    let alone = TestRepo::new();
    assert_eq!(origin_head(&Git::new(alone.path())), None);
}
