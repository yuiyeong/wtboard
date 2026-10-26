use std::fs;
use std::io;
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
    let old_source = match file.status {
        FileStatus::Added | FileStatus::Untracked => None,
        FileStatus::Modified | FileStatus::Deleted => {
            git.run(&["cat-file", "blob", &format!("{merge_base}:{}", file.path)]).ok()
        }
    };
    Ok(FilePatch {
        path: file.path.clone(),
        hash: blake3::hash(&diff).to_hex().to_string(),
        body: text_body(&diff, old_source),
    })
}

/// hash는 diff로만 만든다. `old_source`는 diff에서 정해지는 값이 아니라 표시용이다
fn text_body(diff: &[u8], old_source: Option<Vec<u8>>) -> PatchBody {
    PatchBody::Text {
        diff: String::from_utf8_lossy(diff).into_owned(),
        old_source: old_source.map(|b| String::from_utf8_lossy(&b).into_owned()),
    }
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

/// 두 파일을 `git diff --no-index`로 비교한다 (세션 view). `None`은 없는 파일(`/dev/null`).
/// before가 세션 폴더 안의 파일이어도 header 경로는 `a/<rel>`, `b/<rel>`로 바꾼다.
/// 1MB 초과·바이너리 판정과 hash는 `file_patch`와 같은 규칙이다
pub fn no_index_patch(
    git: &Git,
    rel: &str,
    before: Option<&Path>,
    after: Option<&Path>,
) -> Result<FilePatch, GitError> {
    let size = |p: Option<&Path>| p.and_then(|p| fs::metadata(p).ok()).map(|m| m.len());
    let (old_size, new_size) = (size(before), size(after));
    let too_large = old_size.max(new_size).is_some_and(|s| s > MAX_DIFF_BYTES);
    let diff = if too_large { Vec::new() } else { no_index(git, &[], before, after)? };
    if too_large || is_binary(&diff) {
        let digest = |p: Option<&Path>| p.and_then(|p| file_digest(p).ok().flatten());
        let hash = sides_hash([digest(before), digest(after)]);
        let body = if too_large {
            PatchBody::TooLarge { old_size, new_size }
        } else {
            PatchBody::Binary { old_size, new_size }
        };
        return Ok(FilePatch { path: rel.to_owned(), body, hash });
    }
    let diff = relabel(&diff, rel);
    Ok(FilePatch {
        path: rel.to_owned(),
        hash: blake3::hash(&diff).to_hex().to_string(),
        body: text_body(&diff, before.and_then(|p| fs::read(p).ok())),
    })
}

/// `git diff --no-index --numstat`의 (추가, 삭제) 줄 수. 바이너리면 `None`, 차이가 없으면 0
pub fn no_index_numstat(
    git: &Git,
    before: Option<&Path>,
    after: Option<&Path>,
) -> Result<(Option<u32>, Option<u32>), GitError> {
    let out = no_index(git, &["--numstat"], before, after)?;
    let line = out.split(|b| *b == b'\n').next().unwrap_or_default();
    if line.is_empty() {
        return Ok((Some(0), Some(0)));
    }
    let mut fields = line.split(|b| *b == b'\t').map(|f| std::str::from_utf8(f).ok()?.parse().ok());
    Ok((fields.next().flatten(), fields.next().flatten()))
}

/// `--no-index`는 차이가 있으면 exit 1
fn no_index(git: &Git, extra: &[&str], before: Option<&Path>, after: Option<&Path>) -> Result<Vec<u8>, GitError> {
    let arg = |p: Option<&Path>| p.map_or_else(|| "/dev/null".to_owned(), |p| p.to_string_lossy().into_owned());
    let (a, b) = (arg(before), arg(after));
    let mut args = vec!["diff"];
    args.extend(DIFF_FLAGS);
    args.extend(extra);
    args.extend(["--no-index", "--", a.as_str(), b.as_str()]);
    git.run_allow(&args, &[0, 1])
}

/// 첫 hunk 전 header에 `Binary files ... differ`가 있으면 바이너리
fn is_binary(diff: &[u8]) -> bool {
    diff.split(|b| *b == b'\n')
        .take_while(|l| !l.starts_with(b"@@"))
        .any(|l| l.starts_with(b"Binary files "))
}

/// header의 파일 경로(`diff --git`, `---`, `+++`)를 `rel`로 바꾼다. `/dev/null`은 그대로.
/// 세션 폴더의 내용 파일은 0600이라 mode 변화(`old mode`/`new mode`)는 뜻이 없으므로 뺀다
fn relabel(diff: &[u8], rel: &str) -> Vec<u8> {
    let (a, b) = (quote(&format!("a/{rel}")), quote(&format!("b/{rel}")));
    // git은 공백이 든 이름의 `---`/`+++` 줄 끝에 tab을 붙인다
    let tab = if rel.contains(' ') { "\t" } else { "" };
    let mut out = Vec::with_capacity(diff.len());
    let mut header = true;
    for line in diff.split_inclusive(|c| *c == b'\n') {
        header &= !line.starts_with(b"@@");
        let replaced = if !header {
            None
        } else if line.starts_with(b"diff --git ") {
            Some(format!("diff --git {a} {b}\n"))
        } else if line.starts_with(b"--- ") && !line.starts_with(b"--- /dev/null") {
            Some(format!("--- {a}{tab}\n"))
        } else if line.starts_with(b"+++ ") && !line.starts_with(b"+++ /dev/null") {
            Some(format!("+++ {b}{tab}\n"))
        } else if line.starts_with(b"old mode ") || line.starts_with(b"new mode ") {
            Some(String::new())
        } else {
            None
        };
        match replaced {
            Some(s) => out.extend_from_slice(s.as_bytes()),
            None => out.extend_from_slice(line),
        }
    }
    out
}

/// git의 C-style 경로 quote (`core.quotePath=false`): `"`, `\`, 제어 문자가 있을 때만 감싼다
fn quote(name: &str) -> String {
    if !name.chars().any(|c| c == '"' || c == '\\' || c.is_ascii_control()) {
        return name.to_owned();
    }
    let mut out = String::from("\"");
    for c in name.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\x0b' => out.push_str("\\v"),
            '\x0c' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if c.is_ascii_control() => out.push_str(&format!("\\{:03o}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// 읽은 바이트 수와 blake3 hash (hex). 세션 기록의 `too_large` 원본·스냅샷도 이 값을 남긴다
pub(crate) fn hash_reader(src: impl io::Read) -> io::Result<(u64, String)> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(src)?;
    Ok((hasher.count(), hasher.finalize().to_hex().to_string()))
}

/// 파일 내용의 (크기, blake3 hash). 일반 파일이 아니거나 읽지 못하면 `None`
pub(crate) fn file_digest(path: &Path) -> io::Result<Option<(u64, String)>> {
    match fs::File::open(path) {
        Ok(f) => hash_reader(f).map(Some),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// 세션 view에서 바이너리/1MB 초과 patch의 hash. 양쪽의 (크기, 내용 hash)로 만든다. 없는 쪽은 표시값.
/// 내용 hash가 세션 기록의 `too_large` hash와 같은 방식이라, 같은 내용이면 내용 파일에서 왔든 기록에서 왔든 같은 hash다
pub(crate) fn sides_hash(sides: [Option<(u64, String)>; 2]) -> String {
    let mut hasher = blake3::Hasher::new();
    for side in &sides {
        match side {
            Some((size, hash)) => {
                hasher.update(&size.to_le_bytes());
                hasher.update(hash.as_bytes());
            }
            None => {
                hasher.update(&u64::MAX.to_le_bytes());
            }
        }
    }
    hasher.finalize().to_hex().to_string()
}
