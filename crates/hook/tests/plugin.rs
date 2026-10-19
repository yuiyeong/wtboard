mod common;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use common::{run_with_stdin, session, TestRepo};

fn plugin_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../plugin/wtboard")
}

fn run_sh() -> PathBuf {
    plugin_dir().join("hooks/run.sh")
}

/// hooks.json의 (event, matcher, subcommand) 조합. matcher가 없으면 빈 문자열
fn hook_entries() -> BTreeSet<(String, String, String)> {
    let raw = std::fs::read_to_string(plugin_dir().join("hooks/hooks.json")).unwrap();
    let json: Value = serde_json::from_str(&raw).unwrap();
    let mut out = BTreeSet::new();
    for (event, groups) in json["hooks"].as_object().expect("최상위 \"hooks\" key").iter() {
        for group in groups.as_array().unwrap() {
            let matcher = group["matcher"].as_str().unwrap_or("").to_string();
            for h in group["hooks"].as_array().unwrap() {
                assert_eq!(h["type"], "command");
                let command = h["command"].as_str().unwrap();
                let sub = command
                    .strip_prefix("\"${CLAUDE_PLUGIN_ROOT}\"/hooks/run.sh ")
                    .unwrap_or_else(|| panic!("command 형식이 다르다: {command}"));
                out.insert((event.clone(), matcher.clone(), sub.to_string()));
            }
        }
    }
    out
}

#[test]
fn hooks_json_covers_every_subcommand() {
    const EDIT: &str = "Edit|Write|NotebookEdit";
    let t = |e: &str, m: &str, s: &str| (e.to_string(), m.to_string(), s.to_string());
    let mut want = BTreeSet::new();
    want.insert(t("SessionStart", "", "session-start"));
    want.insert(t("UserPromptSubmit", "", "prompt-submit"));
    want.insert(t("PreToolUse", EDIT, "pre-edit"));
    want.insert(t("PreToolUse", "Bash", "pre-bash"));
    for post in ["PostToolUse", "PostToolUseFailure"] {
        want.insert(t(post, EDIT, "post-edit"));
        want.insert(t(post, "Bash", "post-bash"));
    }
    want.insert(t("Stop", "", "turn-end"));
    want.insert(t("StopFailure", "", "turn-end"));
    want.insert(t("SessionEnd", "", "session-end"));
    assert_eq!(hook_entries(), want);
}

#[test]
fn manifests_match_marketplace_entry() {
    let plugin: Value = serde_json::from_str(
        &std::fs::read_to_string(plugin_dir().join(".claude-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    let market: Value = serde_json::from_str(
        &std::fs::read_to_string(plugin_dir().join("../.claude-plugin/marketplace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(plugin["name"], "wtboard");
    assert_eq!(market["name"], "wtboard-local");
    assert_eq!(market["plugins"][0]["name"], plugin["name"]);
    assert_eq!(market["plugins"][0]["source"], "./wtboard");
    // plugin 목록에 보이는 메타데이터
    assert_eq!(plugin["author"]["name"], "wtboard");
    assert!(market["description"].is_string());
}

#[test]
fn run_sh_exits_zero_without_binary() {
    let out = Command::new(run_sh())
        .arg("session-start")
        .env("WTBOARD_HOOK_BIN", "/nonexistent/wtboard-hook")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

#[test]
fn run_sh_forwards_to_binary() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let mut cmd = Command::new(run_sh());
    cmd.arg("session-start")
        .env("WTBOARD_HOOK_BIN", env!("CARGO_BIN_EXE_wtboard-hook"))
        .env("WTBOARD_CONFIG", data.path().join("config.toml"))
        .env_remove("CLAUDE_PROJECT_DIR");
    let input = json!({
        "session_id": "sess-1",
        "cwd": repo.path(),
        "hook_event_name": "SessionStart",
        "source": "startup",
    });
    let out = run_with_stdin(cmd, input.to_string().as_bytes());
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());
    let meta = session(data.path(), "sess-1").read_meta().unwrap();
    assert_eq!(meta.project_dir, repo.path());
}
