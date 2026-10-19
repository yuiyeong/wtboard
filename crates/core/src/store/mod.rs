pub mod comments;
pub mod config;
pub mod db;
pub mod settings;
pub mod viewed;

use std::path::PathBuf;

use config::Config;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("저장 폴더를 만들 수 없음: {0}")]
    Io(#[from] std::io::Error),
    #[error("상태 DB 오류: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("상태 DB가 이 앱보다 새 version({0})이다")]
    NewerSchema(i32),
    #[error("코멘트 내용이 올바르지 않음: {0}")]
    InvalidBody(String),
    #[error("이미 전송된 코멘트는 고칠 수 없음")]
    AlreadySent,
    #[error("첨부 텍스트를 내보내지 못함: {0}")]
    Emit(std::io::Error),
    #[error("코멘트를 찾을 수 없음: {0}")]
    NotFound(i64),
}

/// 앱과 hook이 함께 쓰는 폴더 (`state.sqlite`, `sessions/`). config 파일이 있는 폴더다
pub fn data_dir() -> PathBuf {
    let config = Config::default_path();
    match config.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    }
}
