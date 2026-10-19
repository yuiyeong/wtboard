#![allow(dead_code)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use wtboard_core::session::SessionDir;

#[path = "../../../core/tests/common/mod.rs"]
mod core_common;
pub use core_common::*;

/// `WTBOARD_CONFIG=<data_dir>/config.toml`로 hook을 부르는 명령. 테스트를 Claude Code 안에서
/// 돌려도 `CLAUDE_PROJECT_DIR`이 섞이지 않게 지운다
pub fn hook_command(data_dir: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_wtboard-hook"));
    cmd.args(args)
        .env("WTBOARD_CONFIG", data_dir.join("config.toml"))
        .env_remove("CLAUDE_PROJECT_DIR");
    cmd
}

/// stdin에 `input`을 넣고 끝날 때까지 기다린다
pub fn run_with_stdin(mut cmd: Command, input: &[u8]) -> Output {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    // hook이 stdin을 읽기 전에 끝나도 테스트가 멈추지 않게 쓰기 실패는 무시한다
    let _ = stdin.write_all(input);
    drop(stdin);
    child.wait_with_output().unwrap()
}

pub fn run_hook(data_dir: &Path, sub: &str, input: &serde_json::Value) -> Output {
    run_with_stdin(hook_command(data_dir, &[sub]), input.to_string().as_bytes())
}

/// 어떤 경우에도 hook이 지켜야 하는 출력 규칙: exit 0, stdout 비어 있음, stderr는 많아야 한 줄
pub fn assert_quiet_exit(out: &Output) {
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty(), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.trim_end().lines().count() <= 1, "stderr: {stderr}");
}

pub fn session(data_dir: &Path, id: &str) -> SessionDir {
    SessionDir::open(data_dir, id).unwrap()
}
