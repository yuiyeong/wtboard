use serde::Serialize;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseSource {
    Config,
    Pr,
    Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub name: String,
    pub source: BaseSource,
}

/// base 결정 순서: config의 `base` → PR base(`remote_branch`로 확인한 것) → `origin/HEAD`
pub fn resolve_base(git: &Git, configured: Option<&str>, pr: Option<&str>) -> Result<Base, BaseError> {
    if let Some(name) = configured {
        return Ok(Base { name: name.to_owned(), source: BaseSource::Config });
    }
    if let Some(name) = pr {
        return Ok(Base { name: name.to_owned(), source: BaseSource::Pr });
    }
    let name = git
        .run_str(&["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .map_err(|_| BaseError::NotFound)?;
    Ok(Base { name, source: BaseSource::Origin })
}

/// PR base 브랜치 이름(`develop`)을 로컬에 있는 remote 브랜치(`origin/develop`)로 바꾼다. 없으면 `None`
pub fn remote_branch(git: &Git, branch: &str) -> Option<String> {
    let name = format!("origin/{branch}");
    git.run(&["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{name}")])
        .ok()
        .map(|_| name)
}

pub fn merge_base(git: &Git, base: &str) -> Result<String, GitError> {
    git.run_str(&["merge-base", base, "HEAD"])
}
