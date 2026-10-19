//! Bash tool 전후. Bash가 어떤 파일을 고칠지 미리 알 수 없으므로 전후 `git status`를 비교해
//! 바뀐 파일을 찾는다. git 레포 밖의 Bash와 ignored 파일은 잡지 않는다
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use wtboard_core::git::{Git, GitError};
use wtboard_core::session::{valid_abs_path, BaselineContent, Event, EventKind, SessionDir};

use crate::{now_ms, HookInput, HookResult};

/// `bash/<tool_use_id>.json`. pre-bash 시점의 레포 상태
#[derive(Debug, Serialize, Deserialize)]
struct StatusSnapshot {
    root: PathBuf,
    head: String,
    entries: Entries,
}

/// git status에 나온 경로 하나. 파일이 없으면 (삭제) mtime·size는 `None`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    /// porcelain v2의 XY. untracked는 `??`
    status: String,
    mtime_ns: Option<u64>,
    size: Option<u64>,
}

/// root 상대 경로 → 상태
type Entries = BTreeMap<String, Entry>;

/// `cwd`의 레포 root·HEAD·status를 저장한다. 레포가 아니거나 `tool_use_id`가 없으면 아무것도 안 한다
pub fn pre_bash(dir: &SessionDir, input: &HookInput) -> HookResult {
    let Some(id) = &input.tool_use_id else { return Ok(()) };
    let Ok(out) = Git::new(&input.cwd).run_str(&["rev-parse", "--show-toplevel", "HEAD"]) else {
        return Ok(());
    };
    let Some((root, head)) = out.split_once('\n') else { return Ok(()) };
    let root = PathBuf::from(root);
    let entries = status(&root)?;
    let snapshot = StatusSnapshot { root, head: head.to_string(), entries };
    dir.write_bash_snapshot(id, &serde_json::to_vec(&snapshot)?)?;
    Ok(())
}

/// pre-bash 스냅샷과 지금 status를 비교해 바뀐 파일마다 원본을 남기고 `bash` event를 쓴다.
/// 스냅샷이 없으면 (레포 밖, `tool_use_id` 없음) 아무것도 안 한다. tool이 실패해도 (PostToolUseFailure) 같다
pub fn post_bash(dir: &SessionDir, input: &HookInput) -> HookResult {
    let Some(id) = &input.tool_use_id else { return Ok(()) };
    let Some(bytes) = dir.take_bash_snapshot(id) else { return Ok(()) };
    let before: StatusSnapshot = serde_json::from_slice(&bytes)?;
    let git = Git::new(&before.root);
    let after = status(&before.root)?;
    let committed = committed(&git, &before.head).unwrap_or_default();
    let meta = dir.read_meta();
    let start_head = meta.as_ref().and_then(|m| m.start_head.as_deref());

    let mut paths = Vec::new();
    // 원본이 아직 없는 경로와 그 원본을 읽을 커밋
    let mut todo = Vec::new();
    for (rel, change) in changed(&before.entries, &after, committed) {
        let Some(path) = valid_abs_path(&before.root.join(&rel)) else { continue };
        // 이미 원본이 있으면 내용을 읽지 않는다. 경쟁은 `ensure_baseline`이 가린다
        if !dir.baseline_file(&path).exists() {
            let rev = match change {
                Change::Clean => Some(before.head.as_str()),
                Change::Unknown => start_head,
            };
            todo.push((path.clone(), change, rev.map(|r| format!("{r}:{rel}"))));
        }
        paths.push(path);
    }

    let mut first_error = None;
    let specs: Vec<_> = todo.iter().map(|(_, _, spec)| spec.as_deref()).collect();
    match read_blobs(&git, &specs) {
        Ok(blobs) => {
            for ((path, change, _), blob) in todo.iter().zip(&blobs) {
                let bytes = blob.as_deref();
                let content = match change {
                    // pre HEAD에 없으면 Bash 전에 없던 파일이다
                    Change::Clean => bytes.map_or(BaselineContent::New, BaselineContent::Bytes),
                    Change::Unknown => {
                        BaselineContent::Uncertain { head: start_head.map(String::from), bytes }
                    }
                };
                if let Err(e) = dir.ensure_baseline(path, content) {
                    first_error.get_or_insert(e.into());
                }
            }
        }
        // 원본을 못 읽으면 기록하지 않는다 (다음 tool이 다시 기록할 수 있다). event는 남긴다
        Err(e) => first_error = Some(e.into()),
    }
    if !paths.is_empty() {
        dir.append_event(&Event {
            t: now_ms(),
            turn: meta.as_ref().map_or(0, |m| m.turn),
            kind: EventKind::Bash,
            tool: Some("Bash".to_string()),
            paths,
            agent_id: input.agent_id.clone(),
        })?;
    }
    first_error.map_or(Ok(()), Err)
}

/// 바뀐 파일의 Bash 전 내용을 얼마나 아는지
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// Bash 전 status에 없었다: 깨끗한 tracked 파일 (내용 = pre HEAD) 또는 없던 파일
    Clean,
    /// Bash 전에 이미 바뀌어 있었다 (재수정, 되돌림, 커밋, 삭제)
    Unknown,
}

/// 설계 3절 "post-bash" 표. S에 없고 T에 있으면 깨끗했던 파일, 둘 다 있으면 mtime·size가 다를 때,
/// S에만 있으면 (되돌림·커밋·삭제) 항상 바뀐 것으로 본다.
/// Bash 중 바뀐 HEAD 사이의 경로(`committed`)도 S에 없었으면 깨끗했다가 바뀐 파일이다
/// (고치고 커밋하면 전후 status 어디에도 안 나온다)
fn changed(
    before: &Entries,
    after: &Entries,
    committed: impl IntoIterator<Item = String>,
) -> BTreeMap<String, Change> {
    let mut out = BTreeMap::new();
    for (rel, now) in after {
        let change = match before.get(rel) {
            None => Change::Clean,
            Some(was) if (was.mtime_ns, was.size) != (now.mtime_ns, now.size) => Change::Unknown,
            Some(_) => continue,
        };
        out.insert(rel.clone(), change);
    }
    for rel in before.keys().filter(|rel| !after.contains_key(*rel)) {
        out.insert(rel.clone(), Change::Unknown);
    }
    for rel in committed {
        if !before.contains_key(&rel) {
            out.insert(rel, Change::Clean);
        }
    }
    out
}

/// Bash 중 HEAD가 바뀌었으면 두 커밋 사이에 바뀐 경로. 그대로면 빈 목록.
/// 실패는 (Bash가 HEAD를 unborn으로 만듦 등) 부른 쪽이 빈 목록으로 본다. status 비교 결과는 그대로 쓴다
fn committed(git: &Git, pre_head: &str) -> Result<Vec<String>, GitError> {
    let head = git.run_str(&["rev-parse", "HEAD"])?;
    if head == pre_head {
        return Ok(Vec::new());
    }
    let out = git.run(&["diff", "--name-only", "-z", "--no-renames", pre_head, &head])?;
    Ok(out
        .split(|b| *b == 0)
        .filter_map(|p| std::str::from_utf8(p).ok())
        .filter(|p| !p.is_empty())
        .map(String::from)
        .collect())
}

/// `<rev>:<path>`마다 blob 내용을 `git cat-file --batch` 한 번으로 읽는다. 결과는 `specs`와 같은 순서이고
/// spec이 없거나, 그 커밋에 경로가 없거나, blob이 아니면 `None`
fn read_blobs(git: &Git, specs: &[Option<&str>]) -> Result<Vec<Option<Vec<u8>>>, GitError> {
    // 입력은 줄 단위라 줄바꿈이 든 경로는 보내지 않는다
    let sent: Vec<&str> = specs.iter().flatten().copied().filter(|s| !s.contains('\n')).collect();
    if sent.is_empty() {
        return Ok(vec![None; specs.len()]);
    }
    let input: String = sent.iter().map(|s| format!("{s}\n")).collect();
    let out = git.run_with_input(&["cat-file", "--batch"], input.as_bytes())?;
    let mut blobs = parse_batch(&out).into_iter();
    Ok(specs
        .iter()
        .map(|spec| match spec {
            Some(s) if !s.contains('\n') => blobs.next().flatten(),
            _ => None,
        })
        .collect())
}

/// `cat-file --batch` 출력. 객체는 `<oid> <type> <size>\n<내용>\n`, 없으면 `<입력> missing\n`처럼
/// 내용 없이 한 줄이다 (`submodule`도 내용 없음). 헤더 끝이 크기면 내용이 따라온다
fn parse_batch(out: &[u8]) -> Vec<Option<Vec<u8>>> {
    let mut rows = Vec::new();
    let mut rest = out;
    while let Some(nl) = rest.iter().position(|b| *b == b'\n') {
        let header = String::from_utf8_lossy(&rest[..nl]);
        rest = &rest[nl + 1..];
        let mut tokens = header.rsplitn(3, ' ');
        let (size, kind) = (tokens.next(), tokens.next());
        match size.and_then(|s| s.parse::<usize>().ok()) {
            Some(size) if tokens.next().is_some() && size < rest.len() => {
                let body = &rest[..size];
                rows.push((kind == Some("blob")).then(|| body.to_vec()));
                rest = &rest[size + 1..];
            }
            _ => rows.push(None),
        }
    }
    rows
}

/// `git status --porcelain=v2 -z --untracked-files=all`의 경로마다 status와 (mtime, size)
fn status(root: &Path) -> Result<Entries, GitError> {
    let out = Git::new(root).run(&["status", "--porcelain=v2", "-z", "--untracked-files=all"])?;
    Ok(parse_status(&out)
        .into_iter()
        .map(|(rel, status)| {
            // 심볼릭 링크는 링크 자체를 본다 (git이 링크를 추적한다)
            let meta = fs::symlink_metadata(root.join(&rel)).ok();
            let mtime_ns = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| {
                t.duration_since(UNIX_EPOCH).ok().map(|d| d.as_nanos() as u64)
            });
            let entry = Entry { status, mtime_ns, size: meta.map(|m| m.len()) };
            (rel, entry)
        })
        .collect())
}

/// porcelain v2 `-z`의 `1`(변경)·`2`(rename)·`u`(충돌)·`?`(untracked) 줄을 (경로, XY)로.
/// rename은 새 경로와 원래 경로(다음 NUL 조각) 둘 다 낸다. 같은 경로가 두 줄로 나올 수 있다
/// (`git rm --cached`: `1 D.`와 `?`). UTF-8이 아닌 경로는 건너뛴다
fn parse_status(buf: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut records = buf.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        // 경로 앞의 칸 수. 경로에는 공백이 들어갈 수 있으므로 그만큼만 나눈다
        let fields = match record[0] {
            b'1' => 8,
            b'2' => 9,
            b'u' => 10,
            b'?' => 1,
            _ => continue,
        };
        // 원래 경로 조각은 UTF-8 검사 전에 꺼낸다. 남기면 다음 줄로 읽힌다
        let original = if record[0] == b'2' { records.next() } else { None };
        let Ok(line) = std::str::from_utf8(record) else { continue };
        let parts: Vec<&str> = line.splitn(fields + 1, ' ').collect();
        if parts.len() != fields + 1 {
            continue;
        }
        let xy = if fields == 1 { "??" } else { parts[1] };
        out.push((parts[fields].to_string(), xy.to_string()));
        if let Some(original) = original.and_then(|o| std::str::from_utf8(o).ok()) {
            out.push((original.to_string(), xy.to_string()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_v2() {
        let z = "\
1 .M N... 100644 100644 100644 aaa aaa src/a b.py\0\
2 R. N... 100644 100644 100644 bbb bbb R100 new name.py\0old name.py\0\
u UU N... 100644 100644 100644 100644 ccc ddd eee conflict.py\0\
? out/new file.txt\0\
! ignored.log\0\
# branch.oid abc\0";
        let rows = parse_status(z.as_bytes());
        let want = [
            ("src/a b.py", ".M"),
            ("new name.py", "R."),
            ("old name.py", "R."),
            ("conflict.py", "UU"),
            ("out/new file.txt", "??"),
        ];
        let want: Vec<_> = want.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
        assert_eq!(rows, want);
    }

    #[test]
    fn rename_original_path_is_its_own_row_even_when_not_utf8() {
        let mut z = b"2 R. N... 100644 100644 100644 bbb bbb R100 new.py\0old.py\0".to_vec();
        // UTF-8이 아닌 새 경로: 원래 경로 조각을 줄로 읽으면 안 된다
        z.extend(b"2 R. N... 100644 100644 100644 bbb bbb R100 \xff.py\0? x.py\0");
        z.extend(b"? after.py\0");
        let rows = parse_status(&z);
        let want = [("new.py", "R."), ("old.py", "R."), ("after.py", "??")];
        let want: Vec<_> = want.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
        assert_eq!(rows, want);
    }

    #[test]
    fn parses_cat_file_batch() {
        let out = b"aaa blob 5\nab\ncd\nHEAD:nope missing\nbbb tree 3\nxyz\n\
            ccc submodule\nddd blob 0\n\n";
        let rows = parse_batch(out);
        assert_eq!(rows, [Some(b"ab\ncd".to_vec()), None, None, None, Some(Vec::new())]);
    }

    fn entry(status: &str, mtime: u64) -> Entry {
        Entry { status: status.into(), mtime_ns: Some(mtime), size: Some(1) }
    }

    #[test]
    fn changed_follows_design_table() {
        let before = BTreeMap::from([
            ("same".to_string(), entry(".M", 1)),
            ("touched".to_string(), entry(".M", 1)),
            ("reverted".to_string(), entry(".M", 1)),
            ("staged".to_string(), entry("??", 1)),
        ]);
        let after = BTreeMap::from([
            ("same".to_string(), entry(".M", 1)),
            ("touched".to_string(), entry(".M", 2)),
            ("clean".to_string(), entry(".M", 1)),
            ("new".to_string(), entry("??", 1)),
            // `git add`만 하면 내용이 그대로라 바뀐 것이 아니다
            ("staged".to_string(), entry("A.", 1)),
        ]);
        // HEAD 사이 경로: Bash 전에 깨끗했으면 Clean, 이미 바뀌어 있었으면 표대로
        let committed = ["committed", "same", "touched"].map(String::from);
        let got = changed(&before, &after, committed);
        let want = BTreeMap::from([
            ("touched".to_string(), Change::Unknown),
            ("clean".to_string(), Change::Clean),
            ("new".to_string(), Change::Clean),
            ("reverted".to_string(), Change::Unknown),
            ("committed".to_string(), Change::Clean),
        ]);
        assert_eq!(got, want);
    }
}
