mod common;

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use common::feat_repo;
use wtboard_core::review::reply_turns;
use wtboard_core::session::list::touched_turns;
use wtboard_core::session::{Event, EventKind, SessionDir};
use wtboard_core::store::comments::{Comment, Side};
use wtboard_core::worktree::Worktree;

/// feat 레포 worktree + data dir + 세션 `s1`, `s2`
struct Fx {
    _repo: common::TestRepo,
    data: tempfile::TempDir,
    root: PathBuf,
}

fn setup() -> Fx {
    let repo = feat_repo();
    let root = Worktree::open(repo.path(), Some("main".into())).unwrap().layout().root.clone();
    let data = tempfile::tempdir().unwrap();
    let fx = Fx { _repo: repo, data, root };
    // s1: turn 0 Edit a, turn 2 Bash b, turn 3 Edit a, turn 3 Edit (worktree 밖)
    let outside = fx.data.path().join("elsewhere/c.py");
    fx.event("s1", 0, EventKind::Edit, fx.abs("src/a.py"));
    fx.event("s1", 2, EventKind::Bash, fx.abs("src/b.py"));
    fx.event("s1", 3, EventKind::Edit, fx.abs("src/a.py"));
    fx.event("s1", 3, EventKind::Edit, outside);
    // s2: turn 5 Edit a
    fx.event("s2", 5, EventKind::Edit, fx.abs("src/a.py"));
    fx
}

impl Fx {
    fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    fn session(&self, id: &str) -> SessionDir {
        SessionDir::open(self.data.path(), id).unwrap()
    }

    fn event(&self, session: &str, turn: u32, kind: EventKind, path: PathBuf) {
        let ev = Event { t: 1, turn, kind, tool: None, paths: vec![path], agent_id: None };
        self.session(session).append_event(&ev).unwrap();
    }

    fn roots(&self) -> Vec<PathBuf> {
        vec![self.root.clone()]
    }

    fn replies(&self, comments: &[Comment], roots: &[PathBuf]) -> HashMap<i64, u32> {
        reply_turns(self.data.path(), &self.root, roots, comments)
    }
}

fn sent(id: i64, path: &str, sent_to: Option<&str>, sent_turn: Option<u32>) -> Comment {
    Comment {
        id,
        worktree: "/w".into(),
        path: path.into(),
        side: Side::New,
        line: 1,
        view: "branch".into(),
        snippet: "x".into(),
        body: "b".into(),
        created_at: 1,
        updated_at: 1,
        sent_at: sent_to.map(|_| 100),
        sent_to: sent_to.map(str::to_owned),
        sent_turn,
        resolved_at: None,
    }
}

fn turns(set: &[u32]) -> BTreeSet<u32> {
    set.iter().copied().collect()
}

#[test]
fn touched_turns_groups_by_path_and_maps_turn_zero_to_one() {
    let fx = setup();
    let got = touched_turns(&fx.session("s1"), &fx.root, &fx.roots());
    let expected: HashMap<String, BTreeSet<u32>> =
        HashMap::from([("src/a.py".into(), turns(&[1, 3])), ("src/b.py".into(), turns(&[2]))]);
    assert_eq!(got, expected);
}

#[test]
fn reply_turn_is_latest_turn_at_or_after_sent_turn() {
    let fx = setup();
    let comments = [
        sent(1, "src/a.py", Some("s1"), Some(1)),
        sent(2, "src/a.py", Some("s1"), Some(3)),
        sent(3, "src/a.py", Some("s1"), Some(4)),
        sent(4, "src/b.py", Some("s1"), Some(1)),
        sent(5, "src/b.py", Some("s1"), Some(3)),
        sent(6, "src/a.py", Some("s2"), Some(1)),
    ];
    let got = fx.replies(&comments, &fx.roots());
    assert_eq!(got, HashMap::from([(1, 3), (2, 3), (4, 2), (6, 5)]));
}

#[test]
fn reply_turns_missing_or_invalid_session_is_none() {
    let fx = setup();
    let comments = [
        sent(1, "src/a.py", Some("copy"), None),
        sent(2, "src/a.py", None, None),
        sent(3, "src/a.py", Some("s-none"), Some(1)),
        sent(4, "src/a.py", Some("../x"), Some(1)),
        sent(5, "src/a.py", Some("s1"), None),
    ];
    assert!(fx.replies(&comments, &fx.roots()).is_empty());
}

#[test]
fn reply_turns_only_this_worktree() {
    let fx = setup();
    // 안쪽 worktree가 가진 경로의 event는 바깥 worktree 코멘트에 잡히지 않는다
    let inner = fx.root.join(".claude/worktrees/x");
    fx.event("s3", 2, EventKind::Edit, inner.join("src/a.py"));
    let roots = vec![fx.root.clone(), inner];
    let comments = [sent(1, "src/a.py", Some("s3"), Some(1)), sent(2, ".claude/worktrees/x/src/a.py", Some("s3"), Some(1))];
    assert!(fx.replies(&comments, &roots).is_empty());
    // worktree 밖 경로(c.py)는 touched에 없다
    assert!(!touched_turns(&fx.session("s1"), &fx.root, &roots).contains_key("elsewhere/c.py"));
    let _ = Path::new("");
}
