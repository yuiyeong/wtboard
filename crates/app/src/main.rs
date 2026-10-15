use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use wtboard_core::domain::{ChangedFile, FilePatch};
use wtboard_core::gh::Gh;
use wtboard_core::registry::{EntryError, Registry};
use wtboard_core::repo::{RepoSummary, Sink};
use wtboard_core::store::config::Config;
use wtboard_core::worktree::{Worktree, WorktreeChanged};

/// Registry는 background thread에서 열고 끝나면 채운다. 그 전에는 비어 있다
struct AppState {
    registry: OnceLock<Result<Registry, String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReposResponse {
    ready: bool,
    repos: Vec<RepoSummary>,
    errors: Vec<EntryError>,
}

#[tauri::command]
async fn list_repos(app: AppHandle) -> Result<ReposResponse, String> {
    blocking(move || match app.state::<AppState>().registry.get() {
        None => Ok(ReposResponse { ready: false, repos: Vec::new(), errors: Vec::new() }),
        Some(Err(e)) => Err(e.clone()),
        Some(Ok(reg)) => Ok(ReposResponse { ready: true, repos: reg.repos(), errors: reg.errors().to_vec() }),
    })
    .await
}

#[tauri::command]
async fn list_files(app: AppHandle, worktree_id: String, view: String) -> Result<Vec<ChangedFile>, String> {
    ensure_branch_view(&view)?;
    blocking(move || Ok(worktree(&app, &worktree_id)?.files())).await
}

#[tauri::command]
async fn get_patch(app: AppHandle, worktree_id: String, view: String, path: String) -> Result<FilePatch, String> {
    ensure_branch_view(&view)?;
    blocking(move || worktree(&app, &worktree_id)?.patch(&path).map_err(|e| e.to_string())).await
}

/// git을 부르는 일은 tokio worker를 막지 않게 blocking pool에서 한다
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn worktree(app: &AppHandle, id: &str) -> Result<Arc<Worktree>, String> {
    match app.state::<AppState>().registry.get() {
        Some(Ok(reg)) => reg.worktree(id).ok_or_else(|| format!("알 수 없는 worktree: {id}")),
        Some(Err(e)) => Err(e.clone()),
        None => Err("아직 불러오는 중입니다".into()),
    }
}

fn ensure_branch_view(view: &str) -> Result<(), String> {
    if view == "branch" {
        Ok(())
    } else {
        Err(format!("아직 지원하지 않는 view: {view}"))
    }
}

fn open_registry(handle: &AppHandle) -> Result<Registry, String> {
    let path = Config::default_path();
    let hint = format!("{} 에 다음을 추가하세요:\n\n[[repo]]\npath = \"~/dev/my-repo\"", path.display());
    let config = Config::load(&path).map_err(|e| format!("{e}\n\n{hint}"))?;
    if config.repos.is_empty() {
        return Err(format!("등록된 레포가 없습니다.\n\n{hint}"));
    }
    let (changed, updated) = (handle.clone(), handle.clone());
    let sink = Sink {
        changed: Arc::new(move |ev: WorktreeChanged| {
            let _ = changed.emit("worktree-changed", ev);
        }),
        updated: Arc::new(move || {
            let _ = updated.emit("worktrees-updated", ());
        }),
    };
    Ok(Registry::open(&config, Gh::detect(), sink))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState { registry: OnceLock::new() });
            let handle = app.handle().clone();
            // 레포가 많거나 크면 수 초 걸린다. 창을 막지 않는다
            std::thread::spawn(move || {
                let result = open_registry(&handle);
                let _ = handle.state::<AppState>().registry.set(result);
                let _ = handle.emit("worktrees-updated", ());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_repos, list_files, get_patch])
        .run(tauri::generate_context!())
        .expect("wtboard 실행 실패");
}
