mod comments;
mod hook;
mod tray;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use wtboard_core::domain::FilePatch;
use wtboard_core::gh::Gh;
use wtboard_core::registry::{EntryError, Registry};
use wtboard_core::repo::{RepoSummary, Sink};
use wtboard_core::session::list::{active_roots, cleanup, sessions_for, worktrees_of, SessionSummary};
use wtboard_core::session::view::{self, SessionView, SessionViewCache};
use wtboard_core::session::{sessions_dir, SessionDir};
use wtboard_core::store::comments::CommentStore;
use wtboard_core::store::config::Config;
use wtboard_core::store::data_dir;
use wtboard_core::store::settings::SettingsStore;
use wtboard_core::store::viewed::ViewedStore;
use wtboard_core::watch::{spawn_sessions_watcher, SessionsWatchHandle};
use wtboard_core::worktree::{now_ms, FileEntry, Worktree, WorktreeChanged};

use tray::TrayUpdater;

/// Registry는 background thread에서 열고 끝나면 채운다. command는 채워질 때까지 기다린다
struct AppState {
    registry: OnceLock<Result<Registry, String>>,
    /// 세션 기록 감시. registry를 열 때 시작한다 (바뀐 세션의 worktree를 정하려면 root 목록이 필요하다)
    sessions_watch: OnceLock<SessionsWatchHandle>,
    /// 세션 view 목록의 파일 비교 결과. 목록을 다시 계산할 때 바뀌지 않은 파일은 비교하지 않는다
    session_cache: SessionViewCache,
    /// registry와 함께 열린다. registry를 기다린 뒤에는 채워져 있다
    comments: OnceLock<Arc<CommentStore>>,
    /// 앱 설정(tray 등). 열 때 `comments`와 같이 채운다
    settings: OnceLock<SettingsStore>,
    tray: TrayUpdater,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReposResponse {
    repos: Vec<RepoSummary>,
    errors: Vec<EntryError>,
    /// 설정을 읽지 못하면 true (기본값)
    auto_attach_comments: bool,
}

/// `session-updated` event. worktree는 그 세션들이 속한 것
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionUpdated {
    session_ids: Vec<String>,
    worktree_ids: Vec<String>,
}

#[tauri::command]
async fn list_repos(app: AppHandle) -> Result<ReposResponse, String> {
    blocking(move || {
        let reg = registry(&app)?;
        let mut repos = reg.repos();
        let active = active_roots(&data_dir(), &reg.roots(), now_ms());
        // 코멘트 저장소는 registry와 함께 열린다. 못 읽으면 0으로 둔다
        let unsent = app.state::<AppState>().inner().comments.get().and_then(|s| s.unsent_counts().ok());
        for w in repos.iter_mut().flat_map(|r| &mut r.worktrees) {
            w.active_session = active.contains(Path::new(&w.id));
            w.unsent_comments = unsent.as_ref().and_then(|u| u.get(&w.id)).copied().unwrap_or(0);
        }
        let auto_attach_comments =
            Config::load(&Config::default_path()).map_or(true, |c| c.review.auto_attach_comments);
        Ok(ReposResponse { repos, errors: reg.errors().to_vec(), auto_attach_comments })
    })
    .await
}

#[tauri::command]
async fn list_sessions(app: AppHandle, worktree_id: String) -> Result<Vec<SessionSummary>, String> {
    blocking(move || {
        let wt = worktree(&app, &worktree_id)?;
        Ok(sessions_for(&data_dir(), &wt.layout().root, &registry(&app)?.roots(), now_ms()))
    })
    .await
}

#[tauri::command]
async fn list_files(
    app: AppHandle,
    worktree_id: String,
    view: String,
    session_id: Option<String>,
) -> Result<Vec<FileEntry>, String> {
    let view = parse_view(&view, session_id)?;
    blocking(move || {
        let wt = worktree(&app, &worktree_id)?;
        match view {
            View::Branch => Ok(wt.entries()),
            View::Session(dir, v) => {
                let cache = &app.state::<AppState>().inner().session_cache;
                view::changes(&dir, v, &wt, &registry(&app)?.roots(), cache).map_err(|e| e.to_string())
            }
        }
    })
    .await
}

#[tauri::command]
async fn get_patch(
    app: AppHandle,
    worktree_id: String,
    view: String,
    path: String,
    session_id: Option<String>,
) -> Result<FilePatch, String> {
    let view = parse_view(&view, session_id)?;
    blocking(move || {
        let wt = worktree(&app, &worktree_id)?;
        match view {
            View::Branch => wt.patch(&path),
            View::Session(dir, v) => view::patch(&dir, v, &wt, &path, &registry(&app)?.roots()),
        }
        .map_err(|e| e.to_string())
    })
    .await
}

/// 봤음은 view와 무관하다. hash는 화면이 보고 있는 diff의 hash다 (없으면 현재 diff)
#[tauri::command]
async fn set_viewed(
    app: AppHandle,
    worktree_id: String,
    path: String,
    viewed: bool,
    hash: Option<String>,
) -> Result<(), String> {
    blocking(move || {
        worktree(&app, &worktree_id)?.set_viewed(&path, viewed, hash).map_err(|e| e.to_string())?;
        request_tray(&app);
        Ok(())
    })
    .await
}

/// git을 부르는 일은 tokio worker를 막지 않게 blocking pool에서 한다
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// 아직 여는 중이면 열릴 때까지 기다린다. blocking pool 안에서만 부른다
fn registry(app: &AppHandle) -> Result<&Registry, String> {
    app.state::<AppState>().inner().registry.wait().as_ref().map_err(Clone::clone)
}

/// 메뉴바를 다시 계산하게 한다. 1초에 한 번으로 모인다
fn request_tray(app: &AppHandle) {
    app.state::<AppState>().inner().tray.request();
}

fn worktree(app: &AppHandle, id: &str) -> Result<Arc<Worktree>, String> {
    registry(app)?.worktree(id).ok_or_else(|| format!("알 수 없는 worktree: {id}"))
}

enum View {
    /// 브랜치 전체
    Branch,
    /// 이번 세션, 마지막 응답
    Session(SessionDir, SessionView),
}

/// `session`·`turn`은 session id가 있어야 한다. id는 화면에서 온 값이라 형식을 확인한다 (폴더 이름으로 쓴다)
fn parse_view(view: &str, session_id: Option<String>) -> Result<View, String> {
    let kind = match view {
        "branch" => return Ok(View::Branch),
        "session" => SessionView::Session,
        "turn" => SessionView::Turn,
        _ => return Err(format!("알 수 없는 view: {view}")),
    };
    let id = session_id.ok_or_else(|| format!("{view} view에는 session id가 필요합니다"))?;
    let dir = SessionDir::open(&data_dir(), &id).ok_or_else(|| format!("잘못된 session id: {id}"))?;
    Ok(View::Session(dir, kind))
}

/// 세션 폴더가 바뀌면 그 세션이 속한 worktree를 구해 `session-updated`를 보낸다. 어느 worktree에도 속하지 않으면 보내지 않는다
fn watch_sessions(handle: &AppHandle, data: PathBuf) -> Result<SessionsWatchHandle, String> {
    let app = handle.clone();
    spawn_sessions_watcher(
        &sessions_dir(&data),
        Arc::new(move |session_ids: Vec<String>| {
            // registry를 연 직후에 시작하므로 곧 채워진다
            let Ok(reg) = registry(&app) else { return };
            let roots = reg.roots();
            let owned: HashSet<PathBuf> = session_ids
                .iter()
                .filter_map(|id| SessionDir::open(&data, id))
                .flat_map(|dir| worktrees_of(&dir, &roots))
                .collect();
            let worktree_ids: Vec<String> =
                roots.iter().filter(|r| owned.contains(*r)).map(|r| r.to_string_lossy().into_owned()).collect();
            if !worktree_ids.is_empty() {
                let _ = app.emit("session-updated", SessionUpdated { session_ids, worktree_ids });
                request_tray(&app);
            }
        }),
    )
    .map_err(|e| format!("세션 기록을 감시할 수 없음: {e}"))
}

/// store를 열고, 열 수 없으면 메모리 store와 경고를 돌려준다
fn open_or_memory<T, E: std::fmt::Display>(
    path: &Path,
    warning: &str,
    open: impl FnOnce(&Path) -> Result<T, E>,
    in_memory: impl FnOnce() -> T,
) -> (T, Option<EntryError>) {
    match open(path) {
        Ok(store) => (store, None),
        Err(e) => {
            (in_memory(), Some(EntryError { path: path.display().to_string(), message: format!("{warning}: {e}") }))
        }
    }
}

fn open_registry(handle: &AppHandle) -> Result<Registry, String> {
    let path = Config::default_path();
    let hint = format!("{} 에 다음을 추가하세요:\n\n[[repo]]\npath = \"~/dev/my-repo\"", path.display());
    let config = Config::load(&path).map_err(|e| format!("{e}\n\n{hint}"))?;
    // 오래된 세션 기록을 지운다. 감시를 시작하기 전에 해서 지우는 것이 event로 오지 않게 한다
    let data = data_dir();
    cleanup(&data, config.sessions.keep_days, now_ms());
    if config.repos.is_empty() {
        return Err(format!("등록된 레포가 없습니다.\n\n{hint}"));
    }
    let (changed, updated) = (handle.clone(), handle.clone());
    let sink = Sink {
        changed: Arc::new(move |ev: WorktreeChanged| {
            let _ = changed.emit("worktree-changed", ev);
            request_tray(&changed);
        }),
        updated: Arc::new(move || {
            let _ = updated.emit("worktrees-updated", ());
            request_tray(&updated);
        }),
    };
    // 상태는 data dir(config 파일이 있는 폴더)에 둔다. 열 수 없으면 이번 실행만 메모리에 둔다
    let state_path = data.join("state.sqlite");
    let (viewed, viewed_warning) =
        open_or_memory(&state_path, "봤음 상태를 저장할 수 없음", ViewedStore::open, ViewedStore::in_memory);
    let (comments, comments_warning) =
        open_or_memory(&state_path, "코멘트를 저장할 수 없음", CommentStore::open, CommentStore::in_memory);
    let (settings, settings_warning) =
        open_or_memory(&state_path, "설정을 저장할 수 없음", SettingsStore::open, SettingsStore::in_memory);
    // 전송된 지 오래된 코멘트를 지운다
    let cutoff = now_ms().saturating_sub(config.sessions.keep_days.saturating_mul(86_400_000));
    let _ = comments.purge_sent_before(cutoff);
    tray::apply_saved_on_top(handle, &settings);
    let state = handle.state::<AppState>();
    let _ = state.comments.set(Arc::new(comments));
    let _ = state.settings.set(settings);
    let mut reg = Registry::open(&config, Gh::detect(), Arc::new(viewed), sink);
    for w in [viewed_warning, comments_warning, settings_warning].into_iter().flatten() {
        reg.warn(w);
    }
    // 옛 hook binary는 schema가 올라간 DB를 열지 못해 첨부가 조용히 멈춘다
    let bin = hook::hook_binary();
    if let Some(message) = hook::check(&bin) {
        reg.warn(EntryError { path: bin.display().to_string(), message });
    }
    let sessions = sessions_dir(&data);
    match watch_sessions(handle, data) {
        Ok(h) => {
            let _ = handle.state::<AppState>().sessions_watch.set(h);
        }
        Err(e) => reg.warn(EntryError { path: sessions.display().to_string(), message: e }),
    }
    Ok(reg)
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState {
                registry: OnceLock::new(),
                sessions_watch: OnceLock::new(),
                session_cache: SessionViewCache::default(),
                comments: OnceLock::new(),
                settings: OnceLock::new(),
                tray: TrayUpdater::spawn(app.handle().clone()),
            });
            tray::build(app.handle())?;
            let handle = app.handle().clone();
            // 레포가 많거나 크면 수 초 걸린다. 창을 막지 않는다
            std::thread::spawn(move || {
                let result = open_registry(&handle);
                let _ = handle.state::<AppState>().registry.set(result);
            });
            request_tray(app.handle());
            Ok(())
        })
        // 창 닫기 = 숨기기. 앱은 메뉴바에 남는다
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_repos,
            list_sessions,
            list_files,
            get_patch,
            set_viewed,
            comments::list_comments,
            comments::add_comment,
            comments::update_comment,
            comments::delete_comment,
            comments::clear_sent_comments,
            comments::copy_comments,
            comments::mark_comments_sent,
            comments::resolve_comment,
            comments::unresolve_comment,
            comments::resend_comment,
        ])
        .build(tauri::generate_context!())
        .expect("wtboard 실행 실패")
        .run(|app, event| {
            // Dock 아이콘을 누르면 숨긴 창을 다시 보인다
            if let RunEvent::Reopen { .. } = event {
                tray::show_window(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_views_need_a_valid_session_id() {
        assert!(matches!(parse_view("branch", None), Ok(View::Branch)));
        assert!(matches!(parse_view("turn", Some("s-1_A".into())), Ok(View::Session(_, SessionView::Turn))));
        assert!(matches!(parse_view("session", Some("s1".into())), Ok(View::Session(_, SessionView::Session))));
        assert!(parse_view("session", None).is_err());
        assert!(parse_view("turn", Some("../x".into())).is_err());
        assert!(parse_view("turn", Some(String::new())).is_err());
        assert!(parse_view("commit", None).is_err());
    }
}
