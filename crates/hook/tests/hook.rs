mod common;

use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
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
    // UTF-8이 아닌 argv
    let mut cmd = hook_command(data.path(), &[]);
    cmd.arg(OsStr::from_bytes(b"turn-\xff"));
    assert_quiet_exit(&run_with_stdin(cmd, br#"{"session_id":"s","cwd":"/"}"#));
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

/// 세션 기록에는 편집한 파일(.env 등)의 원본이 들어간다. 다른 사용자가 못 읽어야 한다
#[test]
fn session_records_are_private() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    let secret = work.path().join("cfg/.env");
    fs::create_dir_all(secret.parent().unwrap()).unwrap();
    fs::write(&secret, "TOKEN=1\n").unwrap();
    fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();

    hook(data.path(), "session-start", SID, work.path(), "SessionStart");
    prompt(data.path(), SID);
    edit(data.path(), SID, &secret, Some("TOKEN=2\n"));
    hook(data.path(), "turn-end", SID, work.path(), "Stop");

    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let sessions = data.path().join("sessions");
    assert_eq!(mode(&sessions), 0o700);
    let all = tree(&sessions);
    assert!(all.iter().any(|p| p.ends_with("turns/1/index.jsonl")));
    for p in all {
        let want = if p.is_dir() { 0o700 } else { 0o600 };
        assert_eq!(mode(&p), want, "{}", p.display());
    }
}

fn bash_input(sid: &str, cwd: &Path, tool_use_id: Option<&str>) -> Value {
    let mut input = with(
        base(sid, cwd, "PreToolUse"),
        json!({ "tool_name": "Bash", "tool_input": { "command": "make" } }),
    );
    if let Some(id) = tool_use_id {
        input["tool_use_id"] = json!(id);
    }
    input
}

/// `pre-bash` → `run` → `post-bash`. Claude Code가 Bash tool 하나를 부를 때와 같다
fn bash_with_id(data: &Path, sid: &str, cwd: &Path, tool_use_id: Option<&str>, run: impl FnOnce()) {
    let input = bash_input(sid, cwd, tool_use_id);
    assert_quiet_exit(&run_hook(data, "pre-bash", &input));
    run();
    assert_quiet_exit(&run_hook(data, "post-bash", &input));
}

fn bash(data: &Path, sid: &str, cwd: &Path, run: impl FnOnce()) {
    bash_with_id(data, sid, cwd, Some("toolu_bash"), run);
}

/// session-start (start_head = 지금 HEAD)와 prompt 하나. hook이 쓰는 경로는 `git rev-parse --show-toplevel`이라
/// realpath 기준이다 (macOS tempdir `/var` → `/private/var`)
fn bash_session(data: &Path, repo: &TestRepo) -> PathBuf {
    hook(data, "session-start", SID, repo.path(), "SessionStart");
    prompt(data, SID);
    fs::canonicalize(repo.path()).unwrap()
}

fn bash_events(dir: &wtboard_core::session::SessionDir) -> Vec<wtboard_core::session::Event> {
    dir.events().into_iter().filter(|e| e.kind == EventKind::Bash).collect()
}

fn baseline_text(dir: &wtboard_core::session::SessionDir, path: &Path) -> String {
    fs::read_to_string(dir.baseline_file(path)).unwrap()
}

#[test]
fn bash_edit_of_clean_tracked_file() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("a.py", "a = 1\n");
    repo.commit_all("a");
    let root = bash_session(data.path(), &repo);

    let input =
        with(bash_input(SID, repo.path(), Some("toolu_1")), json!({ "agent_id": "agent-3" }));
    assert_quiet_exit(&run_hook(data.path(), "pre-bash", &input));
    repo.write("a.py", "a = 2\n");
    assert_quiet_exit(&run_hook(data.path(), "post-bash", &input));

    let dir = session(data.path(), SID);
    let a = root.join("a.py");
    assert_eq!(dir.baselines()[&a].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &a), "a = 1\n");
    let events = bash_events(&dir);
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].paths, [a]);
    assert_eq!(events[0].tool.as_deref(), Some("Bash"));
    assert_eq!(events[0].turn, 1);
    assert_eq!(events[0].agent_id.as_deref(), Some("agent-3"));
}

#[test]
fn bash_new_untracked_file() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let root = bash_session(data.path(), &repo);

    // 하위 폴더 안의 파일도 하나씩 잡는다 (`--untracked-files=all`). 이름에 공백
    bash(data.path(), SID, repo.path(), || repo.write("out/new file.txt", "x"));

    let dir = session(data.path(), SID);
    let new = root.join("out/new file.txt");
    assert_eq!(dir.baselines()[&new].kind, BaselineKind::New);
    assert_eq!(bash_events(&dir)[0].paths, [new]);
}

#[test]
fn bash_reedit_of_dirty_file_is_uncertain() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("b.py", "b = 1\n");
    repo.commit_all("b");
    let start_head = repo.head();
    let root = bash_session(data.path(), &repo);
    // 세션 밖(사용자)에서 고친 파일
    repo.write("b.py", "b = 2\n");

    bash(data.path(), SID, repo.path(), || repo.write("b.py", "b = 33\n"));

    let dir = session(data.path(), SID);
    let b = root.join("b.py");
    let baseline = &dir.baselines()[&b];
    assert_eq!(baseline.kind, BaselineKind::Uncertain);
    assert_eq!(baseline.head.as_deref(), Some(start_head.as_str()));
    assert_eq!(baseline_text(&dir, &b), "b = 1\n");
    assert_eq!(bash_events(&dir)[0].paths, [b]);
}

#[test]
fn bash_revert_is_uncertain() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("b.py", "b = 1\n");
    repo.commit_all("b");
    let start_head = repo.head();
    let root = bash_session(data.path(), &repo);
    repo.write("b.py", "b = 2\n");

    bash(data.path(), SID, repo.path(), || {
        repo.git(&["checkout", "--", "b.py"]);
    });

    let dir = session(data.path(), SID);
    let b = root.join("b.py");
    let baseline = &dir.baselines()[&b];
    assert_eq!(baseline.kind, BaselineKind::Uncertain);
    assert_eq!(baseline.head.as_deref(), Some(start_head.as_str()));
    assert_eq!(baseline_text(&dir, &b), "b = 1\n");
    assert_eq!(bash_events(&dir)[0].paths, [b]);
}

#[test]
fn bash_commit_uses_pre_head() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("c.py", "c = 1\n");
    repo.commit_all("c1");
    let root = bash_session(data.path(), &repo);
    // 세션 시작 뒤 (Bash 전) HEAD가 한 번 바뀐다
    repo.write("c.py", "c = 2\n");
    repo.commit_all("c2");

    // Bash 안에서 고치고 커밋하고 또 고친다. 원본은 Bash 직전 HEAD의 내용
    bash(data.path(), SID, repo.path(), || {
        repo.write("c.py", "c = 3\n");
        repo.commit_all("c3");
        repo.write("c.py", "c = 4\n");
    });

    let dir = session(data.path(), SID);
    let c = root.join("c.py");
    assert_eq!(dir.baselines()[&c].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &c), "c = 2\n");
}

#[test]
fn bash_already_baselined_keeps_baseline() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("a.py", "a = 1\n");
    repo.commit_all("a");
    let root = bash_session(data.path(), &repo);
    let a = root.join("a.py");
    edit(data.path(), SID, &a, Some("a = 2\n"));

    bash(data.path(), SID, repo.path(), || repo.write("a.py", "a = 33\n"));

    let dir = session(data.path(), SID);
    assert_eq!(dir.baselines()[&a].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &a), "a = 1\n");
    assert_eq!(lines(&dir.root().join("baseline.jsonl")), 1);
    assert_eq!(bash_events(&dir)[0].paths, [a]);
}

#[test]
fn bash_outside_repo_does_nothing() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    hook(data.path(), "session-start", SID, work.path(), "SessionStart");

    let input = bash_input(SID, work.path(), Some("toolu_1"));
    for sub in ["pre-bash", "post-bash"] {
        if sub == "post-bash" {
            fs::write(work.path().join("a.txt"), "x").unwrap();
        }
        let out = run_hook(data.path(), sub, &input);
        assert_quiet_exit(&out);
        assert!(out.stderr.is_empty(), "{sub}: {}", String::from_utf8_lossy(&out.stderr));
    }

    let dir = session(data.path(), SID);
    assert!(!dir.root().join("bash").exists());
    assert!(dir.events().is_empty());
}

#[test]
fn bash_without_tool_use_id_does_nothing() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    bash_session(data.path(), &repo);
    let before = tree(data.path());

    bash_with_id(data.path(), SID, repo.path(), None, || repo.write("a.txt", "x"));
    // 파일 이름으로 쓰므로 세션 폴더 밖을 가리키는 id는 받지 않는다
    bash_with_id(data.path(), SID, repo.path(), Some("../../evil"), || repo.write("b.txt", "x"));

    assert_eq!(tree(data.path()), before);
}

#[test]
fn bash_snapshot_file_removed_after_post() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    bash_session(data.path(), &repo);
    let dir = session(data.path(), SID);
    let snapshot = dir.root().join("bash/toolu_9.json");

    let input = bash_input(SID, repo.path(), Some("toolu_9"));
    assert_quiet_exit(&run_hook(data.path(), "pre-bash", &input));
    assert!(snapshot.is_file());
    let mode = fs::metadata(&snapshot).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_quiet_exit(&run_hook(data.path(), "post-bash", &input));
    assert!(!snapshot.exists());
    // 바뀐 파일이 없으면 event도 없다
    assert!(bash_events(&dir).is_empty());

    // 스냅샷 없이 온 post-bash (pre-bash가 레포 밖이었거나 실패)는 아무것도 안 한다
    repo.write("a.txt", "x");
    assert_quiet_exit(&run_hook(data.path(), "post-bash", &input));
    assert!(bash_events(&dir).is_empty());
}

/// 한 Bash 안에서 깨끗한 파일을 고치고 커밋하면 전후 status 어디에도 안 나온다. HEAD 사이 diff로 잡는다
#[test]
fn bash_edit_and_commit_of_clean_file() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("c.py", "c = 1\n");
    repo.commit_all("c1");
    let root = bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.write("c.py", "c = 2\n");
        repo.commit_all("c2");
    });

    let dir = session(data.path(), SID);
    let c = root.join("c.py");
    assert_eq!(dir.baselines()[&c].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &c), "c = 1\n");
    assert_eq!(bash_events(&dir)[0].paths, [c]);
}

#[test]
fn bash_git_mv_records_both_paths() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("old.py", "o = 1\n");
    repo.commit_all("old");
    let root = bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.git(&["mv", "old.py", "new.py"]);
    });

    let dir = session(data.path(), SID);
    let (old, new) = (root.join("old.py"), root.join("new.py"));
    let baselines = dir.baselines();
    assert_eq!(baselines[&old].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &old), "o = 1\n");
    assert_eq!(baselines[&new].kind, BaselineKind::New);
    let paths: BTreeSet<_> = bash_events(&dir)[0].paths.iter().cloned().collect();
    assert_eq!(paths, BTreeSet::from([old, new]));
}

/// `git rm --cached`는 같은 경로를 `1 D.`와 `?` 두 줄로 낸다. 원본은 pre HEAD 내용
#[test]
fn bash_rm_cached_keeps_pre_head_content() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("a.py", "a = 1\n");
    repo.commit_all("a");
    let root = bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.git(&["rm", "-q", "--cached", "a.py"]);
    });

    let dir = session(data.path(), SID);
    let a = root.join("a.py");
    assert_eq!(dir.baselines()[&a].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &a), "a = 1\n");
    assert_eq!(bash_events(&dir)[0].paths, [a]);
}

#[test]
fn bash_delete_of_clean_tracked_file() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.write("a.py", "a = 1\n");
    repo.commit_all("a");
    let root = bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || repo.remove("a.py"));

    let dir = session(data.path(), SID);
    let a = root.join("a.py");
    assert_eq!(dir.baselines()[&a].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &a), "a = 1\n");
    assert_eq!(bash_events(&dir)[0].paths, [a]);
}

#[test]
fn bash_delete_of_untracked_file_is_uncertain() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let root = bash_session(data.path(), &repo);
    repo.write("tmp.log", "x");

    bash(data.path(), SID, repo.path(), || repo.remove("tmp.log"));

    let dir = session(data.path(), SID);
    let log = root.join("tmp.log");
    assert_eq!(dir.baselines()[&log].kind, BaselineKind::Uncertain);
    assert_eq!(baseline_text(&dir, &log), "");
    assert_eq!(bash_events(&dir)[0].paths, [log]);
}

/// 여러 파일의 원본을 한 번에 읽는다 (`cat-file --batch`). 1MB를 넘는 원본은 크기와 hash만
#[test]
fn bash_many_files_and_large_original() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let big = vec![b'x'; wtboard_core::domain::MAX_DIFF_BYTES as usize + 10];
    repo.write("big.bin", &big);
    for i in 0..5 {
        repo.write(&format!("f {i}.txt"), format!("{i}\n"));
    }
    repo.commit_all("files");
    let root = bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.write("big.bin", "small now");
        for i in 0..5 {
            repo.write(&format!("f {i}.txt"), "changed\n");
        }
    });

    let dir = session(data.path(), SID);
    let baselines = dir.baselines();
    for i in 0..5 {
        let f = root.join(format!("f {i}.txt"));
        assert_eq!(baselines[&f].kind, BaselineKind::File);
        assert_eq!(baseline_text(&dir, &f), format!("{i}\n"));
    }
    let b = &baselines[&root.join("big.bin")];
    assert_eq!(b.kind, BaselineKind::TooLarge);
    assert_eq!(b.size, Some(big.len() as u64));
    assert_eq!(b.hash.as_deref(), Some(blake3::hash(&big).to_hex().as_str()));
    assert_eq!(bash_events(&dir)[0].paths.len(), 6);
}

/// committer 시각이 오래전인 커밋(x.py)이 있는 `other` 브랜치. `diverged`면 main에도 새 커밋(a.py)을 더해 갈라 놓는다
fn repo_with_other_branch(diverged: bool) -> TestRepo {
    let repo = TestRepo::new();
    repo.git(&["checkout", "-q", "-b", "other"]);
    repo.write("x.py", "x = 1\n");
    repo.git(&["add", "-A"]);
    let out = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "x"])
        .current_dir(repo.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_COMMITTER_DATE", "2001-01-01T00:00:00Z")
        .output()
        .unwrap();
    assert!(out.status.success());
    repo.git(&["checkout", "-q", "main"]);
    if diverged {
        repo.write("a.py", "a = 1\n");
        repo.commit_all("a");
    }
    repo
}

/// Bash 안의 브랜치 전환은 그 Bash가 고친 것이 아니다. HEAD 사이 경로를 세션에 넣지 않는다
#[test]
fn bash_checkout_of_other_branch_records_nothing() {
    let data = tempfile::tempdir().unwrap();
    let repo = repo_with_other_branch(true);
    bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.git(&["checkout", "-q", "other"]);
    });

    let dir = session(data.path(), SID);
    assert!(dir.baselines().is_empty(), "{:?}", dir.baselines());
    assert!(bash_events(&dir).is_empty());
}

/// `git pull`·`merge --ff-only`로 들어온 남의 (Bash 전에 만든) 커밋도 넣지 않는다
#[test]
fn bash_fast_forward_of_older_commit_records_nothing() {
    let data = tempfile::tempdir().unwrap();
    let repo = repo_with_other_branch(false);
    bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.git(&["merge", "-q", "--ff-only", "other"]);
    });

    let dir = session(data.path(), SID);
    assert!(dir.baselines().is_empty(), "{:?}", dir.baselines());
    assert!(bash_events(&dir).is_empty());
}

/// `-uall`도 안쪽 레포·worktree는 `? inner/` 한 줄로 낸다. 파일이 아니므로 원본을 만들지 않는다
/// (만들면 `baseline/.../inner`가 파일이 되어 그 안 파일의 원본을 못 남긴다)
#[test]
fn bash_nested_repo_is_not_a_file() {
    let data = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    let root = bash_session(data.path(), &repo);

    bash(data.path(), SID, repo.path(), || {
        repo.git(&["init", "-q", "inner"]);
        repo.write("inner/a.py", "a = 1\n");
    });

    let dir = session(data.path(), SID);
    assert!(dir.baselines().is_empty(), "{:?}", dir.baselines());
    assert!(!dir.baseline_file(&root.join("inner")).exists());
    assert!(bash_events(&dir).is_empty());

    let a = root.join("inner/a.py");
    let out = run_hook(data.path(), "pre-edit", &edit_input(SID, &a));
    assert_quiet_exit(&out);
    assert!(out.stderr.is_empty(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(dir.baselines()[&a].kind, BaselineKind::File);
    assert_eq!(baseline_text(&dir, &a), "a = 1\n");
}

/// 상대 경로 `WTBOARD_CONFIG`면 data dir가 hook의 cwd (레포) 기준이 된다. 아무것도 쓰지 않고 오류 한 줄
#[test]
fn relative_data_dir_records_nothing() {
    let repo = TestRepo::new();
    let file = repo.path().join("a.py");
    for data in ["rel", ""] {
        let before = tree(repo.path());
        for sub in ["session-start", "prompt-submit", "pre-edit", "post-edit", "pre-bash", "turn-end", "session-end"] {
            let mut cmd = hook_command(Path::new(data), &[sub]);
            cmd.current_dir(repo.path());
            let out = run_with_stdin(cmd, edit_input(SID, &file).to_string().as_bytes());
            assert_quiet_exit(&out);
            assert!(!out.stderr.is_empty(), "{data:?} {sub}: 오류를 stderr에 남긴다");
        }
        assert_eq!(tree(repo.path()), before, "{data:?}");
    }
}

/// turn 스냅샷을 못 찍어도 (turn 폴더를 못 만듦) turn 번호는 넘어간다. 오류는 stderr 한 줄
#[test]
fn snapshot_failure_still_advances_turn() {
    let data = tempfile::tempdir().unwrap();
    let work = tempfile::tempdir().unwrap();
    hook(data.path(), "session-start", SID, work.path(), "SessionStart");
    prompt(data.path(), SID);
    let dir = session(data.path(), SID);
    fs::write(dir.root().join("turns"), "not a dir").unwrap();

    for want in [2, 3] {
        let input = with(base(SID, Path::new("/"), "UserPromptSubmit"), json!({ "prompt": "go" }));
        let out = run_hook(data.path(), "prompt-submit", &input);
        assert_quiet_exit(&out);
        assert!(!out.stderr.is_empty());
        let meta = dir.read_meta().unwrap();
        assert_eq!((meta.turn, meta.turn_open), (want, true));
    }
    let out = run_hook(data.path(), "turn-end", &base(SID, work.path(), "Stop"));
    assert_quiet_exit(&out);
    assert!(!out.stderr.is_empty());
    assert!(!dir.read_meta().unwrap().turn_open);
}

#[test]
fn version_prints_schema_without_reading_stdin() {
    let data = tempfile::tempdir().unwrap();
    let out = run_with_stdin(hook_command(data.path(), &["version"]), b"");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("wtboard-hook {} schema 2\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(out.stderr.is_empty(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
}
