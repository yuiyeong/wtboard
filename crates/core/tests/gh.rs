use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wtboard_core::gh::{find_gh_in, Gh};

/// `dir/gh` 실행 파일을 만든다
fn stub(dir: &Path, body: &str) -> PathBuf {
    let p = dir.join("gh");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[test]
fn returns_pr_base_branch() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), r#"[ "$1 $2 $3 $4" = "pr view --json baseRefName" ] && echo develop"#)));
    assert_eq!(gh.pr_base(d.path()).as_deref(), Some("develop"));
}

#[test]
fn runs_without_prompts_or_index_locks() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), r#"[ "$GH_PROMPT_DISABLED" = 1 ] && [ "$GIT_OPTIONAL_LOCKS" = 0 ] && echo main"#)));
    assert_eq!(gh.pr_base(d.path()).as_deref(), Some("main"));
}

#[test]
fn no_pr_is_none() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), "echo 'no pull requests found' >&2; exit 1")));
    assert_eq!(gh.pr_base(d.path()), None);
}

#[test]
fn missing_or_disabled_gh_is_none() {
    assert_eq!(Gh::with_bin(Some(PathBuf::from("/nonexistent/gh"))).pr_base(Path::new("/")), None);
    assert!(!Gh::disabled().enabled());
    assert_eq!(Gh::disabled().pr_base(Path::new("/")), None);
}

#[test]
fn slow_gh_is_killed_after_timeout() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), "sleep 10; echo develop"))).timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(gh.pr_base(d.path()), None);
    assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
}

/// Review Focus 1: Finder에서 띄운 앱은 PATH에 Homebrew 경로가 없다
#[test]
fn finds_gh_in_fallback_dirs_when_path_lacks_it() {
    let brew = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let gh = stub(brew.path(), "true");
    let path_env = Some(OsString::from(empty.path()));
    assert_eq!(find_gh_in(path_env.clone(), &[brew.path()]), Some(gh.clone()));
    assert_eq!(find_gh_in(path_env, &[]), None);
    // PATH에 있으면 PATH가 먼저
    let in_path = tempfile::tempdir().unwrap();
    let first = stub(in_path.path(), "true");
    assert_eq!(find_gh_in(Some(OsString::from(in_path.path())), &[brew.path()]), Some(first));
}
