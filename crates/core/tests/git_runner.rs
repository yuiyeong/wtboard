use std::ffi::OsStr;
use std::process::Command;

use wtboard_core::git::{Git, GitError};

fn env_of(cmd: &Command, key: &str) -> Option<String> {
    cmd.get_envs()
        .find(|(k, _)| *k == OsStr::new(key))
        .and_then(|(_, v)| v)
        .map(|v| v.to_string_lossy().into_owned())
}

#[test]
fn every_command_avoids_locks_and_fixes_output_format() {
    let cmd = Git::new("/tmp").command(&["status"]);
    assert_eq!(env_of(&cmd, "GIT_OPTIONAL_LOCKS").as_deref(), Some("0"));
    assert_eq!(env_of(&cmd, "GIT_LITERAL_PATHSPECS").as_deref(), Some("1"));
    assert_eq!(env_of(&cmd, "GIT_TERMINAL_PROMPT").as_deref(), Some("0"));
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(
        args,
        [
            "-c", "core.quotePath=false",
            "-c", "diff.noprefix=false",
            "-c", "diff.mnemonicPrefix=false",
            "status",
        ]
    );
}

#[test]
fn run_allow_accepts_listed_exit_codes_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let git = Git::new(dir.path());
    // diff --no-index는 차이가 있으면 exit 1
    let args = ["diff", "--no-index", "--", "/dev/null", "a.txt"];
    let out = git.run_allow(&args, &[0, 1]).unwrap();
    assert!(String::from_utf8_lossy(&out).contains("+hello"));
    let err = git.run(&args).unwrap_err();
    assert!(matches!(err, GitError::Failed { code: Some(1), .. }), "{err:?}");
}
