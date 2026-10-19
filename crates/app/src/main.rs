use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use wtboard_core::domain::FilePatch;
use wtboard_core::gh::Gh;
use wtboard_core::registry::{EntryError, Registry};
use wtboard_core::repo::{RepoSummary, Sink};
use wtboard_core::store::config::Config;
use wtboard_core::store::data_dir;
use wtboard_core::store::viewed::ViewedStore;
use wtboard_core::worktree::{FileEntry, Worktree, WorktreeChanged};

/// Registry는 background thread에서 열고 끝나면 채운다. command는 채워질 때까지 기다린다
struct AppState {
    registry: OnceLock<Result<Registry, String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReposResponse {
    repos: Vec<RepoSummary>,
    errors: Vec<EntryError>,
}

#[tauri::command]
async fn list_repos(app: AppHandle) -> Result<ReposResponse, String> {
    blocking(move || {
        let reg = registry(&app)?;
        Ok(ReposResponse { repos: reg.repos(), errors: reg.errors().to_vec() })
    })
    .await
}

#[tauri::command]
async fn list_files(app: AppHandle, worktree_id: String, view: String) -> Result<Vec<FileEntry>, String> {
    ensure_branch_view(&view)?;
    blocking(move || Ok(worktree(&app, &worktree_id)?.entries())).await
}

#[tauri::command]
async fn get_patch(app: AppHandle, worktree_id: String, view: String, path: String) -> Result<FilePatch, String> {
    ensure_branch_view(&view)?;
    blocking(move || worktree(&app, &worktree_id)?.patch(&path).map_err(|e| e.to_string())).await
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
    blocking(move || worktree(&app, &worktree_id)?.set_viewed(&path, viewed, hash).map_err(|e| e.to_string())).await
}

/// git을 부르는 일은 tokio worker를 막지 않게 blocking pool에서 한다
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

/// 아직 여는 중이면 열릴 때까지 기다린다. blocking pool 안에서만 부른다
fn registry(app: &AppHandle) -> Result<&Registry, String> {
    app.state::<AppState>().inner().registry.wait().as_ref().map_err(Clone::clone)
}

fn worktree(app: &AppHandle, id: &str) -> Result<Arc<Worktree>, String> {
    registry(app)?.worktree(id).ok_or_else(|| format!("알 수 없는 worktree: {id}"))
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
    // 봤음 상태는 data dir(config 파일이 있는 폴더)에 둔다. 열 수 없으면 이번 실행만 메모리에 둔다
    let state_path = data_dir().join("state.sqlite");
    let (viewed, warning) = match ViewedStore::open(&state_path) {
        Ok(store) => (store, None),
        Err(e) => (
            ViewedStore::in_memory(),
            Some(EntryError {
                path: state_path.display().to_string(),
                message: format!("봤음 상태를 저장할 수 없음: {e}"),
            }),
        ),
    };
    let mut reg = Registry::open(&config, Gh::detect(), Arc::new(viewed), sink);
    if let Some(w) = warning {
        reg.warn(w);
    }
    Ok(reg)
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
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_repos, list_files, get_patch, set_viewed])
        .run(tauri::generate_context!())
        .expect("wtboard 실행 실패");
}
