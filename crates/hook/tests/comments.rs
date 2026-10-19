mod common;

use std::fs;
use std::path::Path;
use std::process::Output;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use wtboard_core::store::db;
use wtboard_core::worktree::Worktree;

use common::{assert_quiet_exit, run_hook, seed_comment, sent_states, session, TestRepo};

const SID: &str = "sess-1";

fn submit(data: &Path, cwd: &Path, prompt: Option<&str>) -> Output {
    let mut input = json!({
        "session_id": SID,
        "transcript_path": "/tmp/transcript.jsonl",
        "cwd": cwd,
        "hook_event_name": "UserPromptSubmit",
    });
    if let Some(p) = prompt {
        input["prompt"] = json!(p);
    }
    run_hook(data, "prompt-submit", &input)
}

fn worktree_id(repo: &Path) -> String {
    Worktree::open(repo, Some("main".into())).unwrap().id().to_string()
}

fn stdout_json(out: &Output) -> Value {
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout.clone()).unwrap();
    assert_eq!(text.lines().count(), 1, "stdout: {text}");
    serde_json::from_str(&text).unwrap()
}

fn all_unsent(data: &Path, wt: &str) -> bool {
    sent_states(data, wt).iter().all(|(_, to)| to.is_none())
}

#[test]
fn attaches_unsent_and_marks_sent() {
    let repo = TestRepo::new();
    let data = tempfile::tempdir().unwrap();
    let wt = worktree_id(repo.path());
    seed_comment(data.path(), &wt, "a.rs", 3, "첫 번째 코멘트");
    seed_comment(data.path(), &wt, "b.rs", 5, "두 번째 코멘트");

    let out = submit(data.path(), repo.path(), Some("고쳐 줘"));
    let json = stdout_json(&out);

    let context = json["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert_eq!(json["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
    assert!(context.contains("a.rs:3") && context.contains("첫 번째 코멘트"), "{context}");
    assert!(context.contains("b.rs:5") && context.contains("두 번째 코멘트"), "{context}");
    let message = json["systemMessage"].as_str().unwrap();
    assert!(message.contains("2개를"), "{message}");
    assert!(String::from_utf8_lossy(&out.stderr).is_empty());
    for (_, sent_to) in sent_states(data.path(), &wt) {
        assert_eq!(sent_to.as_deref(), Some(SID));
    }
    // turn 기록도 된다
    assert_eq!(session(data.path(), SID).read_meta().unwrap().turn, 1);

    // 다시 보내면 붙을 것이 없다
    assert_quiet_exit(&submit(data.path(), repo.path(), Some("again")));
}

#[test]
fn other_worktree_not_attached() {
    let repo = TestRepo::new();
    let other = repo.path().parent().unwrap().join("wt-b");
    repo.git(&["worktree", "add", "-q", "-b", "b", other.to_str().unwrap()]);
    let data = tempfile::tempdir().unwrap();
    seed_comment(data.path(), &worktree_id(&other), "a.rs", 1, "B 전용");

    assert_quiet_exit(&submit(data.path(), repo.path(), Some("go")));
    assert!(all_unsent(data.path(), &worktree_id(&other)));
    // B에서는 붙는다
    let json = stdout_json(&submit(data.path(), &other, Some("go")));
    assert!(json["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("B 전용"));
}

#[test]
fn cwd_subdir_and_symlink_match_app_worktree_id() {
    let repo = TestRepo::new();
    repo.write("src/lib.rs", "x\n");
    let data = tempfile::tempdir().unwrap();
    let wt = worktree_id(repo.path());
    // tempdir는 /var/... 그대로이고 앱 id는 canonical (/private/var/...)이다
    if cfg!(target_os = "macos") {
        assert_ne!(repo.path().to_string_lossy(), wt, "tempdir가 이미 canonical이면 이 test는 의미가 없다");
    }
    seed_comment(data.path(), &wt, "src/lib.rs", 1, "하위 폴더");

    let json = stdout_json(&submit(data.path(), &repo.path().join("src"), Some("go")));
    assert!(json["hookSpecificOutput"]["additionalContext"].as_str().unwrap().contains("하위 폴더"));
    assert!(sent_states(data.path(), &wt).iter().all(|(_, to)| to.is_some()));
}

#[test]
fn auto_attach_off() {
    let repo = TestRepo::new();
    let data = tempfile::tempdir().unwrap();
    fs::write(data.path().join("config.toml"), "[review]\nauto_attach_comments = false\n").unwrap();
    let wt = worktree_id(repo.path());
    seed_comment(data.path(), &wt, "a.rs", 1, "x");

    assert_quiet_exit(&submit(data.path(), repo.path(), Some("go")));
    assert!(all_unsent(data.path(), &wt));
    // turn 기록은 그대로 된다
    assert_eq!(session(data.path(), SID).read_meta().unwrap().turn, 1);
}

#[test]
fn slash_command_does_not_attach() {
    let repo = TestRepo::new();
    let data = tempfile::tempdir().unwrap();
    let wt = worktree_id(repo.path());
    seed_comment(data.path(), &wt, "a.rs", 1, "x");

    assert_quiet_exit(&submit(data.path(), repo.path(), Some("/compact")));
    assert!(all_unsent(data.path(), &wt));

    // prompt 필드가 없으면 첨부한다
    let json = stdout_json(&submit(data.path(), repo.path(), None));
    assert!(json["hookSpecificOutput"]["additionalContext"].as_str().is_some());
}

#[test]
fn missing_or_old_db_is_quiet() {
    let repo = TestRepo::new();
    let data = tempfile::tempdir().unwrap();
    let db_path = data.path().join("state.sqlite");

    // 파일이 없다: 조용하고 만들지 않는다
    assert_quiet_exit(&submit(data.path(), repo.path(), Some("go")));
    assert!(!db_path.exists());

    // version 0 DB
    {
        let conn = db::open_state(&db_path).unwrap();
        conn.execute_batch("PRAGMA user_version = 0").unwrap();
    }
    assert_quiet_exit(&submit(data.path(), repo.path(), Some("go")));

    // 깨진 파일
    fs::write(&db_path, vec![0xAB; 4096]).unwrap();
    let out = submit(data.path(), repo.path(), Some("go"));
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).trim_end().lines().count() <= 1);
    // turn 기록은 막히지 않는다
    assert!(session(data.path(), SID).read_meta().unwrap().turn >= 1);
}

#[test]
fn locked_db_leaves_comments_unsent() {
    let repo = TestRepo::new();
    let data = tempfile::tempdir().unwrap();
    let wt = worktree_id(repo.path());
    seed_comment(data.path(), &wt, "a.rs", 1, "x");

    let holder = db::open_state(&data.path().join("state.sqlite")).unwrap();
    holder.execute_batch("BEGIN EXCLUSIVE").unwrap();
    let start = Instant::now();
    let out = submit(data.path(), repo.path(), Some("go"));
    let elapsed = start.elapsed();
    holder.execute_batch("ROLLBACK").unwrap();

    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "stdout: {}", String::from_utf8_lossy(&out.stdout));
    assert!(String::from_utf8_lossy(&out.stderr).trim_end().lines().count() <= 1);
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    assert!(all_unsent(data.path(), &wt));
}

#[test]
fn overflow_attaches_rest_next_time() {
    let repo = TestRepo::new();
    let data = tempfile::tempdir().unwrap();
    let wt = worktree_id(repo.path());
    for line in 1..=3 {
        seed_comment(data.path(), &wt, "a.rs", line, &format!("{line}번 {}", "가".repeat(4000 - 4)));
    }

    let first = stdout_json(&submit(data.path(), repo.path(), Some("go")));
    let context = first["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(context.chars().count() <= 9_000);
    assert!(context.contains("1번") && context.contains("2번") && !context.contains("3번"));
    assert!(first["systemMessage"].as_str().unwrap().contains("1개 남음"));
    assert_eq!(sent_states(data.path(), &wt).iter().filter(|(_, to)| to.is_some()).count(), 2);

    let second = stdout_json(&submit(data.path(), repo.path(), Some("go")));
    let context = second["hookSpecificOutput"]["additionalContext"].as_str().unwrap();
    assert!(context.contains("3번") && !context.contains("1번"));
    assert!(!second["systemMessage"].as_str().unwrap().contains("남음"));
    assert!(sent_states(data.path(), &wt).iter().all(|(_, to)| to.is_some()));
}

#[test]
fn attach_keeps_index_untouched() {
    let repo = TestRepo::new();
    repo.write("a.txt", "a\n");
    let data = tempfile::tempdir().unwrap();
    let wt = worktree_id(repo.path());
    seed_comment(data.path(), &wt, "a.txt", 1, "x");

    let index = repo.path().join(".git/index");
    let before = (fs::read(&index).unwrap(), fs::metadata(&index).unwrap().modified().unwrap());
    stdout_json(&submit(data.path(), repo.path(), Some("go")));
    let after = (fs::read(&index).unwrap(), fs::metadata(&index).unwrap().modified().unwrap());
    assert_eq!(before, after);
}
