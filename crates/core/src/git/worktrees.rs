use std::path::PathBuf;

use super::{Git, GitError};
use crate::paths::canonical;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    /// canonical + NFC. `Worktree::id()`와 같은 형태
    pub path: PathBuf,
    /// `None`이면 detached HEAD
    pub branch: Option<String>,
}

/// `git worktree list --porcelain -z`. bare, prunable(폴더가 지워짐), 폴더가 없는 entry는 뺀다.
/// git 출력 순서를 따른다 (main worktree가 먼저). common dir에서 실행해도 된다.
pub fn list_worktrees(git: &Git) -> Result<Vec<WorktreeEntry>, GitError> {
    let out = git.run(&["worktree", "list", "--porcelain", "-z"])?;
    let text = String::from_utf8_lossy(&out);
    // 한 entry는 `key value` field들을 NUL로 잇고, entry 사이는 빈 field (NUL 두 개)
    Ok(text
        .split("\0\0")
        .filter_map(|record| {
            let (mut path, mut branch, mut skip) = (None, None, false);
            for field in record.split('\0') {
                let (key, value) = field.split_once(' ').unwrap_or((field, ""));
                match key {
                    "worktree" => path = Some(PathBuf::from(value)),
                    "branch" => branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).to_owned()),
                    "bare" | "prunable" => skip = true,
                    _ => {}
                }
            }
            let path = path?;
            (!skip && path.is_dir()).then(|| WorktreeEntry { path: canonical(&path), branch })
        })
        .collect())
}

/// 모든 worktree가 같이 쓰는 git 디렉터리 (`<main>/.git`, bare면 그 자신)
pub fn common_dir(git: &Git) -> Result<PathBuf, GitError> {
    let out = git.run_str(&["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    Ok(canonical(out))
}
