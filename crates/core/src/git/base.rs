use super::{Git, GitError};

#[derive(Debug, thiserror::Error)]
pub enum BaseError {
    #[error(
        "base 브랜치를 정할 수 없음: ~/.wtboard/config.toml의 [[repo]]에 base를 적거나, \
         레포에서 `git remote set-head origin --auto`를 실행하세요"
    )]
    NotFound,
    #[error(transparent)]
    Git(#[from] GitError),
}

/// base 결정 순서: config의 `base` → `origin/HEAD`. (`gh pr view` 단계는 2단계에서 추가)
pub fn resolve_base(git: &Git, configured: Option<&str>) -> Result<String, BaseError> {
    if let Some(base) = configured {
        return Ok(base.to_owned());
    }
    git.run_str(&["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .map_err(|_| BaseError::NotFound)
}

pub fn merge_base(git: &Git, base: &str) -> Result<String, GitError> {
    git.run_str(&["merge-base", base, "HEAD"])
}
