//! Bash tool 전후. Bash가 어떤 파일을 고칠지 미리 알 수 없으므로 전후 `git status`를 비교해
//! 바뀐 파일을 찾는다. git 레포 밖의 Bash와 ignored 파일은 잡지 않는다
use std::collections::BTreeMap;
use std::fs;
use std::io;
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

impl Entry {
    fn untracked(&self) -> bool {
        self.status == "??"
    }
}

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
    let after = status(&before.root)?;
    let meta = dir.read_meta();
    let git = Git::new(&before.root);
    let start_head = meta.as_ref().and_then(|m| m.start_head.as_deref());
    let source = Source { git: &git, pre_head: &before.head, start_head };

    let mut paths = Vec::new();
    let mut first_error = None;
    for (rel, change) in changed(&before.entries, &after) {
        let Some(path) = valid_abs_path(&before.root.join(&rel)) else { continue };
        // 이미 원본이 있으면 git show를 부르지 않는다. 경쟁은 `ensure_baseline`이 가린다
        if !dir.baseline_file(&path).exists() {
            if let Err(e) = source.record(dir, &path, &rel, change) {
                first_error.get_or_insert(e);
            }
        }
        paths.push(path);
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
    first_error.map_or(Ok(()), |e| Err(e.into()))
}

/// 바뀐 파일의 Bash 전 내용을 얼마나 아는지
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    /// Bash 전에는 깨끗한 tracked 파일이었다. 내용 = pre HEAD
    Clean,
    /// Bash가 만든 untracked 파일
    Untracked,
    /// Bash 전에 이미 바뀌어 있었다 (재수정, 되돌림, 커밋, 삭제)
    Unknown,
}

/// 설계 3절 "post-bash" 표. S에 없고 T에 있으면 깨끗했던 파일, 둘 다 있으면 mtime·size가 다를 때,
/// S에만 있으면 (되돌림·커밋·삭제) 항상 바뀐 것으로 본다
fn changed(before: &Entries, after: &Entries) -> Vec<(String, Change)> {
    let mut out = Vec::new();
    for (rel, now) in after {
        let change = match before.get(rel) {
            None if now.untracked() => Change::Untracked,
            None => Change::Clean,
            Some(was) if (was.mtime_ns, was.size) != (now.mtime_ns, now.size) => Change::Unknown,
            Some(_) => continue,
        };
        out.push((rel.clone(), change));
    }
    let gone = before.keys().filter(|rel| !after.contains_key(*rel));
    out.extend(gone.map(|rel| (rel.clone(), Change::Unknown)));
    out
}

/// 원본 내용을 읽어 올 곳
struct Source<'a> {
    git: &'a Git,
    pre_head: &'a str,
    start_head: Option<&'a str>,
}

impl Source<'_> {
    fn record(&self, dir: &SessionDir, path: &Path, rel: &str, change: Change) -> io::Result<bool> {
        match change {
            Change::Untracked => dir.ensure_baseline(path, BaselineContent::New),
            // pre HEAD에 없는 tracked 파일 (Bash가 만들고 `git add`)은 새 파일이다
            Change::Clean => match self.show(Some(self.pre_head), rel) {
                Some(bytes) => dir.ensure_baseline(path, BaselineContent::Bytes(&bytes)),
                None => dir.ensure_baseline(path, BaselineContent::New),
            },
            Change::Unknown => {
                let bytes = self.show(self.start_head, rel);
                let head = self.start_head.map(String::from);
                let content = BaselineContent::Uncertain { head, bytes: bytes.as_deref() };
                dir.ensure_baseline(path, content)
            }
        }
    }

    /// `git show <rev>:<rel>`. rev가 없거나 그 커밋에 경로가 없으면 `None`
    fn show(&self, rev: Option<&str>, rel: &str) -> Option<Vec<u8>> {
        self.git.run(&["show", &format!("{}:{rel}", rev?)]).ok()
    }
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
/// rename은 새 경로만 쓴다 (원래 경로는 다음 NUL 조각). UTF-8이 아닌 경로는 건너뛴다
fn parse_status(buf: &[u8]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut records = buf.split(|b| *b == 0).filter(|r| !r.is_empty());
    while let Some(record) = records.next() {
        let Ok(line) = std::str::from_utf8(record) else { continue };
        // 경로 앞의 칸 수. 경로에는 공백이 들어갈 수 있으므로 그만큼만 나눈다
        let fields = match line.as_bytes()[0] {
            b'1' => 8,
            b'2' => {
                records.next();
                9
            }
            b'u' => 10,
            b'?' => 1,
            _ => continue,
        };
        let parts: Vec<&str> = line.splitn(fields + 1, ' ').collect();
        if parts.len() == fields + 1 {
            let xy = if fields == 1 { "??" } else { parts[1] };
            out.push((parts[fields].to_string(), xy.to_string()));
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
            ("conflict.py", "UU"),
            ("out/new file.txt", "??"),
        ];
        let want: Vec<_> = want.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
        assert_eq!(rows, want);
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
        let got: BTreeMap<_, _> = changed(&before, &after).into_iter().collect();
        let want = BTreeMap::from([
            ("touched".to_string(), Change::Unknown),
            ("clean".to_string(), Change::Clean),
            ("new".to_string(), Change::Untracked),
            ("reverted".to_string(), Change::Unknown),
        ]);
        assert_eq!(got, want);
    }
}
