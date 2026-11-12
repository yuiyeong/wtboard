//! 설정창의 Repositories. `config.toml`의 `[[repo]]`를 읽고 고친 뒤 열린 registry에 반영한다
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use wtboard_core::git::branches::base_candidates;
use wtboard_core::git::Git;
use wtboard_core::registry::{check_new_repo, inspect_entry};
use wtboard_core::store::config::{Config, RepoConfig};
use wtboard_core::store::config_edit::{self, abbreviate_home, update_file};
use wtboard_core::worktree::{ChangeScope, WorktreeChanged};

use crate::settings_window::SETTINGS;
use crate::{blocking, registry, request_tray, unsent_counts, AppState};

/// 설정창의 한 줄 = `config.toml`의 `[[repo]]` entry 하나
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoEntry {
    /// config에 적힌 그대로. 줄을 가리키는 key이기도 하다
    path: String,
    name: String,
    base: Option<String>,
    auto_base: Option<String>,
    /// 열 수 없는 entry의 이유
    error: Option<String>,
    /// 그 레포의 모든 worktree에 남은 미전송 코멘트
    unsent_comments: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoEntries {
    entries: Vec<RepoEntry>,
    config_path: String,
    /// config를 읽지 못한 이유. 있으면 `entries`는 비고, 화면은 추가·삭제·base 변경을 끈다
    error: Option<String>,
}

/// registry를 기다리지 않는다 (앱이 레포를 여는 동안에도 설정창이 바로 보인다)
#[tauri::command]
pub async fn list_repo_entries(app: AppHandle) -> Result<RepoEntries, String> {
    blocking(move || Ok(entries(&Config::default_path(), &unsent_counts(&app)))).await
}

/// `path` = config에 적힌 문자열
#[tauri::command]
pub async fn list_base_candidates(path: String) -> Result<Vec<String>, String> {
    blocking(move || {
        let root = RepoConfig { path, base: None }.resolved_path();
        base_candidates(&Git::new(&root)).map_err(|e| e.to_string())
    })
    .await
}

/// 폴더를 골라 등록한다. dialog를 취소하면 `None`
#[tauri::command]
pub async fn add_repo(app: AppHandle) -> Result<Option<String>, String> {
    blocking(move || {
        // 고르는 동안(사용자 시간) config lock을 잡지 않는다
        let Some(picked) = pick_folder(&app) else { return Ok(None) };
        let home = std::env::var_os("HOME").map(PathBuf::from);
        edit(&app, |path| add_picked(path, &picked, home.as_deref())).map(Some)
    })
    .await
}

#[tauri::command]
pub async fn remove_repo(app: AppHandle, path: String) -> Result<(), String> {
    blocking(move || {
        edit(&app, |config| {
            update_file(config, |src| config_edit::remove_repo(src, &path)).map_err(|e| e.to_string())
        })
    })
    .await
}

/// `base`가 `None`이면 자동 (`base` 줄을 지운다)
#[tauri::command]
pub async fn set_repo_base(app: AppHandle, path: String, base: Option<String>) -> Result<(), String> {
    blocking(move || {
        edit(&app, |config| {
            update_file(config, |src| config_edit::set_repo_base(src, &path, base.as_deref()))
                .map_err(|e| e.to_string())
        })
    })
    .await
}

/// `WTBOARD_PICK_FOLDER`가 있으면 dialog 대신 그 경로 (실제 앱 확인용). 설정창이 열려 있으면 그 창의 sheet로 띄운다
fn pick_folder(app: &AppHandle) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("WTBOARD_PICK_FOLDER").filter(|v| !v.is_empty()) {
        return Some(p.into());
    }
    let mut dialog = app.dialog().file();
    if let Some(w) = app.get_webview_window(SETTINGS) {
        dialog = dialog.set_parent(&w);
    }
    dialog.blocking_pick_folder().and_then(|p| p.into_path().ok())
}

/// config lock 안에서 `f`로 `config.toml`을 고치고 registry에 반영한 뒤 화면과 메뉴바에 알린다.
/// `update_file`은 자체 lock이 없고 임시 파일 이름이 process마다 하나라서, 읽기부터 반영까지 한 번에 하나만 돈다
fn edit<T>(app: &AppHandle, f: impl FnOnce(&Path) -> Result<T, String>) -> Result<T, String> {
    let state = app.state::<AppState>();
    let _serial = state.config_lock.lock().unwrap_or_else(PoisonError::into_inner);
    let path = Config::default_path();
    let out = f(&path)?;
    let config = Config::load(&path).map_err(|e| e.to_string())?;
    // 다시 연 레포는 worktree id가 그대로라 화면의 파일 목록·diff query가 옛 base로 남는다
    for worktree_id in registry(app)?.apply(&config) {
        let ev = WorktreeChanged { worktree_id, changed_paths: Vec::new(), scope: ChangeScope::Base };
        let _ = app.emit("worktree-changed", ev);
    }
    let _ = app.emit("worktrees-updated", ());
    request_tray(app);
    Ok(out)
}

/// `unsent`: worktree id → 미전송 코멘트 수
fn entries(config_path: &Path, unsent: &HashMap<String, usize>) -> RepoEntries {
    let config_path_text = config_path.display().to_string();
    let config = match Config::load_or_default(config_path) {
        Ok(c) => c,
        Err(e) => return RepoEntries { entries: Vec::new(), config_path: config_path_text, error: Some(e.to_string()) },
    };
    // entry마다 git을 부른다
    let infos: Vec<_> = std::thread::scope(|s| {
        let handles: Vec<_> = config.repos.iter().map(|e| s.spawn(move || inspect_entry(e))).collect();
        handles.into_iter().map(|h| h.join().expect("entry 확인 thread panic")).collect()
    });
    let entries = config
        .repos
        .into_iter()
        .zip(infos)
        .map(|(entry, info)| RepoEntry {
            // 열 수 없는 entry는 이름을 모르므로 config 경로의 끝을 쓴다
            name: if info.name.is_empty() { folder_name(&entry.path) } else { info.name },
            unsent_comments: info.worktrees.iter().filter_map(|w| unsent.get(&*w.to_string_lossy())).sum(),
            path: entry.path,
            base: entry.base,
            auto_base: info.auto_base,
            error: info.error,
        })
        .collect();
    RepoEntries { entries, config_path: config_path_text, error: None }
}

fn folder_name(path: &str) -> String {
    Path::new(path).file_name().map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// 고른 폴더(레포 안 어느 폴더든)의 worktree root를 등록한다. 쓴 path(home 아래면 `~/…`)를 돌려준다
fn add_picked(config_path: &Path, picked: &Path, home: Option<&Path>) -> Result<String, String> {
    let config = Config::load_or_default(config_path).map_err(|e| e.to_string())?;
    let root = check_new_repo(&config, picked).map_err(|e| e.to_string())?;
    let written = match home {
        Some(home) => abbreviate_home(&root, home),
        None => root.display().to_string(),
    };
    update_file(config_path, |src| config_edit::add_repo(src, &written)).map_err(|e| e.to_string())?;
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    fn git_init(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let ok = Command::new("git").args(["init", "-q"]).current_dir(dir).status().unwrap().success();
        assert!(ok, "git init 실패");
        std::fs::canonicalize(dir).unwrap()
    }

    #[test]
    fn missing_config_is_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let out = entries(&path, &HashMap::new());
        assert!(out.entries.is_empty());
        assert_eq!(out.error, None);
        assert_eq!(out.config_path, path.display().to_string());
    }

    #[test]
    fn broken_config_has_error_and_no_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        std::fs::write(&path, "[[repo]]\npath = \"/a\"\n[[repo").unwrap();
        let out = entries(&path, &HashMap::new());
        assert!(out.entries.is_empty());
        assert!(out.error.unwrap().contains("형식 오류"));
    }

    #[test]
    fn entries_follow_config_order_with_reason_and_unsent_sum() {
        let tmp = tempfile::tempdir().unwrap();
        let root = git_init(&tmp.path().join("app"));
        let missing = tmp.path().join("gone");
        let path = tmp.path().join("config.toml");
        std::fs::write(
            &path,
            format!("[[repo]]\npath = \"{}\"\nbase = \"v1\"\n\n[[repo]]\npath = \"{}\"\n", missing.display(), root.display()),
        )
        .unwrap();
        let unsent = HashMap::from([(root.display().to_string(), 2), ("/other".to_string(), 5)]);
        let out = entries(&path, &unsent);
        assert_eq!(out.error, None);
        let [gone, app] = &out.entries[..] else { panic!("{:?}", out.entries) };
        assert_eq!((gone.name.as_str(), gone.base.as_deref()), ("gone", Some("v1")));
        assert_eq!(gone.error.as_deref(), Some("경로 없음"));
        assert_eq!(gone.unsent_comments, 0);
        assert_eq!((app.name.as_str(), app.path.clone()), ("app", root.display().to_string()));
        assert_eq!((app.error.as_deref(), app.auto_base.as_deref(), app.unsent_comments), (None, None, 2));
    }

    #[test]
    fn add_writes_worktree_root_abbreviated_under_home() {
        let tmp = tempfile::tempdir().unwrap();
        let home = std::fs::canonicalize(tmp.path()).unwrap();
        git_init(&home.join("dev/app"));
        let path = home.join(".wtboard/config.toml");
        assert_eq!(add_picked(&path, &home.join("dev/app/src"), Some(&home)), Ok("~/dev/app".to_string()));
        let written = Config::load(&path).unwrap();
        assert_eq!(written.repos[0].path, "~/dev/app");
    }

    #[test]
    fn add_refuses_registered_repo_and_plain_folder_without_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = git_init(&tmp.path().join("app"));
        let plain = tmp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        let path = tmp.path().join("config.toml");
        let before = "# 내 설정\n[[repo]]\npath = \"".to_string() + &root.display().to_string() + "\"\n";
        std::fs::write(&path, &before).unwrap();
        assert_eq!(add_picked(&path, &root.join("src"), None), Err("이미 등록된 repository입니다".to_string()));
        assert_eq!(add_picked(&path, &plain, None), Err("git repository가 아닙니다".to_string()));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn entry_serializes_camel_case() {
        let entry = RepoEntry {
            path: "~/a".into(),
            name: "a".into(),
            base: None,
            auto_base: Some("origin/main".into()),
            error: None,
            unsent_comments: 1,
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["autoBase"], "origin/main");
        assert_eq!(json["unsentComments"], 1);
    }
}
