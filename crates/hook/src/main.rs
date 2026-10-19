//! Claude Code plugin hook이 부르는 바이너리. stdin의 hook 입력을 읽어 세션 기록
//! (`<data dir>/sessions/<session_id>/`)에 쓴다.
//!
//! 어떤 입력·오류에도 exit 0이고 stdout에는 아무것도 쓰지 않는다 (Claude Code가 stdout을 JSON으로
//! 해석하고, exit 2는 tool 호출을 막는다). 오류는 stderr 한 줄.
mod bash;
mod edit;
mod turn;

use std::error::Error;
use std::io::{self, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use wtboard_core::session::SessionDir;
use wtboard_core::store::data_dir;

type HookResult = Result<(), Box<dyn Error>>;
type Handler = fn(&SessionDir, &HookInput) -> HookResult;

/// hook 입력 중 쓰는 필드. 모르는 필드는 무시한다
#[derive(Debug, Deserialize)]
pub struct HookInput {
    pub session_id: String,
    pub cwd: PathBuf,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
    pub tool_use_id: Option<String>,
    /// subagent 안의 tool 호출일 때만 있다
    pub agent_id: Option<String>,
    /// SessionStart: startup | resume | clear | compact | fork
    pub source: Option<String>,
}

fn main() {
    // `args()`는 UTF-8이 아닌 인자에서 panic한다
    let sub = std::env::args_os()
        .nth(1)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    // 기본 panic hook은 여러 줄을 찍는다. 메시지는 아래에서 한 줄로 남긴다
    panic::set_hook(Box::new(|_| {}));
    let message = match panic::catch_unwind(AssertUnwindSafe(|| run(&sub))) {
        Ok(Ok(())) => return,
        Ok(Err(e)) => e.to_string(),
        Err(payload) => {
            let text = payload
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_default();
            format!("panic: {text}")
        }
    };
    // stderr가 닫혀 있어도 panic하지 않게 eprintln 대신 결과를 버린다
    let line = message.replace('\n', " ");
    let _ = writeln!(io::stderr(), "wtboard-hook {sub}: {line}");
}

fn run(sub: &str) -> HookResult {
    let handler: Handler = match sub {
        "session-start" => turn::session_start,
        "prompt-submit" => turn::prompt_submit,
        "turn-end" => turn::turn_end,
        "session-end" => turn::session_end,
        "pre-edit" => edit::pre_edit,
        "post-edit" => edit::post_edit,
        "pre-bash" => bash::pre_bash,
        "post-bash" => bash::post_bash,
        _ => return Err(format!("모르는 subcommand: {sub:?}").into()),
    };
    let mut stdin = Vec::new();
    io::stdin().read_to_end(&mut stdin)?;
    let input: HookInput = serde_json::from_slice(&stdin)?;
    let dir = SessionDir::open(&data_dir(), &input.session_id)
        .ok_or_else(|| format!("잘못된 session_id: {:?}", input.session_id))?;
    handler(&dir, &input)
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}
