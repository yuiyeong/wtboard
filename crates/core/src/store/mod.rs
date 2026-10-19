pub mod config;
pub mod viewed;

use std::path::PathBuf;

use config::Config;

/// 앱과 hook이 함께 쓰는 폴더 (`state.sqlite`, `sessions/`). config 파일이 있는 폴더다
pub fn data_dir() -> PathBuf {
    let config = Config::default_path();
    match config.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}
