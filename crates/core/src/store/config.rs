use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("설정 파일을 읽을 수 없음: {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("설정 파일 형식 오류: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default, rename = "repo")]
    pub repos: Vec<RepoConfig>,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub sessions: SessionsConfig,
    #[serde(default)]
    pub review: ReviewConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoConfig {
    pub path: String,
    pub base: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct WatchConfig {
    pub fallback_poll_secs: u64,
}

/// 0 같은 값이면 worker가 쉬지 않고 재계산한다
const MIN_POLL_SECS: u64 = 5;

impl WatchConfig {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.fallback_poll_secs.max(MIN_POLL_SECS))
    }
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self { fallback_poll_secs: 30 }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SessionsConfig {
    /// 마지막 활동이 이보다 오래된 세션 기록은 앱 시작 때 지운다
    pub keep_days: u64,
}

impl Default for SessionsConfig {
    fn default() -> Self {
        Self { keep_days: 7 }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ReviewConfig {
    /// false면 hook이 prompt에 리뷰 코멘트를 붙이지 않는다
    pub auto_attach_comments: bool,
}

impl Default for ReviewConfig {
    fn default() -> Self {
        Self { auto_attach_comments: true }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(s)?)
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let s = std::fs::read_to_string(path)
            .map_err(|source| ConfigError::Io { path: path.to_path_buf(), source })?;
        Self::parse(&s)
    }

    pub fn default_path() -> PathBuf {
        config_path(std::env::var_os("WTBOARD_CONFIG"), &home_dir())
    }
}

impl RepoConfig {
    pub fn resolved_path(&self) -> PathBuf {
        expand_tilde(&self.path, &home_dir())
    }
}

/// `WTBOARD_CONFIG`가 있으면 그 경로, 없으면 `~/.wtboard/config.toml`
pub fn config_path(env: Option<OsString>, home: &Path) -> PathBuf {
    env.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".wtboard/config.toml"))
}

/// Claude Code plugin(`run.sh`)이 실행하는 hook binary. `WTBOARD_HOOK_BIN`이 비어 있지 않으면 그것,
/// 없으면 `~/.wtboard/bin/wtboard-hook` (`scripts/install-hook.sh`가 설치하는 곳)
pub fn hook_binary_path(env: Option<OsString>, home: &Path) -> PathBuf {
    env.filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".wtboard/bin/wtboard-hook"))
}

pub fn hook_binary() -> PathBuf {
    hook_binary_path(std::env::var_os("WTBOARD_HOOK_BIN"), &home_dir())
}

pub fn expand_tilde(p: &str, home: &Path) -> PathBuf {
    if p == "~" {
        home.to_path_buf()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(p)
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}
