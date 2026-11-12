use std::path::PathBuf;

use super::{Git, GitError};

/// remote branch(이름순, `<remote>/HEAD` 제외) 다음 local branch(이름순). 예: `["origin/develop", "origin/main", "feat", "main"]`
pub fn base_candidates(git: &Git) -> Result<Vec<String>, GitError> {
    // `%(refname:short)`는 `refs/remotes/origin/HEAD`를 `origin`으로 줄이므로 full name에서 prefix를 직접 뗀다
    let out = git.run_str(&["for-each-ref", "--format=%(refname)", "refs/remotes", "refs/heads"])?;
    let (mut remote, mut local) = (Vec::new(), Vec::new());
    for refname in out.lines() {
        if let Some(name) = refname.strip_prefix("refs/remotes/") {
            if !name.ends_with("/HEAD") {
                remote.push(name.to_owned());
            }
        } else if let Some(name) = refname.strip_prefix("refs/heads/") {
            local.push(name.to_owned());
        }
    }
    remote.sort();
    local.sort();
    remote.extend(local);
    Ok(remote)
}

/// worktree root (`git rev-parse --show-toplevel`). worktree 안 어느 폴더에서 실행해도 같다
pub fn toplevel(git: &Git) -> Result<PathBuf, GitError> {
    git.run_str(&["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}
