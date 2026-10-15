//! `gh` CLI로 현재 브랜치 PR의 base를 묻는다. network 호출이라 느릴 수 있고, 어떤 실패도 `None`이다.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5);
/// Finder에서 띄운 앱은 PATH에 Homebrew 경로가 없다
const FALLBACK_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

#[derive(Debug, Clone)]
pub struct Gh {
    bin: Option<PathBuf>,
    timeout: Duration,
}

impl Gh {
    pub fn detect() -> Self {
        let fallback: Vec<&Path> = FALLBACK_DIRS.iter().map(Path::new).collect();
        Self::with_bin(find_gh_in(std::env::var_os("PATH"), &fallback))
    }

    pub fn disabled() -> Self {
        Self::with_bin(None)
    }

    pub fn with_bin(bin: Option<PathBuf>) -> Self {
        Self { bin, timeout: TIMEOUT }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn enabled(&self) -> bool {
        self.bin.is_some()
    }

    /// 현재 브랜치 PR의 base 브랜치 이름 (`develop`). gh 없음, 로그인 안 됨, PR 없음, timeout이면 `None`
    pub fn pr_base(&self, cwd: &Path) -> Option<String> {
        let mut child = Command::new(self.bin.as_ref()?)
            .args(["pr", "view", "--json", "baseRefName", "--jq", ".baseRefName"])
            .current_dir(cwd)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1")
            // gh가 내부에서 부르는 git도 index lock을 잡지 않게 한다
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        }
        let out = child.wait_with_output().ok()?;
        if !out.status.success() {
            return None;
        }
        let out = String::from_utf8(out.stdout).ok()?;
        let name = out.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }
}

/// PATH의 디렉터리들, 그다음 `fallback_dirs`에서 `gh` 실행 파일을 찾는다
pub fn find_gh_in(path_env: Option<OsString>, fallback_dirs: &[&Path]) -> Option<PathBuf> {
    path_env
        .iter()
        .flat_map(std::env::split_paths)
        .chain(fallback_dirs.iter().map(|d| d.to_path_buf()))
        .map(|d| d.join("gh"))
        .find(|p| p.is_file())
}
