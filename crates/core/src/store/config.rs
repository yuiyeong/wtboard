use std::path::{Path, PathBuf};

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

impl Default for WatchConfig {
    fn default() -> Self {
        Self { fallback_poll_secs: 30 }
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
        home_dir().join(".wtboard/config.toml")
    }
}

impl RepoConfig {
    pub fn resolved_path(&self) -> PathBuf {
        expand_tilde(&self.path, &home_dir())
    }
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
