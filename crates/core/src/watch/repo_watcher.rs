use std::path::{Component, Path};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};

use super::DEBOUNCE;
use crate::paths::{canonical, nfc};

pub type RediscoverFn = Arc<dyn Fn() + Send + Sync>;

/// drop하면 감시가 멈추고 worker thread도 끝난다.
pub struct RepoWatchHandle {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

/// worktree가 생기거나 사라질 때(`<common dir>/worktrees/<이름>`), 그리고 `poll`마다 `on_change`를 부른다.
/// `worktrees/`는 첫 worktree를 만들 때 생기므로 common dir 전체를 감시하고 경로로 거른다.
/// file id cache를 쓰지 않아(`NoCache`) 시작할 때 `.git/objects`를 훑지 않는다.
pub fn spawn_repo_watcher(common_dir: &Path, poll: Duration, on_change: RediscoverFn) -> notify::Result<RepoWatchHandle> {
    let common_dir = canonical(common_dir);
    let filter_dir = common_dir.clone();
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        DEBOUNCE,
        None,
        move |res: DebounceEventResult| {
            let relevant = match res {
                Ok(events) => events
                    .iter()
                    .any(|e| e.need_rescan() || e.event.paths.iter().any(|p| is_worktree_entry(&filter_dir, p))),
                Err(_) => true,
            };
            if relevant {
                let _ = tx.send(());
            }
        },
        NoCache,
        notify::Config::default(),
    )?;
    debouncer.watch(&common_dir, RecursiveMode::Recursive)?;
    std::thread::Builder::new()
        .name("wtboard-repo-watch".into())
        .spawn(move || loop {
            match rx.recv_timeout(poll) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => on_change(),
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        })
        .map_err(notify::Error::io)?;
    Ok(RepoWatchHandle { _debouncer: debouncer })
}

/// `worktrees` 자신이나 `worktrees/<이름>`. 그 안의 파일(`HEAD`, `index`)은 커밋마다 바뀌므로 보지 않는다
/// FS event마다(대부분 `.git/objects`) 불리므로 할당을 피한다. NFC 변환은 ASCII가 아닐 때만
fn is_worktree_entry(common_dir: &Path, path: &Path) -> bool {
    let normalized;
    let path = if path.as_os_str().is_ascii() {
        path
    } else {
        normalized = nfc(path);
        &normalized
    };
    let Ok(rel) = path.strip_prefix(common_dir) else { return false };
    let mut parts = rel.components();
    parts.next() == Some(Component::Normal("worktrees".as_ref())) && parts.nth(1).is_none()
}
