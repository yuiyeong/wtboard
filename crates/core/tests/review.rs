use std::sync::{Arc, Barrier, Mutex};

use rusqlite::params;
use tempfile::tempdir;
use wtboard_core::review::{attach, render, system_message, ATTACH_CAP};
use wtboard_core::store::comments::{Comment, CommentStore, NewComment, Side};
use wtboard_core::store::{db, StoreError};

const CLOSE: &str = "</wtboard-review-comments>";

fn comment(id: i64, path: &str, side: Side, line: u32, snippet: &str, body: &str) -> Comment {
    Comment {
        id,
        worktree: "/w".into(),
        path: path.into(),
        side,
        line,
        view: "branch".into(),
        snippet: snippet.into(),
        body: body.into(),
        created_at: 1,
        updated_at: 1,
        sent_at: None,
        sent_to: None,
    }
}

fn long(id: i64, body: String) -> Comment {
    comment(id, "a.rs", Side::New, id as u32, "x", &body)
}

fn new_comment(line: u32, body: String) -> NewComment {
    NewComment {
        worktree: "/w".into(),
        path: "a.rs".into(),
        side: Side::New,
        line,
        view: "branch".into(),
        snippet: "x".into(),
        body,
    }
}

#[test]
fn renders_new_and_old_side() {
    let list = [
        comment(1, "app/api/users.py", Side::New, 42, "    return db.query(User).all()", "페이지네이션이 필요합니다."),
        comment(2, "app/api/deps.py", Side::Old, 17, "from x import y", "이 import는 다른 곳에서 쓰고 있어요."),
    ];
    let r = render("/Users/jo/projects/my-api", &list, None);
    let expected = "\
<wtboard-review-comments worktree=\"/Users/jo/projects/my-api\">
사용자가 wtboard에서 diff를 보며 남긴 리뷰 코멘트입니다. 각 코멘트가 가리키는 코드를 확인하고 반영해 주세요.

1. app/api/users.py:42
```
    return db.query(User).all()
```
페이지네이션이 필요합니다.

2. app/api/deps.py (삭제된 줄, 원래 17번째 줄)
```
from x import y
```
이 import는 다른 곳에서 쓰고 있어요.
</wtboard-review-comments>";
    assert_eq!(r.text, expected);
    assert_eq!(r.included, vec![1, 2]);
    assert_eq!(r.remaining, 0);
}

#[test]
fn empty_input_renders_nothing() {
    let r = render("/w", &[], Some(ATTACH_CAP));
    assert_eq!(r.text, "");
    assert!(r.included.is_empty());
    assert_eq!(r.remaining, 0);
}

#[test]
fn fence_grows_past_backticks() {
    let list = [comment(1, "a.rs", Side::New, 1, "let s = \"```\";", "b")];
    let r = render("/w", &list, None);
    assert!(r.text.contains("1. a.rs:1\n````\nlet s = \"```\";\n````\nb\n"), "{}", r.text);
}

#[test]
fn closing_tag_in_body_is_escaped() {
    let list = [comment(1, "a.rs", Side::New, 1, &format!("s {CLOSE}"), &format!("앞 {CLOSE} 뒤"))];
    let r = render("/w", &list, None);
    assert_eq!(r.text.matches(CLOSE).count(), 1);
    assert!(r.text.ends_with(CLOSE));
    assert!(r.text.contains("앞 <\\/wtboard-review-comments> 뒤"));
}

#[test]
fn cap_stops_and_counts_remaining() {
    let list: Vec<_> = (1..=3).map(|i| long(i, "가".repeat(4000))).collect();
    let r = render("/w", &list, Some(ATTACH_CAP));
    assert_eq!(r.included, vec![1, 2]);
    assert_eq!(r.remaining, 1);
    assert!(r.text.contains("(코멘트 1개가 더 있습니다. 다음 메시지에 붙습니다)"));
    assert!(r.text.ends_with(CLOSE));
    assert!(r.text.chars().count() <= ATTACH_CAP);
}

#[test]
fn cap_counts_chars_not_bytes() {
    let list: Vec<_> = (1..=3).map(|i| long(i, "가".repeat(2900))).collect();
    let r = render("/w", &list, Some(ATTACH_CAP));
    assert_eq!(r.included, vec![1, 2, 3]);
    assert_eq!(r.remaining, 0);
    assert!(r.text.chars().count() <= ATTACH_CAP);
    assert!(r.text.len() > ATTACH_CAP);
}

#[test]
fn no_cap_includes_all() {
    let list: Vec<_> = (1..=3).map(|i| long(i, "가".repeat(4000))).collect();
    let r = render("/w", &list, None);
    assert_eq!(r.included, vec![1, 2, 3]);
    assert_eq!(r.remaining, 0);
    assert!(!r.text.contains("더 있습니다"));
}

#[test]
fn oversized_first_comment_is_still_included() {
    let list = [long(1, "가".repeat(10_000))];
    let r = render("/w", &list, Some(ATTACH_CAP));
    assert_eq!(r.included, vec![1]);
}

#[test]
fn system_message_text() {
    assert_eq!(system_message(3, 0), "wtboard: 리뷰 코멘트 3개를 첨부했습니다");
    assert_eq!(system_message(2, 1), "wtboard: 리뷰 코멘트 2개를 첨부했습니다 (1개 남음)");
}

fn sent_to_of(path: &std::path::Path) -> Vec<Option<String>> {
    let store = CommentStore::open(path).unwrap();
    let mut list = store.list("/w").unwrap();
    list.sort_by_key(|c| c.id);
    list.into_iter().map(|c| c.sent_to).collect()
}

fn three_long(path: &std::path::Path) -> Vec<i64> {
    let store = CommentStore::open(path).unwrap();
    (1..=3).map(|i| store.add(&new_comment(i, "가".repeat(4000))).unwrap().id).collect()
}

#[test]
fn attach_marks_only_included() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let ids = three_long(&path);
    let mut conn = db::open_state(&path).unwrap();
    let mut emitted = Vec::new();
    let r = attach(&mut conn, "/w", "s1", 500, |r| {
        emitted = r.included.clone();
        Ok(())
    })
    .unwrap()
    .unwrap();
    assert_eq!(r.included, ids[..2]);
    assert_eq!(emitted, ids[..2]);
    assert_eq!(r.remaining, 1);
    assert_eq!(sent_to_of(&path), vec![Some("s1".into()), Some("s1".into()), None]);
    let sent_at: Vec<Option<i64>> = conn
        .prepare("SELECT sent_at FROM comments ORDER BY id")
        .unwrap()
        .query_map(params![], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(sent_at, vec![Some(500), Some(500), None]);
}

#[test]
fn attach_emit_error_keeps_unsent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    three_long(&path);
    let mut conn = db::open_state(&path).unwrap();
    let err = attach(&mut conn, "/w", "s1", 500, |_| Err(std::io::Error::other("broken pipe"))).unwrap_err();
    assert!(matches!(err, StoreError::Emit(_)), "{err:?}");
    assert_eq!(sent_to_of(&path), vec![None, None, None]);
}

#[test]
fn attach_without_comments_skips_emit() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let mut conn = db::open_state(&path).unwrap();
    let r = attach(&mut conn, "/w", "s1", 500, |_| panic!("emit이 불리면 안 됨")).unwrap();
    assert!(r.is_none());
}

#[test]
fn concurrent_attach_claims_once() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.sqlite");
    let ids = three_long(&path);
    let emitted = Arc::new(Mutex::new(Vec::<Vec<i64>>::new()));
    let barrier = Arc::new(Barrier::new(2));
    let handles: Vec<_> = ["s1", "s2"]
        .into_iter()
        .map(|session| {
            let (path, emitted, barrier) = (path.clone(), emitted.clone(), barrier.clone());
            std::thread::spawn(move || {
                let mut conn = db::open_state(&path).unwrap();
                barrier.wait();
                attach(&mut conn, "/w", session, 500, |r| {
                    emitted.lock().unwrap().push(r.included.clone());
                    Ok(())
                })
                .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let mut all: Vec<i64> = emitted.lock().unwrap().iter().flatten().copied().collect();
    all.sort();
    assert_eq!(all, ids, "합집합 = 전체, 교집합 = 없음");
    let sent = sent_to_of(&path);
    assert!(sent.iter().all(|s| s.is_some()));
}
