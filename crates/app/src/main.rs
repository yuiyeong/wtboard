use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use wtboard_core::domain::{ChangedFile, FilePatch};
use wtboard_core::store::config::Config;
use wtboard_core::watch::{spawn_watcher, EmitFn, WatchHandle};
use wtboard_core::worktree::{Worktree, WorktreeChanged, WorktreeSummary};

/// 1단계: config.toml의 첫 번째 레포 하나만 연다. 실패하면 이유를 화면에 보여 준다.
struct AppState {
    worktree: Result<Arc<Worktree>, String>,
    _watch: Mutex<Option<WatchHandle>>,
}

impl AppState {
    fn worktree(&self, id: &str) -> Result<&Arc<Worktree>, String> {
        let wt = self.worktree.as_ref().map_err(Clone::clone)?;
        if wt.id() == id {
            Ok(wt)
        } else {
            Err(format!("알 수 없는 worktree: {id}"))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoSummary {
    name: String,
    worktrees: Vec<WorktreeSummary>,
}

#[tauri::command]
async fn list_repos(state: State<'_, AppState>) -> Result<Vec<RepoSummary>, String> {
    let wt = state.worktree.as_ref().map_err(Clone::clone)?;
    let summary = wt.summary();
    let name = Path::new(&summary.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(vec![RepoSummary { name, worktrees: vec![summary] }])
}

#[tauri::command]
async fn list_files(state: State<'_, AppState>, worktree_id: String, view: String) -> Result<Vec<ChangedFile>, String> {
    ensure_branch_view(&view)?;
    Ok(state.worktree(&worktree_id)?.files())
}

#[tauri::command]
async fn get_patch(
    state: State<'_, AppState>,
    worktree_id: String,
    view: String,
    path: String,
) -> Result<FilePatch, String> {
    ensure_branch_view(&view)?;
    state.worktree(&worktree_id)?.patch(&path).map_err(|e| e.to_string())
}

fn ensure_branch_view(view: &str) -> Result<(), String> {
    if view == "branch" {
        Ok(())
    } else {
        Err(format!("아직 지원하지 않는 view: {view}"))
    }
}

fn open_first_repo() -> Result<(Arc<Worktree>, Config), String> {
    let path = Config::default_path();
    let hint = format!("{} 에 다음을 추가하세요:\n\n[[repo]]\npath = \"~/dev/my-repo\"", path.display());
    let config = Config::load(&path).map_err(|e| format!("{e}\n\n{hint}"))?;
    let repo = config.repos.first().ok_or_else(|| format!("등록된 레포가 없습니다.\n\n{hint}"))?;
    let wt = Worktree::open(&repo.resolved_path(), repo.base.clone()).map_err(|e| e.to_string())?;
    Ok((wt, config))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let (worktree, watch) = match open_first_repo() {
                Ok((wt, config)) => {
                    let emit: EmitFn = Arc::new(move |ev: WorktreeChanged| {
                        let _ = handle.emit("worktree-changed", ev);
                    });
                    let poll = config.watch.poll_interval();
                    match spawn_watcher(wt.clone(), poll, emit) {
                        Ok(w) => (Ok(wt), Some(w)),
                        Err(e) => (Err(format!("파일 감시를 시작할 수 없음: {e}")), None),
                    }
                }
                Err(e) => (Err(e), None),
            };
            app.manage(AppState { worktree, _watch: Mutex::new(watch) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_repos, list_files, get_patch])
        .run(tauri::generate_context!())
        .expect("wtboard 실행 실패");
}
