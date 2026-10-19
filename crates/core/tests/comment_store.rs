use wtboard_core::store::comments::{Comment, CommentStore, NewComment, Side};
use wtboard_core::store::settings::SettingsStore;
use wtboard_core::store::StoreError;

fn new(worktree: &str, path: &str, line: u32) -> NewComment {
    NewComment {
        worktree: worktree.into(),
        path: path.into(),
        side: Side::New,
        line,
        view: "branch".into(),
        snippet: "let x = 1;".into(),
        body: "고쳐 주세요".into(),
    }
}

fn locations(list: &[Comment]) -> Vec<(String, u32, bool)> {
    list.iter().map(|c| (c.path.clone(), c.line, c.sent_at.is_some())).collect()
}

#[test]
fn add_and_list_order() {
    let s = CommentStore::in_memory();
    s.add(&new("w", "b.rs", 3)).unwrap();
    s.add(&new("w", "a.rs", 9)).unwrap();
    let a2 = s.add(&new("w", "a.rs", 2)).unwrap();
    assert_eq!(s.mark_sent(&[a2.id], "s1", 100).unwrap(), 1);
    let list = s.list("w").unwrap();
    assert_eq!(
        locations(&list),
        vec![("a.rs".into(), 9, false), ("b.rs".into(), 3, false), ("a.rs".into(), 2, true)]
    );
    assert_eq!(list[2].sent_to.as_deref(), Some("s1"));
    assert!(s.list("other").unwrap().is_empty());
}

#[test]
fn body_rules() {
    let s = CommentStore::in_memory();
    let mut c = new("w", "a.rs", 1);
    c.body = "  ".into();
    assert!(matches!(s.add(&c), Err(StoreError::InvalidBody(_))));
    c.body = "가".repeat(4001);
    assert!(matches!(s.add(&c), Err(StoreError::InvalidBody(_))));
    c.body = "가".repeat(4000);
    assert!(s.add(&c).is_ok());

    let mut c = new("w", "a.rs", 2);
    c.snippet = "x".repeat(600);
    assert_eq!(s.add(&c).unwrap().snippet.chars().count(), 500);

    let added = s.add(&new("w", "a.rs", 3)).unwrap();
    assert!(matches!(s.update_body(added.id, "\n"), Err(StoreError::InvalidBody(_))));
}

#[test]
fn update_only_unsent() {
    let s = CommentStore::in_memory();
    let c = s.add(&new("w", "a.rs", 1)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    let updated = s.update_body(c.id, "바뀐 내용").unwrap();
    assert_eq!(updated.body, "바뀐 내용");
    assert!(updated.updated_at > c.updated_at);
    assert_eq!(updated.created_at, c.created_at);

    s.mark_sent(&[c.id], "s1", 100).unwrap();
    assert!(matches!(s.update_body(c.id, "또"), Err(StoreError::AlreadySent)));
    assert!(matches!(s.update_body(999, "또"), Err(StoreError::NotFound(999))));
}

#[test]
fn delete_is_idempotent() {
    let s = CommentStore::in_memory();
    let c = s.add(&new("w", "a.rs", 1)).unwrap();
    s.delete(c.id).unwrap();
    assert!(s.list("w").unwrap().is_empty());
    s.delete(c.id).unwrap();
}

#[test]
fn mark_sent_skips_already_sent() {
    let s = CommentStore::in_memory();
    let c = s.add(&new("w", "a.rs", 1)).unwrap();
    assert_eq!(s.mark_sent(&[c.id], "s1", 100).unwrap(), 1);
    assert_eq!(s.mark_sent(&[c.id], "copy", 200).unwrap(), 0);
    let got = &s.list("w").unwrap()[0];
    assert_eq!(got.sent_to.as_deref(), Some("s1"));
    assert_eq!(got.sent_at, Some(100));
}

#[test]
fn clear_sent_and_counts() {
    let s = CommentStore::in_memory();
    let a1 = s.add(&new("A", "a.rs", 1)).unwrap();
    s.add(&new("A", "a.rs", 2)).unwrap();
    s.add(&new("A", "a.rs", 3)).unwrap();
    s.add(&new("B", "b.rs", 1)).unwrap();
    s.mark_sent(&[a1.id], "s1", 100).unwrap();
    let counts = s.unsent_counts().unwrap();
    assert_eq!(counts.len(), 2);
    assert_eq!(counts["A"], 2);
    assert_eq!(counts["B"], 1);
    assert_eq!(s.clear_sent("A").unwrap(), 1);
    assert_eq!(s.list("A").unwrap().len(), 2);
    assert_eq!(s.list("B").unwrap().len(), 1);
}

#[test]
fn purge_sent_before_cutoff() {
    let s = CommentStore::in_memory();
    let c1 = s.add(&new("w", "a.rs", 1)).unwrap();
    let c2 = s.add(&new("w", "a.rs", 2)).unwrap();
    s.add(&new("w", "a.rs", 3)).unwrap();
    s.mark_sent(&[c1.id], "s1", 100).unwrap();
    s.mark_sent(&[c2.id], "s1", 200).unwrap();
    assert_eq!(s.purge_sent_before(150).unwrap(), 1);
    assert_eq!(s.list("w").unwrap().len(), 2);
}

#[test]
fn file_store_shares_state_with_second_connection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let a = CommentStore::open(&path).unwrap();
    let b = CommentStore::open(&path).unwrap();
    a.add(&new("w", "a.rs", 1)).unwrap();
    assert_eq!(b.list("w").unwrap().len(), 1);
}

#[test]
fn settings_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let s = SettingsStore::open(&dir.path().join("state.sqlite")).unwrap();
    assert_eq!(s.get("always_on_top").unwrap(), None);
    s.set("always_on_top", "1").unwrap();
    assert_eq!(s.get("always_on_top").unwrap().as_deref(), Some("1"));
    s.set("always_on_top", "0").unwrap();
    assert_eq!(s.get("always_on_top").unwrap().as_deref(), Some("0"));
    assert_eq!(SettingsStore::in_memory().get("x").unwrap(), None);
}
