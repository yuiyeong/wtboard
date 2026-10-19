//! 리뷰 코멘트 command. 본문은 `CommentStore`를 받는 순수 함수이고, command는 store가 열리길 기다려 그것을 부른다
use serde::Serialize;
use tauri::{AppHandle, Manager};
use wtboard_core::review::render;
use wtboard_core::store::comments::{Comment, CommentStore, NewComment, Side};
use wtboard_core::store::StoreError;
use wtboard_core::worktree::now_ms;

use super::{blocking, registry, AppState};

/// "마크다운으로 복사"에 쓰는 텍스트와 그 안에 든 코멘트 id. 복사가 끝나면 UI가 `mark_comments_sent`로 id를 돌려준다
#[derive(Serialize)]
pub struct CopyPayload {
    markdown: String,
    ids: Vec<i64>,
}

fn msg(e: StoreError) -> String {
    match e {
        StoreError::AlreadySent => "이미 전송된 코멘트는 고칠 수 없습니다".into(),
        e => e.to_string(),
    }
}

fn list(store: &CommentStore, worktree: &str) -> Result<Vec<Comment>, String> {
    store.list(worktree).map_err(msg)
}

/// `view`는 화면의 view 이름만 받는다 (session id는 코멘트에 필요 없다)
fn add(store: &CommentStore, new: NewComment) -> Result<Comment, String> {
    if !matches!(new.view.as_str(), "branch" | "session" | "turn") {
        return Err(format!("알 수 없는 view: {}", new.view));
    }
    store.add(&new).map_err(msg)
}

fn update(store: &CommentStore, id: i64, body: &str) -> Result<Comment, String> {
    store.update_body(id, body).map_err(msg)
}

/// 미전송 코멘트 전부. 첨부와 달리 cap이 없고, 상태는 바꾸지 않는다
fn copy_payload(store: &CommentStore, worktree: &str) -> Result<CopyPayload, String> {
    let unsent: Vec<Comment> = list(store, worktree)?.into_iter().filter(|c| c.sent_at.is_none()).collect();
    let rendered = render(worktree, &unsent, None);
    Ok(CopyPayload { markdown: rendered.text, ids: rendered.included })
}

/// registry를 열 때 함께 열린다. registry를 기다린 뒤에는 항상 채워져 있다 (registry가 실패했으면 그 오류)
async fn with_store<T: Send + 'static>(
    app: AppHandle,
    f: impl FnOnce(&CommentStore) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    blocking(move || {
        registry(&app)?;
        let store = app.state::<AppState>().inner().comments.get().ok_or("코멘트 저장소가 열리지 않았습니다")?;
        f(store)
    })
    .await
}

#[tauri::command]
pub async fn list_comments(app: AppHandle, worktree_id: String) -> Result<Vec<Comment>, String> {
    with_store(app, move |s| list(s, &worktree_id)).await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn add_comment(
    app: AppHandle,
    worktree_id: String,
    path: String,
    side: Side,
    line: u32,
    view: String,
    snippet: String,
    body: String,
) -> Result<Comment, String> {
    let new = NewComment { worktree: worktree_id, path, side, line, view, snippet, body };
    with_store(app, move |s| add(s, new)).await
}

#[tauri::command]
pub async fn update_comment(app: AppHandle, id: i64, body: String) -> Result<Comment, String> {
    with_store(app, move |s| update(s, id, &body)).await
}

#[tauri::command]
pub async fn delete_comment(app: AppHandle, id: i64) -> Result<(), String> {
    with_store(app, move |s| s.delete(id).map_err(msg)).await
}

#[tauri::command]
pub async fn clear_sent_comments(app: AppHandle, worktree_id: String) -> Result<usize, String> {
    with_store(app, move |s| s.clear_sent(&worktree_id).map_err(msg)).await
}

#[tauri::command]
pub async fn copy_comments(app: AppHandle, worktree_id: String) -> Result<CopyPayload, String> {
    with_store(app, move |s| copy_payload(s, &worktree_id)).await
}

#[tauri::command]
pub async fn mark_comments_sent(app: AppHandle, ids: Vec<i64>) -> Result<usize, String> {
    with_store(app, move |s| s.mark_sent(&ids, "copy", now_ms()).map_err(msg)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new(view: &str, body: &str) -> NewComment {
        NewComment {
            worktree: "/w".into(),
            path: "a.rs".into(),
            side: Side::New,
            line: 1,
            view: view.into(),
            snippet: String::new(),
            body: body.into(),
        }
    }

    #[test]
    fn add_rejects_unknown_view() {
        let store = CommentStore::in_memory();
        assert!(add(&store, new("commit", "x")).is_err());
        for view in ["branch", "session", "turn"] {
            assert!(add(&store, new(view, "x")).is_ok(), "{view}");
        }
    }

    #[test]
    fn copy_payload_lists_unsent_only() {
        let store = CommentStore::in_memory();
        let long = "가".repeat(4000);
        let ids: Vec<i64> = (0..3).map(|_| add(&store, new("branch", &long)).unwrap().id).collect();
        store.mark_sent(&ids[..1], "hook", 1).unwrap();
        let payload = copy_payload(&store, "/w").unwrap();
        assert_eq!(payload.ids, ids[1..].to_vec());
        // 첨부와 달리 cap이 없어 4,000자 코멘트가 전부 들어간다
        assert_eq!(payload.markdown.matches(&long).count(), 2);
        // 상태는 바꾸지 않는다
        assert_eq!(store.unsent_counts().unwrap()["/w"], 2);
    }

    #[test]
    fn update_sent_error_text() {
        let store = CommentStore::in_memory();
        let c = add(&store, new("branch", "x")).unwrap();
        store.mark_sent(&[c.id], "copy", 1).unwrap();
        assert_eq!(update(&store, c.id, "y").unwrap_err(), "이미 전송된 코멘트는 고칠 수 없습니다");
    }
}
