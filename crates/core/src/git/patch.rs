use std::fs;
use std::path::Path;

use super::{Git, GitError, DIFF_FLAGS};
use crate::domain::{ChangedFile, FilePatch, FileStatus, PatchBody, MAX_DIFF_BYTES};

pub fn file_patch(git: &Git, merge_base: &str, file: &ChangedFile) -> Result<FilePatch, GitError> {
    let full_path = git.cwd().join(&file.path);
    let new_size = fs::metadata(&full_path).ok().filter(|m| m.is_file()).map(|m| m.len());
    let old_size = match file.status {
        FileStatus::Added | FileStatus::Untracked => None,
        FileStatus::Modified | FileStatus::Deleted => blob_size(git, merge_base, &file.path),
    };
    let too_large = old_size.max(new_size).is_some_and(|s| s > MAX_DIFF_BYTES);

    if too_large || file.additions.is_none() {
        let hash = content_hash(merge_base, old_size, &full_path);
        let body = if too_large {
            PatchBody::TooLarge { old_size, new_size }
        } else {
            PatchBody::Binary { old_size, new_size }
        };
        return Ok(FilePatch { path: file.path.clone(), body, hash });
    }

    let mut args = vec!["diff"];
    args.extend(DIFF_FLAGS);
    let diff = if file.status == FileStatus::Untracked {
        args.extend(["--no-index", "--", "/dev/null", file.path.as_str()]);
        git.run_allow(&args, &[0, 1])? // --no-index는 차이가 있으면 exit 1
    } else {
        args.extend([merge_base, "--", file.path.as_str()]);
        git.run(&args)?
    };
    Ok(FilePatch {
        path: file.path.clone(),
        hash: blake3::hash(&diff).to_hex().to_string(),
        body: PatchBody::Text { diff: String::from_utf8_lossy(&diff).into_owned() },
    })
}

fn blob_size(git: &Git, merge_base: &str, path: &str) -> Option<u64> {
    git.run_str(&["cat-file", "-s", &format!("{merge_base}:{path}")]).ok()?.parse().ok()
}

fn content_hash(merge_base: &str, old_size: Option<u64>, path: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(merge_base.as_bytes());
    hasher.update(&old_size.unwrap_or(u64::MAX).to_le_bytes());
    if let Ok(file) = fs::File::open(path) {
        let _ = hasher.update_reader(file);
    }
    hasher.finalize().to_hex().to_string()
}
