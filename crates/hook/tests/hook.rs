mod common;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use wtboard_core::session::{BaselineKind, EventKind, Snapshot};

use common::{assert_quiet_exit, hook_command, run_hook, run_with_stdin, session, TestRepo};

const SID: &str = "sess-1";

/// Claude Code가 모든 hook에 넣는 공통 필드. hook이 모르는 필드도 섞는다
fn base(sid: &str, cwd: &Path, event: &str) -> Value {
    json!({
        "session_id": sid,
        "transcript_path": "/tmp/transcript.jsonl",
        "cwd": cwd,
        "permission_mode": "default",
        "hook_event_name": event,
    })
}

fn with(mut v: Value, extra: Value) -> Value {
    let (Value::Object(m), Value::Object(e)) = (&mut v, extra) else { unreachable!() };
    m.extend(e);
    v
}

fn edit_input(sid: &str, path: &Path) -> Value {
    with(
        base(sid, Path::new("/"), "PreToolUse"),
        json!({
            "tool_name": "Edit",
            "tool_input": { "file_path": path, "old_string": "a", "new_string": "b" },
            "tool_use_id": "toolu_1",
        }),
    )
}

/// `pre-edit` → `write` → `post-edit`. Claude Code가 Edit tool 하나를 부를 때와 같다
fn edit(data: &Path, sid: &str, path: &Path, contents: Option<&str>) {
    assert_quiet_exit(&run_hook(data, "pre-edit", &edit_input(sid, path)));
    match contents {
        Some(c) => fs::write(path, c).unwrap(),
        None => fs::remove_file(path).unwrap(),
    }
    assert_quiet_exit(&run_hook(data, "post-edit", &edit_input(sid, path)));
}

fn hook(data: &Path, sub: &str, sid: &str, cwd: &Path, event: &str) {
    assert_quiet_exit(&run_hook(data, sub, &base(sid, cwd, event)));
}

fn prompt(data: &Path, sid: &str) {
    let input = with(base(sid, Path::new("/"), "UserPromptSubmit"), json!({ "prompt": "go" }));
    assert_quiet_exit(&run_hook(data, "prompt-submit", &input));
}

fn lines(path: &Path) -> usize {
    fs::read_to_string(path).unwrap_or_default().lines().count()
}

/// `root` 아래 모든 경로
fn tree(root: &Path) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p.clone());
            }
            out.insert(p);
        }
    }
    out
}

#[test]
fn any_input_exits_zero_with_empty_stdout() {
    let data = tempfile::tempdir().unwrap();
    let cases: [(&[&str], &[u8]); 6] = [
        (&["session-start"], b""),
        (&["post-edit"], b"not json"),
        (&["prompt-submit"], b"[1, 2]"),
        (&["no-such-subcommand"], br#"{"session_id":"s","cwd":"/"}"#),
        (&[], br#"{"session_id":"s","cwd":"/"}"#),
        (&["turn-end"], br#"{"cwd":"/"}"#),
    ];
    for (args, stdin) in cases {
        let out = run_with_stdin(hook_command(data.path(), args), stdin);
        assert_quiet_exit(&out);
    }
    // 모든 subcommand가 session_id 없는 입력에서 조용히 끝난다
    for sub in [
        "session-start", "prompt-submit", "pre-edit", "post-edit", "pre-bash", "post-bash",
        "turn-end", "session-end",
    ] {
        assert_quiet_exit(&run_hook(data.path(), sub, &json!({ "cwd": "/" })));
    }
    assert!(!data.path().join("sessions").exists());
}

#[test]
fn unwritable_data_dir_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("not-a-dir");
    fs::write(&data, "file").unwrap();
    let file = tmp.path().join("a.txt");
    fs::write(&file, "v1").unwrap();

    for sub in ["session-start", "prompt-submit", "pre-edit", "post-edit", "turn-end", "session-end"] {
        let start = Instant::now();
        let out = run_hook(&data, sub, &edit_input(SID, &file));
        assert!(start.elapsed() < Duration::from_secs(1), "{sub}: {:?}", start.elapsed());
        assert_quiet_exit(&out);
        // turn-end는 meta가 없어 (session-start가 못 씀) 닫을 turn이 없다
        if sub != "turn-end" {
            assert!(!out.stderr.is_empty(), "{sub}: 오류를 stderr에 남긴다");
        }
    }
    assert_eq!(fs::read_to_string(&data).unwrap(), "file");
}

#[test]
fn rejects_traversal() {
    let tmp = tempfile::tempdir().unwrap();
    let data = tmp.path().join("data");
    fs::create_dir(&data).unwrap();
    let file = tmp.path().join("a.txt");
    fs::write(&file, "v1").unwrap();
    let before = tree(tmp.path());

    // session_id에 `..`
    for sub in ["session-start", "prompt-submit", "pre-edit", "post-edit", "turn-end", "session-end"] {
        let out = run_hook(&data, sub, &edit_input("../evil", &file));
        assert_quiet_exit(&out);
    }
    // 경로에 `..`, 상대 경로
    let sneaky = tmp.path().join("x/../a.txt");
    for path in [Path::new("/tmp/../etc/x"), sneaky.as_path(), Path::new("rel/a.txt")] {
        for sub in ["pre-edit", "post-edit"] {
            assert_quiet_exit(&run_hook(&data, sub, &edit_input(SID, path)));
        }
    }
    let input = with(edit_input(SID, &file), json!({ "tool_input": { "notebook_path": "../a" } }));
    assert_quiet_exit(&run_hook(&data, "pre-edit", &input));

    assert_eq!(tree(tmp.path()), before);
}

#[test]
fn session_start_creates_meta_and_resume_keeps_it() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let start = |source: &str| {
        let input = with(base(SID, repo.path(), "SessionStart"), json!({ "source": source }));
        assert_quiet_exit(&run_hook(data.path(), "session-start", &input));
    };
    start("startup");
    let dir = session(data.path(), SID);
    let meta = dir.read_meta().unwrap();
    assert_eq!(meta.turn, 0);
    assert!(!meta.turn_open);
    assert_eq!(meta.project_dir, repo.path());
    assert_eq!(meta.start_head.as_deref(), Some(repo.head().as_str()));
    assert_eq!(meta.ended_at, None);

    // 열린 turn을 닫고 끝난다
    let file = repo.path().join("a.py");
    prompt(data.path(), SID);
    edit(data.path(), SID, &file, Some("v1"));
    hook(data.path(), "session-end", SID, repo.path(), "SessionEnd");
    let ended = dir.read_meta().unwrap();
    assert!(ended.ended_at.is_some());
    assert!(!ended.turn_open);
    assert!(matches!(dir.turn_snapshot(1, &file), Some(Snapshot::Content(_))));
    assert_eq!(dir.events().last().unwrap().kind, EventKind::TurnEnd);

    // resume은 meta를 새로 만들지 않고 ended_at만 지운다
    repo.commit_all("later");
    start("resume");
    let resumed = dir.read_meta().unwrap();
    assert_eq!(resumed.ended_at, None);
    assert_eq!(resumed.turn, 1);
    assert_eq!(resumed.started_at, meta.started_at);
    assert_eq!(resumed.start_head, meta.start_head);
}

#[test]
fn session_start_project_dir() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let other = tempfile::tempdir().unwrap();

    // CLAUDE_PROJECT_DIR가 cwd보다 앞선다
    let mut cmd = hook_command(data.path(), &["session-start"]);
    cmd.env("CLAUDE_PROJECT_DIR", repo.path());
    let input = base("with-env", other.path(), "SessionStart");
    assert_quiet_exit(&run_with_stdin(cmd, input.to_string().as_bytes()));
    let meta = session(data.path(), "with-env").read_meta().unwrap();
    assert_eq!(meta.project_dir, repo.path());
    assert_eq!(meta.start_head.as_deref(), Some(repo.head().as_str()));

    // git 레포가 아니면 start_head 없음
    hook(data.path(), "session-start", "no-repo", other.path(), "SessionStart");
    let meta = session(data.path(), "no-repo").read_meta().unwrap();
    assert_eq!(meta.project_dir, other.path());
    assert_eq!(meta.start_head, None);
}

#[test]
fn pre_edit_baselines_existing_and_new() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let existing = work.path().join("a.txt");
    let missing = work.path().join("sub/b.txt");
    fs::write(&existing, "v1").unwrap();

    for path in [&existing, &missing] {
        assert_quiet_exit(&run_hook(data.path(), "pre-edit", &edit_input(SID, path)));
    }
    let dir = session(data.path(), SID);
    let baselines = dir.baselines();
    assert_eq!(baselines[&existing].kind, BaselineKind::File);
    assert_eq!(fs::read_to_string(dir.baseline_file(&existing)).unwrap(), "v1");
    assert_eq!(baselines[&missing].kind, BaselineKind::New);

    // 두 번째 pre-edit은 원본을 바꾸지 않는다
    fs::write(&existing, "v2").unwrap();
    assert_quiet_exit(&run_hook(data.path(), "pre-edit", &edit_input(SID, &existing)));
    assert_eq!(fs::read_to_string(dir.baseline_file(&existing)).unwrap(), "v1");
    assert_eq!(lines(&dir.root().join("baseline.jsonl")), 2);
}

/// `n`개 프로세스를 한꺼번에 띄운다
fn parallel(n: usize, f: impl Fn(usize) + Send + Sync + 'static) {
    let f = Arc::new(f);
    let barrier = Arc::new(Barrier::new(n));
    let handles: Vec<_> = (0..n)
        .map(|i| {
            let (f, barrier) = (f.clone(), barrier.clone());
            thread::spawn(move || {
                barrier.wait();
                f(i)
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn parallel_pre_edit_one_baseline() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let file = work.path().join("a.txt");
    fs::write(&file, "v1").unwrap();

    let (d, f) = (data.path().to_path_buf(), file.clone());
    parallel(10, move |_| assert_quiet_exit(&run_hook(&d, "pre-edit", &edit_input(SID, &f))));

    let dir = session(data.path(), SID);
    assert_eq!(dir.baselines().len(), 1);
    assert_eq!(lines(&dir.root().join("baseline.jsonl")), 1);
    assert_eq!(fs::read_to_string(dir.baseline_file(&file)).unwrap(), "v1");
}

#[test]
fn parallel_appends_stay_line_atomic() {
    let data = tempfile::tempdir().unwrap();
    let d = data.path().to_path_buf();
    parallel(50, move |i| {
        let path = PathBuf::from(format!("/work/{i:02}-{}.txt", "x".repeat(200)));
        assert_quiet_exit(&run_hook(&d, "post-edit", &edit_input(SID, &path)));
    });

    let dir = session(data.path(), SID);
    let events = dir.events();
    assert_eq!(events.len(), 50);
    assert_eq!(lines(&dir.root().join("events.jsonl")), 50);
    let paths: BTreeSet<_> = events.iter().flat_map(|e| e.paths.clone()).collect();
    assert_eq!(paths.len(), 50);
}

#[test]
fn turn_lifecycle_snapshots() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (a, b) = (work.path().join("a.py"), work.path().join("b.py"));
    fs::write(&a, "v1").unwrap();
    fs::write(&b, "b").unwrap();

    hook(data.path(), "session-start", SID, work.path(), "SessionStart");
    prompt(data.path(), SID);
    let dir = session(data.path(), SID);
    let meta = dir.read_meta().unwrap();
    assert_eq!((meta.turn, meta.turn_open), (1, true));

    edit(data.path(), SID, &a, Some("v2"));
    edit(data.path(), SID, &b, None);
    hook(data.path(), "turn-end", SID, work.path(), "Stop");

    let Some(Snapshot::Content(snap)) = dir.turn_snapshot(1, &a) else { panic!("no snapshot") };
    assert_eq!(fs::read_to_string(snap).unwrap(), "v2");
    assert_eq!(dir.turn_snapshot(1, &b), Some(Snapshot::Deleted));
    assert_eq!(fs::read_to_string(dir.baseline_file(&a)).unwrap(), "v1");

    let events = dir.events();
    let kinds: Vec<_> = events.iter().map(|e| (e.kind, e.turn)).collect();
    assert_eq!(
        kinds,
        [
            (EventKind::TurnStart, 1),
            (EventKind::Edit, 1),
            (EventKind::Edit, 1),
            (EventKind::TurnEnd, 1),
        ]
    );
    assert_eq!(events[1].tool.as_deref(), Some("Edit"));
    assert_eq!(events[1].paths, [a.clone()]);
    let meta = dir.read_meta().unwrap();
    assert_eq!((meta.turn, meta.turn_open), (1, false));

    // 닫힌 turn에 Stop이 또 와도 아무 일 없다
    hook(data.path(), "turn-end", SID, work.path(), "Stop");
    assert_eq!(dir.events().len(), 4);
}

#[test]
fn prompt_without_stop_closes_previous_turn() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let a = work.path().join("a.py");
    fs::write(&a, "v1").unwrap();

    hook(data.path(), "session-start", SID, work.path(), "SessionStart");
    prompt(data.path(), SID);
    edit(data.path(), SID, &a, Some("v2"));
    prompt(data.path(), SID);

    let dir = session(data.path(), SID);
    let Some(Snapshot::Content(snap)) = dir.turn_snapshot(1, &a) else { panic!("no snapshot") };
    assert_eq!(fs::read_to_string(snap).unwrap(), "v2");
    let meta = dir.read_meta().unwrap();
    assert_eq!((meta.turn, meta.turn_open), (2, true));
    let kinds: Vec<_> = dir.events().iter().map(|e| (e.kind, e.turn)).collect();
    assert_eq!(
        kinds,
        [
            (EventKind::TurnStart, 1),
            (EventKind::Edit, 1),
            (EventKind::TurnEnd, 1),
            (EventKind::TurnStart, 2),
        ]
    );
}

#[test]
fn events_before_first_prompt_are_turn_zero() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let (a, b) = (work.path().join("a.py"), work.path().join("b.py"));

    // meta 없이 (plugin을 세션 중간에 켬) 그리고 session-start 뒤 첫 prompt 전
    edit(data.path(), "no-meta", &a, Some("v1"));
    hook(data.path(), "session-start", SID, work.path(), "SessionStart");
    edit(data.path(), SID, &a, Some("v2"));
    for sid in ["no-meta", SID] {
        let events = session(data.path(), sid).events();
        assert_eq!(events.len(), 1);
        assert_eq!((events[0].kind, events[0].turn), (EventKind::Edit, 0));
    }

    // 앱은 turn 0을 turn 1에 합친다. turn 1 스냅샷에 turn 0 경로도 들어간다
    prompt(data.path(), SID);
    edit(data.path(), SID, &b, Some("b"));
    hook(data.path(), "turn-end", SID, work.path(), "Stop");
    let dir = session(data.path(), SID);
    assert!(matches!(dir.turn_snapshot(1, &a), Some(Snapshot::Content(_))));
    assert!(matches!(dir.turn_snapshot(1, &b), Some(Snapshot::Content(_))));

    // meta가 없던 세션도 prompt에서 meta를 만들고 turn 1을 연다
    prompt(data.path(), "no-meta");
    let meta = session(data.path(), "no-meta").read_meta().unwrap();
    assert_eq!((meta.turn, meta.turn_open), (1, true));
}

#[test]
fn notebook_path_is_accepted() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let nb = work.path().join("n.ipynb");
    fs::write(&nb, "{}").unwrap();
    let input = with(
        edit_input(SID, &nb),
        json!({ "tool_name": "NotebookEdit", "tool_input": { "notebook_path": nb, "new_source": "x" } }),
    );
    assert_quiet_exit(&run_hook(data.path(), "pre-edit", &input));
    assert_quiet_exit(&run_hook(data.path(), "post-edit", &input));

    let dir = session(data.path(), SID);
    assert_eq!(dir.baselines()[&nb].kind, BaselineKind::File);
    let events = dir.events();
    assert_eq!(events[0].paths, [nb.clone()]);
    assert_eq!(events[0].tool.as_deref(), Some("NotebookEdit"));
}

#[test]
fn subagent_edit_keeps_agent_id() {
    let data = tempfile::tempdir().unwrap();
    let input = with(edit_input(SID, Path::new("/w/a.py")), json!({ "agent_id": "agent-7" }));
    assert_quiet_exit(&run_hook(data.path(), "post-edit", &input));
    let events = session(data.path(), SID).events();
    assert_eq!(events[0].agent_id.as_deref(), Some("agent-7"));
}

#[test]
fn hook_never_touches_git_index() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("README.md", "changed\n");
    let file = repo.path().join("README.md");
    // 테스트의 git status는 index를 refresh할 수 있으므로 기록 전에 부른다
    let status = repo.git(&["status", "--porcelain"]);
    let index = repo.path().join(".git/index");
    let bytes = fs::read(&index).unwrap();
    let mtime = fs::metadata(&index).unwrap().modified().unwrap();

    let mut input = with(
        base(SID, repo.path(), "PreToolUse"),
        json!({ "tool_name": "Edit", "tool_input": { "file_path": file }, "tool_use_id": "t1" }),
    );
    for sub in [
        "session-start", "prompt-submit", "pre-edit", "post-edit", "pre-bash", "post-bash",
        "turn-end", "session-end",
    ] {
        if sub.ends_with("bash") {
            input["tool_name"] = json!("Bash");
            input["tool_input"] = json!({ "command": "true" });
        }
        assert_quiet_exit(&run_hook(data.path(), sub, &input));
    }

    assert_eq!(fs::read(&index).unwrap(), bytes);
    assert_eq!(fs::metadata(&index).unwrap().modified().unwrap(), mtime);
    assert_eq!(repo.git(&["status", "--porcelain"]), status);
}
