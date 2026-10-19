use std::fs;
use std::path::{Component, Path};
use std::sync::Arc;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};

use super::DEBOUNCE;
use crate::paths::{canonical, nfc};
use crate::session::format::create_private_dir;
use crate::session::is_valid_session_id;

/// 바뀐 session id들. 중복 없이, 처음 나온 순서대로
pub type SessionsChangedFn = Arc<dyn Fn(Vec<String>) + Send + Sync>;

/// drop하면 감시가 멈춘다.
pub struct SessionsWatchHandle {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

/// `<data dir>/sessions/`를 감시해 바뀐 경로의 첫 component(session id)를 모아 `on_change`를 부른다.
/// 폴더가 없으면 0700으로 만든다 (hook보다 앱이 먼저 떠도 감시할 수 있게).
/// event를 놓쳤다는 신호(rescan, 감시 오류)면 있는 세션 전부를 부른다.
pub fn spawn_sessions_watcher(sessions_dir: &Path, on_change: SessionsChangedFn) -> notify::Result<SessionsWatchHandle> {
    create_private_dir(sessions_dir).map_err(notify::Error::io)?;
    let dir = canonical(sessions_dir);
    let filter_dir = dir.clone();
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        DEBOUNCE,
        None,
        move |res: DebounceEventResult| {
            let ids = match res {
                Ok(events) if !events.iter().any(|e| e.need_rescan()) => {
                    let mut ids: Vec<String> = Vec::new();
                    for id in events.iter().flat_map(|e| &e.event.paths).filter_map(|p| session_id(&filter_dir, p)) {
                        if !ids.contains(&id) {
                            ids.push(id);
                        }
                    }
                    ids
                }
                _ => all_sessions(&filter_dir),
            };
            if !ids.is_empty() {
                on_change(ids);
            }
        },
        NoCache,
        notify::Config::default(),
    )?;
    debouncer.watch(&dir, RecursiveMode::Recursive)?;
    Ok(SessionsWatchHandle { _debouncer: debouncer })
}

/// `sessions_dir` 아래 경로의 첫 component가 session id면 그것
fn session_id(sessions_dir: &Path, path: &Path) -> Option<String> {
    let path = nfc(path);
    match path.strip_prefix(sessions_dir).ok()?.components().next()? {
        Component::Normal(s) => s.to_str().filter(|s| is_valid_session_id(s)).map(str::to_owned),
        _ => None,
    }
}

fn all_sessions(sessions_dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(sessions_dir) else { return Vec::new() };
    entries.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|s| is_valid_session_id(s)).collect()
}
