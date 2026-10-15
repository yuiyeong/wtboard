use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use notify::RecommendedWatcher;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use super::Target;
use crate::worktree::{Worktree, WorktreeChanged};

pub type EmitFn = Arc<dyn Fn(WorktreeChanged) + Send + Sync>;
/// worktree 폴더가 사라졌을 때 부른다 (`rm -rf`). 그 뒤 worker는 멈춘다
pub type GoneFn = Arc<dyn Fn() + Send + Sync>;

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
    spawn_watcher_with(wt, poll, emit, Arc::new(|| {}))
}

pub fn spawn_watcher_with(
    wt: Arc<Worktree>,
    poll: Duration,
    emit: EmitFn,
    on_gone: GoneFn,
) -> notify::Result<WatchHandle> {
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
    lookup_pr(&wt, &emit);
    std::thread::Builder::new()
        .name("wtboard-watch".into())
        .spawn(move || run_worker(wt, rx, poll, emit, on_gone))
        .map_err(notify::Error::io)?;
    Ok(WatchHandle { _debouncer: debouncer })
}

fn run_worker(wt: Arc<Worktree>, rx: mpsc::Receiver<Msg>, poll: Duration, emit: EmitFn, on_gone: GoneFn) {
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
        // `rm -rf`는 root 자신의 event 하나로 올 수 있고, 그 경로는 분류하면 Ignore다
        if !wt.layout().root.is_dir() {
            on_gone();
            return;
        }
        if targets.is_empty() {
            continue;
        }
        match wt.handle(&targets) {
            Ok(Some(ev)) => emit(ev),
            Ok(None) => {}
            Err(e) => eprintln!("[wtboard] {}: 재계산 실패: {e}", wt.id()),
        }
        // HEAD가 바뀌었으면(브랜치 전환) PR base를 다시 묻는다. 같은 브랜치면 claim이 막는다
        if targets.contains(&Target::Base) {
            lookup_pr(&wt, &emit);
        }
    }
}

/// 이 브랜치의 PR base를 아직 묻지 않았으면 background thread에서 gh를 부른다
fn lookup_pr(wt: &Arc<Worktree>, emit: &EmitFn) {
    if !wt.claim_pr_lookup() {
        return;
    }
    let (wt, emit) = (wt.clone(), emit.clone());
    let _ = std::thread::Builder::new().name("wtboard-gh".into()).spawn(move || match wt.refresh_pr_base() {
        Ok(Some(ev)) => emit(ev),
        Ok(None) => {}
        Err(e) => eprintln!("[wtboard] {}: PR base 반영 실패: {e}", wt.id()),
    });
}
