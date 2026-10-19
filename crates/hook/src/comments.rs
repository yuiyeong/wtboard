//! `prompt-submit`에서 그 worktree의 미전송 리뷰 코멘트를 `additionalContext`로 붙인다 (spec 3절).
//! DB는 만들지 않고, 못 읽으면 아무것도 붙이지 않는다. 붙은 코멘트는 출력을 flush한 뒤에 전송됨으로 바꾼다
use std::io::Write;
use std::time::Duration;

use serde_json::json;
use wtboard_core::git::Git;
use wtboard_core::review::{self, system_message};
use wtboard_core::store::config::Config;
use wtboard_core::store::{data_dir, db};
use wtboard_core::watch::WatchLayout;

use crate::{now_ms, HookInput, HookResult};

/// 앱이 쓰는 중이어도 prompt를 오래 붙잡지 않는다
const BUSY: Duration = Duration::from_secs(1);

pub fn attach(input: &HookInput, out: &mut impl Write) -> HookResult {
    // `/compact` 같은 명령에 붙으면 반영되지 않은 채 전송됨이 된다
    if input.prompt.as_deref().is_some_and(is_slash_command) {
        return Ok(());
    }
    // config를 못 읽어도 켜진 것으로 본다
    if !Config::load(&Config::default_path()).map_or(true, |c| c.review.auto_attach_comments) {
        return Ok(());
    }
    // 앱과 같은 worktree id. git 레포가 아니면 붙일 곳이 없다
    let Ok(layout) = WatchLayout::detect(&Git::new(&input.cwd)) else { return Ok(()) };
    let Some(mut conn) = db::open_existing(&data_dir().join("state.sqlite"), BUSY)? else { return Ok(()) };
    let worktree = layout.root.to_string_lossy();
    review::attach(&mut conn, &worktree, &input.session_id, now_ms(), |rendered| {
        let output = json!({
            "systemMessage": system_message(rendered.included.len(), rendered.remaining),
            "hookSpecificOutput": { "hookEventName": "UserPromptSubmit", "additionalContext": rendered.text },
        });
        writeln!(out, "{output}")?;
        out.flush()
    })?;
    Ok(())
}

/// 첫 단어가 `/`로 시작하고 그 뒤에 `/`가 없으면 명령이다 (`/compact`, `/review 123`, `/plugin:cmd`).
/// `/Users/jo/x.rs 고쳐 줘`처럼 경로로 시작하는 prompt는 명령이 아니다
fn is_slash_command(prompt: &str) -> bool {
    let first = prompt.split_whitespace().next().unwrap_or("");
    first.strip_prefix('/').is_some_and(|rest| !rest.contains('/'))
}
