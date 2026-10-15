use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use notify::RecommendedWatcher;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use super::Target;
use crate::worktree::{Worktree, WorktreeChanged};

pub type EmitFn = Arc<dyn Fn(WorktreeChanged) + Send + Sync>;

const DEBOUNCE: Duration = Duration::from_millis(300);

/// drop하면 감시가 멈추고 worker thread도 끝난다.
pub struct WatchHandle {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

enum Msg {
    Paths(Vec<PathBuf>),
    /// FSEvents가 event를 놓쳤다고 알림 → 전체 재계산
    Rescan,
}

pub fn spawn_watcher(wt: Arc<Worktree>, poll: Duration, emit: EmitFn) -> notify::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |res: DebounceEventResult| {
        let msg = match res {
            Ok(events) if events.iter().any(|e| e.need_rescan()) => Msg::Rescan,
            Ok(events) => Msg::Paths(events.into_iter().flat_map(|e| e.event.paths).collect()),
            Err(_) => Msg::Rescan,
        };
        let _ = tx.send(msg);
    })?;
    for (path, mode) in wt.layout().watch_roots() {
        debouncer.watch(&path, mode)?;
    }
    std::thread::Builder::new()
        .name("wtboard-watch".into())
        .spawn(move || run_worker(wt, rx, poll, emit))
        .map_err(notify::Error::io)?;
    Ok(WatchHandle { _debouncer: debouncer })
}

fn run_worker(wt: Arc<Worktree>, rx: mpsc::Receiver<Msg>, poll: Duration, emit: EmitFn) {
    let mut next_poll = Instant::now() + poll;
    loop {
        let targets: Vec<Target> = match rx.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(Msg::Paths(paths)) => paths
                .iter()
                .map(|p| wt.layout().classify(p))
                .filter(|t| *t != Target::Ignore)
                .collect(),
            Ok(Msg::Rescan) => vec![Target::Base],
            // FSEvents 누락 대비 백업 재계산. Base는 merge-base가 그대로면 목록 재계산으로 끝난다
            Err(mpsc::RecvTimeoutError::Timeout) => {
                next_poll = Instant::now() + poll;
                vec![Target::Base]
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if targets.is_empty() {
            continue;
        }
        match wt.handle(&targets) {
            Ok(Some(ev)) => emit(ev),
            Ok(None) => {}
            Err(e) => eprintln!("[wtboard] {}: 재계산 실패: {e}", wt.id()),
        }
    }
}
