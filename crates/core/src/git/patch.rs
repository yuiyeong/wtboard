use std::fs;
use std::path::Path;

use super::{Git, GitError, DIFF_FLAGS};
use crate::domain::{ChangedFile, FilePatch, FileStatus, PatchBody, MAX_DIFF_BYTES};

pub fn file_patch(git: &Git, merge_base: &str, file: &ChangedFile) -> Result<FilePatch, GitError> {
    let full_path = git.cwd().join(&file.path);
    // symlink는 따라가지 않는다: 링크 자체(대상 경로)의 크기
    let new_size = fs::symlink_metadata(&full_path).ok().map(|m| m.len());
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
    let diff = if file.status == FileStatus::Untracked && is_symlink(&full_path) {
        // `git diff --no-index`는 명령줄 인자의 symlink를 따라간다 (FIFO면 멈춤). git 형식으로 직접 만든다
        untracked_symlink_diff(&file.path, &full_path)
    } else if file.status == FileStatus::Untracked {
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

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

fn untracked_symlink_diff(rel: &str, path: &Path) -> Vec<u8> {
    let target = fs::read_link(path).map(|t| t.to_string_lossy().into_owned()).unwrap_or_default();
    format!(
        "diff --git a/{rel} b/{rel}\nnew file mode 120000\n--- /dev/null\n+++ b/{rel}\n@@ -0,0 +1 @@\n+{target}\n\\ No newline at end of file\n"
    )
    .into_bytes()
}

fn blob_size(git: &Git, merge_base: &str, path: &str) -> Option<u64> {
    git.run_str(&["cat-file", "-s", &format!("{merge_base}:{path}")]).ok()?.parse().ok()
}

fn content_hash(merge_base: &str, old_size: Option<u64>, path: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(merge_base.as_bytes());
    hasher.update(&old_size.unwrap_or(u64::MAX).to_le_bytes());
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if let Ok(target) = fs::read_link(path) {
                hasher.update(target.to_string_lossy().as_bytes());
            }
        }
        // 일반 파일만 연다. FIFO, 장치 파일은 열면 멈추거나 끝없이 읽힌다
        Ok(meta) if meta.is_file() => {
            if let Ok(file) = fs::File::open(path) {
                let _ = hasher.update_reader(file);
            }
        }
        _ => {}
    }
    hasher.finalize().to_hex().to_string()
}
