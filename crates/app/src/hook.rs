//! 설치된 `wtboard-hook` binary가 이 앱의 schema와 맞는지 시작 때 한 번 확인한다. 옛 binary는 새 `state.sqlite`를
//! 열지 못해 자동 첨부가 조용히 멈추므로 sidebar 경고로 알린다. 앱이 hook을 실행하는 곳은 여기뿐이다
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use wtboard_core::store::db::{parse_hook_schema, SCHEMA_VERSION};

/// `version`은 stdin도 DB도 읽지 않아 바로 끝난다. 이보다 오래 걸리면 포기하고 경고한다 (멈춘 binary가 앱 시작을 막지 않게)
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(2);

/// binary가 없으면 없음 (hook을 안 쓰는 사용자). 있는데 이 schema를 말하지 않거나 실행할 수 없으면 경고 문구
pub fn check(bin: &Path) -> Option<String> {
    let detail = match Command::new(bin).arg("version").stdin(Stdio::null()).output() {
        Ok(out) if parse_hook_schema(&String::from_utf8_lossy(&out.stdout)) == Some(SCHEMA_VERSION) => return None,
        Ok(_) => String::new(),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => format!(" ({e})"),
    };
    Some(format!(
        "hook binary가 옛 version입니다 (schema {SCHEMA_VERSION} 필요). scripts/install-hook.sh를 다시 실행하세요. \
         그때까지 리뷰 코멘트가 첨부되지 않습니다{detail}"
    ))
}

/// `check`를 다른 thread에서 돌리고 `timeout` 안에 끝나지 않으면 그 사실을 경고한다. 그 thread는 버려진다
pub fn check_with_timeout(bin: PathBuf, timeout: Duration) -> Option<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(check(&bin));
    });
    rx.recv_timeout(timeout).unwrap_or_else(|_| {
        Some(format!(
            "hook binary 확인이 {}초 안에 끝나지 않았습니다. 옛 version이면 scripts/install-hook.sh를 다시 실행하세요",
            timeout.as_secs()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_is_silent_without_binary_and_warns_on_binary_without_schema() {
        assert_eq!(check(Path::new("/nonexistent/wtboard/bin/wtboard-hook")), None);
        let msg = check(Path::new("/usr/bin/true")).expect("schema를 말하지 않는 binary는 옛 binary다");
        assert!(msg.contains(&format!("schema {SCHEMA_VERSION} 필요")), "{msg}");
        assert!(msg.contains("scripts/install-hook.sh"), "{msg}");
        // 실행할 수 없는 파일 (폴더)은 오류를 덧붙인 경고
        let msg = check(Path::new("/usr/bin")).expect("폴더는 실행 오류");
        assert!(msg.contains("필요). ") && msg.ends_with(')'), "{msg}");
    }

    #[test]
    fn check_with_timeout_gives_up_on_hung_binary() {
        // `sleep`은 인자 `version`을 숫자로 읽지 못해 바로 끝나지만, 걸리는 binary를 흉내 내려면 sh script가 필요하다
        let dir = std::env::temp_dir().join(format!("wtboard-hook-hang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("hang.sh");
        std::fs::write(&bin, "#!/bin/sh\nsleep 5\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = std::time::Instant::now();
        let msg = check_with_timeout(bin.clone(), Duration::from_millis(300)).expect("timeout은 경고");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(msg.contains("끝나지 않"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
