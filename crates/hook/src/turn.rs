//! 세션·turn 경계. `meta.json`은 이 hook들만 쓴다 (한 세션 안에서 겹치지 않는다)
use std::collections::BTreeSet;
use std::path::PathBuf;

use wtboard_core::git::Git;
use wtboard_core::session::{valid_abs_path, Event, EventKind, Meta, SessionDir};

use crate::{now_ms, HookInput, HookResult};

/// meta가 없으면 만든다. resume 등으로 이미 있으면 `ended_at`만 지운다
pub fn session_start(dir: &SessionDir, input: &HookInput) -> HookResult {
    match dir.read_meta() {
        Some(mut meta) if meta.ended_at.is_some() => {
            meta.ended_at = None;
            dir.write_meta(&meta)?;
        }
        Some(_) => {}
        None => dir.write_meta(&new_meta(input))?,
    }
    Ok(())
}

/// 열린 turn을 닫고 다음 turn을 연다. SessionStart를 놓친 세션 (plugin을 세션 중간에 켬)은 여기서 meta를 만든다
pub fn prompt_submit(dir: &SessionDir, input: &HookInput) -> HookResult {
    let mut meta = dir.read_meta().unwrap_or_else(|| new_meta(input));
    // 스냅샷을 못 찍어도 turn은 넘긴다. 아니면 같은 turn에 계속 머문다. 오류는 끝에 알린다
    let closed = close_turn(dir, &mut meta);
    meta.turn += 1;
    meta.turn_open = true;
    dir.write_meta(&meta)?;
    dir.append_event(&turn_event(meta.turn, EventKind::TurnStart))?;
    closed
}

pub fn turn_end(dir: &SessionDir, _input: &HookInput) -> HookResult {
    let Some(mut meta) = dir.read_meta() else { return Ok(()) };
    if !meta.turn_open {
        return Ok(());
    }
    let closed = close_turn(dir, &mut meta);
    dir.write_meta(&meta)?;
    closed
}

pub fn session_end(dir: &SessionDir, input: &HookInput) -> HookResult {
    let mut meta = dir.read_meta().unwrap_or_else(|| new_meta(input));
    let closed = close_turn(dir, &mut meta);
    meta.ended_at = Some(now_ms());
    dir.write_meta(&meta)?;
    closed
}

/// `project_dir`은 `CLAUDE_PROJECT_DIR`, 없으면 `cwd`. `start_head`는 그 폴더의 HEAD (레포가 아니면 없음)
fn new_meta(input: &HookInput) -> Meta {
    let project_dir = std::env::var_os("CLAUDE_PROJECT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| input.cwd.clone());
    let start_head = Git::new(&project_dir).run_str(&["rev-parse", "HEAD"]).ok();
    Meta {
        started_at: now_ms(),
        // 절대 경로면 NFC로. Claude Code는 절대 경로를 준다
        project_dir: valid_abs_path(&project_dir).unwrap_or(project_dir),
        start_head,
        turn: 0,
        turn_open: false,
        ended_at: None,
    }
}

/// 열린 turn이면 그 turn이 건드린 경로를 `turns/<n>/`에 찍고 `turn_end` event를 쓴다.
/// 스냅샷을 못 찍어도 turn은 닫고 그 오류를 돌려준다 (앱은 스냅샷 없는 turn을 현재 파일로 본다).
/// meta는 바꾸기만 하고 쓰지 않는다 (부른 쪽이 한 번에 쓰고 오류는 그 뒤에 돌려준다)
fn close_turn(dir: &SessionDir, meta: &mut Meta) -> HookResult {
    if !meta.turn_open {
        return Ok(());
    }
    // 앱은 turn 0 (첫 prompt 전)을 turn 1에 합친다. turn 1 스냅샷에 turn 0 경로도 넣어야
    // 다음 turn의 비교 전(before)이 원본까지 거슬러 가지 않는다
    let in_turn = |t: u32| t == meta.turn || (meta.turn == 1 && t == 0);
    let paths: BTreeSet<PathBuf> = dir
        .events()
        .into_iter()
        .filter(|e| in_turn(e.turn) && matches!(e.kind, EventKind::Edit | EventKind::Bash))
        .flat_map(|e| e.paths)
        .collect();
    let snapshot = dir.snapshot_turn(meta.turn, &paths.into_iter().collect::<Vec<_>>());
    dir.append_event(&turn_event(meta.turn, EventKind::TurnEnd))?;
    meta.turn_open = false;
    Ok(snapshot?)
}

fn turn_event(turn: u32, kind: EventKind) -> Event {
    Event { t: now_ms(), turn, kind, tool: None, paths: Vec::new(), agent_id: None }
}
