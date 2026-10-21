//! 설치된 `wtboard-hook` binary가 이 앱의 schema와 맞는지 시작 때 한 번 확인한다. 옛 binary는 새 `state.sqlite`를
//! 열지 못해 자동 첨부가 조용히 멈추므로 sidebar 경고로 알린다. 앱이 hook을 실행하는 곳은 여기뿐이다
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use wtboard_core::store::db::SCHEMA_VERSION;

/// plugin의 run.sh와 같은 규칙: `WTBOARD_HOOK_BIN`, 없으면 `~/.wtboard/bin/wtboard-hook`
pub fn hook_binary() -> PathBuf {
    if let Some(bin) = std::env::var_os("WTBOARD_HOOK_BIN").filter(|v| !v.is_empty()) {
        return PathBuf::from(bin);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"));
    home.join(".wtboard/bin/wtboard-hook")
}

/// `wtboard-hook version` 출력(`wtboard-hook 0.1.0 schema 2`)의 schema 번호
pub fn parse_schema(stdout: &str) -> Option<i32> {
    let mut words = stdout.split_whitespace();
    words.find(|w| *w == "schema")?;
    words.next()?.parse().ok()
}

/// binary가 없으면 없음 (hook을 안 쓰는 사용자). 있는데 이 schema를 말하지 않으면 경고 문구
pub fn check(bin: &Path) -> Option<String> {
    if !bin.exists() {
        return None;
    }
    let warning = format!(
        "hook binary가 옛 version입니다 (schema {SCHEMA_VERSION} 필요). scripts/install-hook.sh를 다시 실행하세요. \
         그때까지 리뷰 코멘트가 첨부되지 않습니다"
    );
    match Command::new(bin).arg("version").stdin(Stdio::null()).output() {
        Ok(out) if parse_schema(&String::from_utf8_lossy(&out.stdout)) == Some(SCHEMA_VERSION) => None,
        Ok(_) => Some(warning),
        Err(e) => Some(format!("{warning} ({e})")),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parse_schema_reads_number_after_schema() {
        assert_eq!(parse_schema("wtboard-hook 0.1.0 schema 2\n"), Some(2));
        assert_eq!(parse_schema(""), None);
        assert_eq!(parse_schema("schema x"), None);
        assert_eq!(parse_schema("wtboard-hook prompt-submit: 모르는 subcommand"), None);
    }

    #[test]
    fn check_is_silent_without_binary_and_warns_on_binary_without_schema() {
        assert_eq!(check(Path::new("/nonexistent/wtboard/bin/wtboard-hook")), None);
        let msg = check(Path::new("/usr/bin/true")).expect("schema를 말하지 않는 binary는 옛 binary다");
        assert!(msg.contains("schema 2 필요"), "{msg}");
        assert!(msg.contains("scripts/install-hook.sh"), "{msg}");
    }
}
