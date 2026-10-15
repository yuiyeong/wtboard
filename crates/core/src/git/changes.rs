use std::collections::HashMap;
use std::fs;
use std::path::Path;

use super::{Git, GitError, DIFF_FLAGS};
use crate::domain::{ChangedFile, FileStatus, MAX_DIFF_BYTES};

/// merge-base 대비 작업 트리의 변경 파일 (커밋됨 + 커밋 안 됨 + untracked).
/// `paths`가 비어 있으면 전체, 아니면 그 경로들로 제한한다 (literal pathspec).
pub fn list_changes(git: &Git, merge_base: &str, paths: &[&str]) -> Result<Vec<ChangedFile>, GitError> {
    let status = git.run(&diff_args(&["--name-status", "-z", merge_base], paths))?;
    let numstat = git.run(&diff_args(&["--numstat", "-z", merge_base], paths))?;
    let mut others_args = vec!["ls-files", "--others", "--exclude-standard", "-z", "--"];
    others_args.extend(paths);
    let others = git.run(&others_args)?;

    let counts = parse_numstat(&numstat);
    let mut files: Vec<ChangedFile> = parse_name_status(&status)
        .into_iter()
        .map(|(status, path)| {
            let (additions, deletions) = counts.get(&path).copied().unwrap_or((None, None));
            ChangedFile { path, status, additions, deletions }
        })
        .collect();
    for path in split_z(&others) {
        let additions = count_text_lines(&git.cwd().join(&path));
        files.push(ChangedFile { path, status: FileStatus::Untracked, additions, deletions: additions.map(|_| 0) });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn diff_args<'a>(extra: &[&'a str], paths: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["diff"];
    args.extend(DIFF_FLAGS);
    args.extend(extra);
    args.push("--");
    args.extend(paths);
    args
}

fn split_z(buf: &[u8]) -> Vec<String> {
    buf.split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect()
}

/// `--name-status -z --no-renames` 출력: `M\0path\0A\0path\0...`
fn parse_name_status(buf: &[u8]) -> Vec<(FileStatus, String)> {
    split_z(buf)
        .chunks(2)
        .filter_map(|pair| match pair {
            [code, path] => Some((status_from_code(code), path.clone())),
            _ => None,
        })
        .collect()
}

fn status_from_code(code: &str) -> FileStatus {
    match code.as_bytes().first() {
        Some(b'A') => FileStatus::Added,
        Some(b'D') => FileStatus::Deleted,
        _ => FileStatus::Modified,
    }
}

/// `--numstat -z --no-renames` 출력: `added\tdeleted\tpath\0`. 바이너리는 `-\t-\t`
fn parse_numstat(buf: &[u8]) -> HashMap<String, (Option<u32>, Option<u32>)> {
    split_z(buf)
        .into_iter()
        .filter_map(|rec| {
            let mut parts = rec.splitn(3, '\t');
            let added = parts.next()?.parse().ok();
            let deleted = parts.next()?.parse().ok();
            let path = parts.next()?.to_owned();
            Some((path, (added, deleted)))
        })
        .collect()
}

/// untracked 텍스트 파일의 줄 수. 바이너리이거나 너무 크면 `None`.
/// symlink는 따라가지 않는다 (git처럼 링크 대상 경로 한 줄로 본다). FIFO 같은 특수 파일은 읽지 않는다.
fn count_text_lines(path: &Path) -> Option<u32> {
    let meta = fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        return Some(1);
    }
    if !meta.is_file() || meta.len() > MAX_DIFF_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    let newlines = bytes.iter().filter(|b| **b == b'\n').count();
    let trailing = usize::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
    Some((newlines + trailing) as u32)
}
