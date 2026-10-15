//! PATH를 바꾸므로 다른 test와 섞이지 않게 binary를 따로 둔다
mod common;

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::TestRepo;
use wtboard_core::gh::Gh;
use wtboard_core::git::worktrees::common_dir;
use wtboard_core::git::Git;
use wtboard_core::repo::{Repo, RepoOptions, Sink};

/// final review #1: 새 worktree를 여는 동안(수 초) 레포 summary가 막히면 모든 command가 멈춘다
#[test]
fn summary_does_not_wait_for_a_worktree_being_opened() {
    let repo = TestRepo::new();
    let sink = Sink { changed: Arc::new(|_| {}), updated: Arc::new(|| {}) };
    let opts = RepoOptions { base: Some("main".into()), gh: Gh::disabled(), poll: Duration::from_secs(30) };
    let r = Repo::open(&common_dir(&Git::new(repo.path())).unwrap(), opts, sink).unwrap();

    // 이제부터 untracked 목록 계산(worktree 열기의 일부)만 2초 걸리게 한다
    let real_git = String::from_utf8(std::process::Command::new("which").arg("git").output().unwrap().stdout).unwrap();
    let bin = tempfile::tempdir().unwrap();
    let wrapper = bin.path().join("git");
    std::fs::write(
        &wrapper,
        format!("#!/bin/sh\ncase \"$*\" in *\"ls-files --others\"*) sleep 2;; esac\nexec {} \"$@\"\n", real_git.trim()),
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", bin.path().display(), std::env::var("PATH").unwrap());
    std::env::set_var("PATH", path);

    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let opener = {
        let r = r.clone();
        std::thread::spawn(move || r.rediscover().unwrap())
    };
    std::thread::sleep(Duration::from_millis(300)); // 여는 중

    let started = Instant::now();
    let s = r.summary();
    assert!(started.elapsed() < Duration::from_millis(1000), "summary waited {:?}", started.elapsed());
    assert_eq!(s.worktrees.len(), 1, "아직 열리는 중인 worktree는 보이지 않는다");
    assert!(opener.join().unwrap());
    assert_eq!(r.summary().worktrees.len(), 2);
}
