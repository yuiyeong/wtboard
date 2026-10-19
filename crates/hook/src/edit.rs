//! Edit·Write·NotebookEdit tool 전후. 원본(baseline)을 남기고 `edit` event를 쓴다
use std::path::PathBuf;

use wtboard_core::session::{valid_abs_path, BaselineContent, Event, EventKind, SessionDir};

use crate::{now_ms, HookInput, HookResult};

/// tool이 건드리는 파일. Edit·Write는 `file_path`, NotebookEdit은 `notebook_path`
fn tool_path(input: &HookInput) -> Option<PathBuf> {
    let tool_input = input.tool_input.as_ref()?;
    let path = tool_input.get("file_path").or_else(|| tool_input.get("notebook_path"))?;
    path.as_str().map(PathBuf::from)
}

/// 경로의 원본이 없으면 지금 내용으로 만든다. 파일이 없으면 `new`
pub fn pre_edit(dir: &SessionDir, input: &HookInput) -> HookResult {
    let raw = tool_path(input).ok_or("tool_input에 파일 경로가 없음")?;
    let path = valid_abs_path(&raw).ok_or_else(|| format!("기록하지 않는 경로: {}", raw.display()))?;
    dir.ensure_baseline(&path, BaselineContent::CopyOf(&path))?;
    Ok(())
}

/// tool이 실패해도 (PostToolUseFailure) 쓴다. 파일이 일부 바뀌었을 수 있다
pub fn post_edit(dir: &SessionDir, input: &HookInput) -> HookResult {
    let turn = dir.read_meta().map_or(0, |m| m.turn);
    dir.append_event(&Event {
        t: now_ms(),
        turn,
        kind: EventKind::Edit,
        tool: input.tool_name.clone(),
        paths: tool_path(input).into_iter().collect(),
        agent_id: input.agent_id.clone(),
    })?;
    Ok(())
}
