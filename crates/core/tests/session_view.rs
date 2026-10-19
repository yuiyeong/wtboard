mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};

use common::{feat_repo, TestRepo};
use wtboard_core::domain::{FileStatus, PatchBody, MAX_DIFF_BYTES};
use wtboard_core::session::list::{active_roots, cleanup, sessions_for, worktrees_of, ACTIVE_WINDOW_MS};
use wtboard_core::session::view::{changes, patch, SessionView};
use wtboard_core::session::{BaselineContent, Event, EventKind, Meta, SessionDir};
use wtboard_core::worktree::{FileEntry, Worktree, WorktreeError};

const MIN: u64 = 60 * 1000;
const DAY: u64 = 24 * 60 * MIN;

/// feat 브랜치 레포 + 빈 data dir + 세션 `s1` (turn 0, 닫힘)
struct Fx {
    repo: TestRepo,
    data: tempfile::TempDir,
    wt: Arc<Worktree>,
    dir: SessionDir,
}

fn setup() -> Fx {
    let repo = feat_repo();
    let wt = Worktree::open(repo.path(), Some("main".into())).unwrap();
    let data = tempfile::tempdir().unwrap();
    let dir = SessionDir::open(data.path(), "s1").unwrap();
    let fx = Fx { repo, data, wt, dir };
    fx.set_turn(0, false);
    fx
}

impl Fx {
    fn root(&self) -> &Path {
        &self.wt.layout().root
    }

    fn abs(&self, rel: &str) -> PathBuf {
        self.root().join(rel)
    }

    fn set_turn(&self, turn: u32, open: bool) {
        let meta = Meta {
            started_at: 1_000,
            project_dir: self.root().to_path_buf(),
            start_head: Some(self.repo.head()),
            turn,
            turn_open: open,
            ended_at: None,
        };
        self.dir.write_meta(&meta).unwrap();
    }

    /// pre-edit: 지금 내용을 원본으로 (없으면 new)
    fn pre_edit(&self, rel: &str) {
        let abs = self.abs(rel);
        self.dir.ensure_baseline(&abs, BaselineContent::CopyOf(&abs)).unwrap();
    }

    fn event(&self, turn: u32, paths: Vec<PathBuf>) {
        let ev = Event { t: 1, turn, kind: EventKind::Edit, tool: Some("Edit".into()), paths, agent_id: None };
        self.dir.append_event(&ev).unwrap();
    }

    /// pre-edit → 내용 쓰기 → post-edit
    fn edit(&self, turn: u32, rel: &str, contents: &str) {
        self.pre_edit(rel);
        self.repo.write(rel, contents);
        self.event(turn, vec![self.abs(rel)]);
    }

    /// turn-end: 그 turn에 건드린 경로를 스냅샷하고 turn을 닫는다
    fn close_turn(&self, turn: u32, rels: &[&str]) {
        let paths: Vec<PathBuf> = rels.iter().map(|r| self.abs(r)).collect();
        self.dir.snapshot_turn(turn, &paths).unwrap();
        self.set_turn(turn, false);
    }

    fn changes(&self, view: SessionView) -> Vec<FileEntry> {
        changes(&self.dir, view, &self.wt).unwrap()
    }

    fn paths(&self, view: SessionView) -> Vec<String> {
        self.changes(view).into_iter().map(|e| e.file.path).collect()
    }

    fn diff(&self, view: SessionView, rel: &str) -> String {
        match patch(&self.dir, view, &self.wt, rel).unwrap().body {
            PatchBody::Text { diff } => diff,
            other => panic!("text가 아님: {other:?}"),
        }
    }
}

fn set_mtime(path: &Path, ms: u64) {
    let f = fs::File::options().write(true).open(path).unwrap();
    f.set_modified(UNIX_EPOCH + Duration::from_millis(ms)).unwrap();
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
}

/// data dir 아래 모든 파일의 (경로, 크기, mtime)
fn tree(dir: &Path) -> Vec<(PathBuf, u64, std::time::SystemTime)> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let meta = fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            out.extend(tree(&path));
        }
        out.push((path, meta.len(), meta.modified().unwrap()));
    }
    out.sort();
    out
}

#[test]
fn session_view_modified_added_deleted() {
    let fx = setup();
    fx.edit(0, "src/a.py", "a = 2\n");
    fx.edit(0, "new.py", "n = 1\nn = 2\n");
    fx.pre_edit("src/b.py");
    fx.repo.remove("src/b.py");
    fx.event(0, vec![fx.abs("src/b.py")]);

    let got: Vec<_> = fx
        .changes(SessionView::Session)
        .into_iter()
        .map(|e| (e.file.path, e.file.status, e.file.additions, e.file.deletions, e.uncertain))
        .collect();
    assert_eq!(
        got,
        vec![
            ("new.py".into(), FileStatus::Added, Some(2), Some(0), false),
            ("src/a.py".into(), FileStatus::Modified, Some(1), Some(1), false),
            ("src/b.py".into(), FileStatus::Deleted, Some(0), Some(1), false),
        ]
    );
    let diff = fx.diff(SessionView::Session, "src/b.py");
    assert!(diff.contains("deleted file mode") && diff.contains("-b = 1"), "{diff}");
}

#[test]
fn session_view_drops_reverted_files() {
    let fx = setup();
    fx.edit(0, "src/a.py", "a = 2\n");
    fx.repo.write("src/a.py", "a = 1\n");
    // 만들었다가 지운 파일도 빠진다
    fx.edit(0, "tmp.py", "x\n");
    fx.repo.remove("tmp.py");
    assert!(fx.paths(SessionView::Session).is_empty());
}

#[test]
fn session_view_survives_commit() {
    let fx = setup();
    fx.edit(0, "src/a.py", "a = 2\n");
    fx.repo.commit_all("wip");
    assert_eq!(fx.paths(SessionView::Session), vec!["src/a.py"]);
    let diff = fx.diff(SessionView::Session, "src/a.py");
    assert!(diff.contains("-a = 1") && diff.contains("+a = 2"), "{diff}");
}

#[test]
fn session_view_uncertain_uses_start_head() {
    let fx = setup();
    let abs = fx.abs("src/a.py");
    let content = BaselineContent::Uncertain { head: Some(fx.repo.head()), bytes: Some(b"a = 1\n") };
    fx.dir.ensure_baseline(&abs, content).unwrap();
    fx.repo.write("src/a.py", "a = 3\n");
    fx.event(0, vec![abs]);

    let entries = fx.changes(SessionView::Session);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].uncertain);
    assert_eq!(entries[0].file.status, FileStatus::Modified);
    let diff = fx.diff(SessionView::Session, "src/a.py");
    assert!(diff.contains("-a = 1") && diff.contains("+a = 3"), "{diff}");
}

#[test]
fn missing_baseline_is_uncertain_without_before() {
    let fx = setup();
    // hook이 늦게 설치돼 원본이 없는 경로
    fx.repo.write("src/a.py", "a = 2\n");
    fx.event(0, vec![fx.abs("src/a.py")]);
    let entries = fx.changes(SessionView::Session);
    assert_eq!(entries.len(), 1);
    assert!(entries[0].uncertain);
    assert_eq!(entries[0].file.status, FileStatus::Added);
}

#[test]
fn session_view_only_this_worktree() {
    let fx = setup();
    let other = tempfile::tempdir().unwrap();
    let outside = other.path().join("x.py");
    fs::write(&outside, "x\n").unwrap();
    fx.dir.ensure_baseline(&outside, BaselineContent::New).unwrap();
    fx.edit(0, "src/a.py", "a = 2\n");
    fx.event(0, vec![outside]);
    assert_eq!(fx.paths(SessionView::Session), vec!["src/a.py"]);
}

#[test]
fn symlinked_event_path_matches_worktree() {
    let fx = setup();
    // tempdir은 `/var/...`이고 worktree root는 canonical한 `/private/var/...`
    let abs = fx.repo.path().join("src/a.py");
    fx.dir.ensure_baseline(&abs, BaselineContent::CopyOf(&abs)).unwrap();
    fx.repo.write("src/a.py", "a = 2\n");
    fx.event(0, vec![abs]);
    assert_eq!(fx.paths(SessionView::Session), vec!["src/a.py"]);
    assert!(!fx.changes(SessionView::Session)[0].uncertain);
    let roots = vec![fx.root().to_path_buf()];
    assert_eq!(worktrees_of(&fx.dir, &roots), roots);
}

#[test]
fn turn_view_closed_turn() {
    let fx = setup();
    fx.set_turn(1, true);
    fx.edit(1, "src/a.py", "a = 2\n");
    fx.close_turn(1, &["src/a.py"]);
    fx.set_turn(2, true);
    fx.edit(2, "src/a.py", "a = 3\n");
    fx.edit(2, "src/b.py", "b = 2\n");
    fx.close_turn(2, &["src/a.py", "src/b.py"]);
    // 닫힌 turn은 현재 파일이 아니라 turn 끝 스냅샷과 비교한다
    fx.repo.write("src/a.py", "a = 9\n");

    assert_eq!(fx.paths(SessionView::Turn), vec!["src/a.py", "src/b.py"]);
    let diff = fx.diff(SessionView::Turn, "src/a.py");
    assert!(diff.contains("-a = 2") && diff.contains("+a = 3"), "{diff}");
    let s = &sessions_for(fx.data.path(), fx.root(), now())[0];
    assert_eq!((s.turn, s.turn_open), (2, false));
}

#[test]
fn turn_view_open_turn_compares_current() {
    let fx = setup();
    fx.set_turn(1, true);
    fx.edit(1, "src/a.py", "a = 2\n");
    fx.close_turn(1, &["src/a.py"]);
    fx.set_turn(2, true);
    fx.edit(2, "src/a.py", "a = 3\n");
    fx.close_turn(2, &["src/a.py"]);
    fx.set_turn(3, true);
    fx.edit(3, "src/a.py", "a = 4\n");

    assert_eq!(fx.paths(SessionView::Turn), vec!["src/a.py"]);
    let diff = fx.diff(SessionView::Turn, "src/a.py");
    assert!(diff.contains("-a = 3") && diff.contains("+a = 4"), "{diff}");
    let s = &sessions_for(fx.data.path(), fx.root(), now())[0];
    assert_eq!((s.turn, s.turn_open), (3, true));
}

#[test]
fn turn_view_open_turn_without_events_shows_last_closed() {
    let fx = setup();
    fx.set_turn(1, true);
    fx.edit(1, "src/a.py", "a = 2\n");
    fx.close_turn(1, &["src/a.py"]);
    fx.set_turn(2, true);

    assert_eq!(fx.paths(SessionView::Turn), vec!["src/a.py"]);
    let diff = fx.diff(SessionView::Turn, "src/a.py");
    assert!(diff.contains("-a = 1") && diff.contains("+a = 2"), "{diff}");
    // 탭 label은 화면이 보이는 turn을 따른다
    let s = &sessions_for(fx.data.path(), fx.root(), now())[0];
    assert_eq!((s.turn, s.turn_open), (1, false));
}

#[test]
fn turn_view_before_falls_back_to_baseline() {
    let fx = setup();
    fx.set_turn(1, true);
    fx.edit(1, "src/a.py", "a = 2\n");
    fx.close_turn(1, &["src/a.py"]);
    fx.set_turn(2, true);
    fx.edit(2, "src/b.py", "b = 2\n");
    fx.close_turn(2, &["src/b.py"]);

    assert_eq!(fx.paths(SessionView::Turn), vec!["src/b.py"]);
    let diff = fx.diff(SessionView::Turn, "src/b.py");
    assert!(diff.contains("-b = 1") && diff.contains("+b = 2"), "{diff}");
}

#[test]
fn turn_zero_merges_into_turn_one() {
    let fx = setup();
    fx.edit(0, "src/a.py", "a = 2\n");
    fx.set_turn(1, true);
    fx.edit(1, "src/b.py", "b = 2\n");
    assert_eq!(fx.paths(SessionView::Turn), vec!["src/a.py", "src/b.py"]);
    // hook은 turn 1을 닫을 때 turn 0 경로도 turns/1에 찍는다
    fx.close_turn(1, &["src/a.py", "src/b.py"]);
    assert_eq!(fx.paths(SessionView::Turn), vec!["src/a.py", "src/b.py"]);
}

#[test]
fn patch_headers_use_worktree_relative_path() {
    let fx = setup();
    fx.set_turn(1, true);
    fx.edit(1, "src/a.py", "a = 2\n");
    fx.close_turn(1, &["src/a.py"]);
    let before = tree(fx.data.path());

    for view in [SessionView::Session, SessionView::Turn] {
        let p = patch(&fx.dir, view, &fx.wt, "src/a.py").unwrap();
        assert_eq!(p.path, "src/a.py");
        let PatchBody::Text { diff } = p.body else { panic!("text가 아님") };
        assert!(diff.starts_with("diff --git a/src/a.py b/src/a.py\n"), "{diff}");
        assert!(diff.contains("\n--- a/src/a.py\n+++ b/src/a.py\n"), "{diff}");
        let data = fx.data.path().to_string_lossy().trim_start_matches('/').to_string();
        assert!(!diff.contains("sessions/") && !diff.contains(&data), "{diff}");
        assert_eq!(p.hash, blake3::hash(diff.as_bytes()).to_hex().to_string());
    }
    // 앱은 세션 폴더에 쓰지 않는다
    assert_eq!(tree(fx.data.path()), before);
}

#[test]
fn patch_unknown_file_errors() {
    let fx = setup();
    fx.edit(0, "src/a.py", "a = 2\n");
    fx.edit(0, "src/b.py", "b = 1\n"); // 내용 그대로
    for rel in ["nope.py", "src/b.py"] {
        let err = patch(&fx.dir, SessionView::Session, &fx.wt, rel).unwrap_err();
        assert!(matches!(err, WorktreeError::UnknownFile(ref p) if p == rel), "{err}");
    }
}

#[test]
fn too_large_side_is_too_large_body() {
    let fx = setup();
    let big = "x".repeat(MAX_DIFF_BYTES as usize + 10);
    fx.edit(0, "src/a.py", &big);
    // 원본도 현재도 1MB 초과: 크기·hash로 비교한다
    fx.repo.write("big.bin", &big);
    fx.edit(0, "big.bin", &format!("{big}y"));
    fx.repo.write("same.bin", &big);
    fx.edit(0, "same.bin", &big);

    let got: Vec<_> = fx
        .changes(SessionView::Session)
        .into_iter()
        .map(|e| (e.file.path, e.file.status, e.file.additions))
        .collect();
    assert_eq!(
        got,
        vec![("big.bin".into(), FileStatus::Modified, None), ("src/a.py".into(), FileStatus::Modified, None)]
    );
    let p = patch(&fx.dir, SessionView::Session, &fx.wt, "src/a.py").unwrap();
    assert_eq!(p.body, PatchBody::TooLarge { old_size: Some(6), new_size: Some(big.len() as u64) });
    let p = patch(&fx.dir, SessionView::Session, &fx.wt, "big.bin").unwrap();
    let n = big.len() as u64;
    assert_eq!(p.body, PatchBody::TooLarge { old_size: Some(n), new_size: Some(n + 1) });
}

#[test]
fn nfd_path_matches_worktree_path() {
    let fx = setup();
    let nfd = "\u{1112}\u{1161}\u{11AB}\u{1100}\u{1173}\u{11AF}.txt"; // 한글.txt
    let abs = fx.root().join(nfd);
    fx.dir.ensure_baseline(&abs, BaselineContent::New).unwrap();
    fx.repo.write("한글.txt", "가\n");
    fx.event(0, vec![abs]);

    let entries = fx.changes(SessionView::Session);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].file.path, "한글.txt");
    assert_eq!(entries[0].file.status, FileStatus::Added);
    let diff = fx.diff(SessionView::Session, "한글.txt");
    assert!(diff.contains("+가"), "{diff}");
}

#[test]
fn sessions_for_orders_and_attributes() {
    let fx = setup();
    let other = tempfile::tempdir().unwrap();
    let t = now();

    // s1: project_dir만 이 worktree (기록된 경로는 symlink를 거친 /var/...), event 없음
    let s1 = SessionDir::open(fx.data.path(), "s1").unwrap();
    let mut meta = s1.read_meta().unwrap();
    meta.project_dir = fx.repo.path().to_path_buf();
    s1.write_meta(&meta).unwrap();
    s1.append_event(&Event { t: 1, turn: 0, kind: EventKind::TurnStart, tool: None, paths: vec![], agent_id: None })
        .unwrap();
    set_mtime(&s1.root().join("events.jsonl"), t - 10 * MIN);

    // s2: project_dir은 다른 곳, 이 worktree 파일 2개 + 밖 파일 1개
    let s2 = SessionDir::open(fx.data.path(), "s2").unwrap();
    s2.write_meta(&Meta { project_dir: other.path().to_path_buf(), ..meta.clone() }).unwrap();
    let paths = vec![fx.abs("src/a.py"), fx.abs("src/b.py"), other.path().join("x.py"), fx.abs("src/a.py")];
    s2.append_event(&Event { t: 2, turn: 1, kind: EventKind::Edit, tool: None, paths, agent_id: None }).unwrap();
    set_mtime(&s2.root().join("events.jsonl"), t - MIN);

    // s3: 이 worktree와 무관
    let s3 = SessionDir::open(fx.data.path(), "s3").unwrap();
    s3.write_meta(&Meta { project_dir: other.path().to_path_buf(), ..meta.clone() }).unwrap();
    let paths = vec![other.path().join("y.py")];
    s3.append_event(&Event { t: 3, turn: 1, kind: EventKind::Edit, tool: None, paths, agent_id: None }).unwrap();

    let got: Vec<_> = sessions_for(fx.data.path(), fx.root(), t)
        .into_iter()
        .map(|s| (s.id, s.touched_files, s.last_event_at))
        .collect();
    assert_eq!(got, vec![("s2".into(), 2, t - MIN), ("s1".into(), 0, t - 10 * MIN)]);
    assert_eq!(sessions_for(fx.data.path(), fx.root(), t)[0].started_at, 1_000);

    let roots = vec![other.path().to_path_buf(), fx.root().to_path_buf()];
    let other_root = other.path().to_path_buf();
    assert_eq!(worktrees_of(&s2, &roots), roots);
    assert_eq!(worktrees_of(&s1, &roots), vec![fx.root().to_path_buf()]);
    assert_eq!(worktrees_of(&s3, &roots), vec![other_root]);

    let json = serde_json::to_value(&sessions_for(fx.data.path(), fx.root(), t)[0]).unwrap();
    for key in ["id", "startedAt", "lastEventAt", "active", "turn", "turnOpen", "touchedFiles"] {
        assert!(json.get(key).is_some(), "{key}: {json}");
    }
}

#[test]
fn active_rules() {
    let fx = setup();
    let t = now();
    let make = |id: &str, ended: bool, last: u64| {
        let dir = SessionDir::open(fx.data.path(), id).unwrap();
        let meta = Meta {
            started_at: 1_000,
            project_dir: fx.root().to_path_buf(),
            start_head: None,
            turn: 1,
            turn_open: !ended,
            ended_at: ended.then_some(last),
        };
        dir.write_meta(&meta).unwrap();
        dir.append_event(&Event { t: 1, turn: 1, kind: EventKind::TurnStart, tool: None, paths: vec![], agent_id: None })
            .unwrap();
        set_mtime(&dir.root().join("events.jsonl"), last);
    };
    fs::remove_dir_all(fx.dir.root()).unwrap();
    make("ended", true, t - MIN);
    make("stale", false, t - 31 * MIN);

    let actives: Vec<_> = sessions_for(fx.data.path(), fx.root(), t).into_iter().map(|s| s.active).collect();
    assert_eq!(actives, vec![false, false]);
    let roots = vec![fx.root().to_path_buf()];
    assert!(active_roots(fx.data.path(), &roots, t).is_empty());

    make("live", false, t - 1_000);
    let got: Vec<_> =
        sessions_for(fx.data.path(), fx.root(), t).into_iter().map(|s| (s.id, s.active)).collect();
    assert_eq!(got, vec![("live".into(), true), ("ended".into(), false), ("stale".into(), false)]);
    assert_eq!(active_roots(fx.data.path(), &roots, t), roots.into_iter().collect());
    assert_eq!(ACTIVE_WINDOW_MS, 30 * MIN);
}

#[test]
fn cleanup_removes_old_sessions_only() {
    let fx = setup();
    let t = now();
    let old = SessionDir::open(fx.data.path(), "old").unwrap();
    old.write_meta(&fx.dir.read_meta().unwrap()).unwrap();
    old.append_event(&Event { t: 1, turn: 0, kind: EventKind::TurnStart, tool: None, paths: vec![], agent_id: None })
        .unwrap();
    set_mtime(&old.root().join("events.jsonl"), t - 8 * DAY);
    fx.event(0, vec![fx.abs("src/a.py")]);
    // 세션 폴더가 아닌 것은 건드리지 않는다
    fs::write(fx.data.path().join("sessions/not-a-dir"), "x").unwrap();

    assert_eq!(cleanup(fx.data.path(), 7, t), 1);
    assert!(!old.root().exists());
    assert!(fx.dir.root().exists());
    assert!(fx.data.path().join("sessions/not-a-dir").exists());
    assert_eq!(cleanup(fx.data.path(), 7, t), 0);
}
