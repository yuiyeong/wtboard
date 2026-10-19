//! 세션 기록 폴더 형식. `<data dir>/sessions/<session_id>/` 하나가 Claude Code 세션 하나다.
//!
//! hook은 짧게 도는 프로세스라 여러 개가 동시에 같은 폴더에 쓴다. 파일마다 쓰는 방식이 정해져 있다.
//! - `meta.json`: turn 경계 hook만 쓴다 (한 세션 안에서 겹치지 않는다). tmp에 쓰고 rename
//! - `events.jsonl`: 줄 하나를 `O_APPEND` write 한 번으로
//! - `baseline/<abs>`: `create_new`로 먼저 만든 쪽이 이긴다. 이긴 쪽만 `baseline.jsonl`에 한 줄 append
//! - `turns/<n>/<abs>`, `turns/<n>/deleted.txt`, `turns/<n>/too_large.txt`: turn 경계 hook만 쓴다
//!
//! 경로 key는 NFC 절대 경로다. 내용 파일은 절대 경로에서 앞의 `/`를 뗀 경로에 둔다.
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::domain::MAX_DIFF_BYTES;
use crate::paths::nfc;

const META: &str = "meta.json";
const EVENTS: &str = "events.jsonl";
const BASELINE_DIR: &str = "baseline";
const BASELINES: &str = "baseline.jsonl";
const TURNS: &str = "turns";
const DELETED: &str = "deleted.txt";
const TOO_LARGE: &str = "too_large.txt";

/// `[A-Za-z0-9_-]+`만 허용한다. 폴더 이름으로 쓰므로 `/`, `..`가 들어오면 세션 폴더 밖에 쓰게 된다
pub fn is_valid_session_id(id: &str) -> bool {
    !id.is_empty() && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// 절대 경로이고 `..` component가 없으면 NFC로 바꿔 돌려준다
pub fn valid_abs_path(path: &Path) -> Option<PathBuf> {
    let ok = path.is_absolute() && !path.components().any(|c| c == Component::ParentDir);
    ok.then(|| nfc(path))
}

/// 세션 상태. 시각은 Unix ms
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub started_at: u64,
    pub project_dir: PathBuf,
    /// 세션 시작 때 `project_dir`의 HEAD. 레포가 아니면 없음
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_head: Option<String>,
    /// 마지막으로 시작한 turn. 첫 prompt 전은 0
    pub turn: u32,
    pub turn_open: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Edit,
    Bash,
    TurnStart,
    TurnEnd,
}

/// `events.jsonl`의 한 줄
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub t: u64,
    pub turn: u32,
    pub kind: EventKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    pub paths: Vec<PathBuf>,
    /// subagent 안에서 난 event면 그 agent id
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    /// 내용 파일이 원본이다
    File,
    /// 세션이 처음 건드릴 때 없던 파일
    New,
    /// 1MB를 넘어 내용을 복사하지 않음
    TooLarge,
    /// Bash가 이미 바뀐 파일을 고쳐 세션 전 내용을 모른다. 내용 파일은 세션 시작 HEAD의 내용
    Uncertain,
}

/// `baseline.jsonl`의 한 줄
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    pub path: PathBuf,
    pub kind: BaselineKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
}

/// `ensure_baseline`에 넘기는 원본
#[derive(Debug, Clone)]
pub enum BaselineContent<'a> {
    Bytes(&'a [u8]),
    /// 지금 파일 내용을 복사한다. 그 사이 파일이 없어졌으면 `New`
    CopyOf(&'a Path),
    New,
    /// `bytes`는 세션 시작 HEAD의 내용 (hook이 `git show`로 읽는다). 없으면 빈 파일로 본다
    Uncertain { head: Option<String>, bytes: Option<&'a [u8]> },
}

/// turn이 끝난 시점의 파일 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Snapshot {
    /// 세션 폴더 안의 내용 파일
    Content(PathBuf),
    Deleted,
    TooLarge,
}

/// 세션 폴더 하나. `open`은 경로만 정하고, 폴더는 처음 쓸 때 만든다
#[derive(Debug, Clone)]
pub struct SessionDir {
    id: String,
    root: PathBuf,
}

impl SessionDir {
    pub fn open(data_dir: &Path, session_id: &str) -> Option<Self> {
        is_valid_session_id(session_id).then(|| Self {
            id: session_id.to_string(),
            root: data_dir.join("sessions").join(session_id),
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn read_meta(&self) -> Option<Meta> {
        serde_json::from_slice(&fs::read(self.root.join(META)).ok()?).ok()
    }

    /// tmp에 쓰고 rename한다. 앱이 반쯤 쓴 meta를 읽지 않는다
    pub fn write_meta(&self, meta: &Meta) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let tmp = self.root.join(format!("{META}.{}.tmp", std::process::id()));
        let written = fs::write(&tmp, serde_json::to_vec(meta)?)
            .and_then(|()| fs::rename(&tmp, self.root.join(META)));
        if written.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        written
    }

    pub fn append_event(&self, event: &Event) -> io::Result<()> {
        self.append_line(EVENTS, event)
    }

    /// 못 읽는 줄은 건너뛴다
    pub fn events(&self) -> Vec<Event> {
        self.read_lines(EVENTS)
    }

    /// 마지막 활동 시각 (Unix ms). meta는 turn 경계에만 쓰므로 `events.jsonl`의 mtime을 본다
    pub fn last_activity(&self) -> Option<u64> {
        mtime_ms(&self.root.join(EVENTS)).or_else(|| mtime_ms(&self.root.join(META)))
    }

    /// 경로의 원본을 처음 한 번만 기록한다. 이미 있으면 `false`.
    /// 내용 파일을 `create_new`로 만들어 병렬 hook 중 하나만 이긴다 (`New`·`TooLarge`도 빈 파일을 만든다).
    /// 이긴 뒤 실패하면 내용 파일을 지워 다음 hook이 다시 기록할 수 있게 한다
    pub fn ensure_baseline(&self, path: &Path, content: BaselineContent) -> io::Result<bool> {
        let path = valid_abs_path(path).ok_or_else(|| invalid_path(path))?;
        let file = self.baseline_file(&path);
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut out = match OpenOptions::new().write(true).create_new(true).open(&file) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(e) => return Err(e),
        };
        let recorded = write_baseline(&mut out, content).and_then(|(kind, head)| {
            self.append_line(BASELINES, &Baseline { path, kind, head })
        });
        if let Err(e) = recorded {
            drop(out);
            let _ = fs::remove_file(&file);
            return Err(e);
        }
        Ok(true)
    }

    /// 경로별 원본 기록. 같은 경로가 두 번 있으면 먼저 쓴 줄
    pub fn baselines(&self) -> HashMap<PathBuf, Baseline> {
        let mut map = HashMap::new();
        for b in self.read_lines::<Baseline>(BASELINES) {
            map.entry(b.path.clone()).or_insert(b);
        }
        map
    }

    pub fn baseline_file(&self, path: &Path) -> PathBuf {
        mirror(&self.root.join(BASELINE_DIR), path)
    }

    /// turn이 끝난 시점의 내용을 기록한다. 그 turn이 건드린 경로를 한 번에 넘긴다
    /// (`deleted.txt`, `too_large.txt`는 이번 호출의 목록으로 바뀐다). 잘못된 경로는 건너뛴다
    pub fn snapshot_turn(&self, turn: u32, paths: &[PathBuf]) -> io::Result<()> {
        let dir = self.turn_dir(turn);
        fs::create_dir_all(&dir)?;
        let (mut deleted, mut too_large) = (String::new(), String::new());
        for path in paths.iter().filter_map(|p| valid_abs_path(p)) {
            let dest = mirror(&dir, &path);
            let list = match fs::metadata(&path) {
                Ok(m) if !m.is_file() => &mut deleted,
                Ok(m) if m.len() > MAX_DIFF_BYTES => &mut too_large,
                Ok(_) => {
                    if let Some(parent) = dest.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::copy(&path, &dest)?;
                    continue;
                }
                Err(e) if e.kind() == io::ErrorKind::NotFound => &mut deleted,
                Err(e) => return Err(e),
            };
            // 같은 turn을 다시 찍을 때 예전 내용 파일이 남아 있으면 Content로 읽힌다
            if let Err(e) = fs::remove_file(&dest) {
                if e.kind() != io::ErrorKind::NotFound {
                    return Err(e);
                }
            }
            list.push_str(&path.to_string_lossy());
            list.push('\n');
        }
        fs::write(dir.join(DELETED), deleted)?;
        fs::write(dir.join(TOO_LARGE), too_large)
    }

    /// turn `n`에 기록된 경로 하나. 그 turn이 건드리지 않았으면 `None`
    pub fn turn_snapshot(&self, turn: u32, path: &Path) -> Option<Snapshot> {
        let dir = self.turn_dir(turn);
        let path = nfc(path);
        let file = mirror(&dir, &path);
        if file.is_file() {
            return Some(Snapshot::Content(file));
        }
        let listed = |name: &str| {
            fs::read_to_string(dir.join(name))
                .is_ok_and(|s| s.lines().any(|l| Path::new(l) == path))
        };
        if listed(TOO_LARGE) {
            Some(Snapshot::TooLarge)
        } else if listed(DELETED) {
            Some(Snapshot::Deleted)
        } else {
            None
        }
    }

    fn turn_dir(&self, turn: u32) -> PathBuf {
        self.root.join(TURNS).join(turn.to_string())
    }

    /// 줄 전체를 write 한 번으로 쓴다. `O_APPEND`라 병렬 hook의 줄이 섞이지 않는다
    fn append_line(&self, name: &str, value: &impl Serialize) -> io::Result<()> {
        let mut line = serde_json::to_vec(value)?;
        line.push(b'\n');
        fs::create_dir_all(&self.root)?;
        let mut f = OpenOptions::new().create(true).append(true).open(self.root.join(name))?;
        let n = f.write(&line)?;
        if n != line.len() {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "줄 일부만 씀"));
        }
        Ok(())
    }

    fn read_lines<T: DeserializeOwned>(&self, name: &str) -> Vec<T> {
        let Ok(bytes) = fs::read(self.root.join(name)) else { return Vec::new() };
        bytes.split(|b| *b == b'\n').filter_map(|l| serde_json::from_slice(l).ok()).collect()
    }
}

/// 원본 내용을 쓰고 기록할 kind와 head를 돌려준다
fn write_baseline(
    out: &mut File,
    content: BaselineContent,
) -> io::Result<(BaselineKind, Option<String>)> {
    Ok(match content {
        BaselineContent::New => (BaselineKind::New, None),
        BaselineContent::Bytes(bytes) => (write_small(out, bytes, BaselineKind::File)?, None),
        BaselineContent::Uncertain { head, bytes } => {
            (write_small(out, bytes.unwrap_or_default(), BaselineKind::Uncertain)?, head)
        }
        BaselineContent::CopyOf(src) => match File::open(src) {
            Ok(mut f) if f.metadata()?.len() <= MAX_DIFF_BYTES => {
                io::copy(&mut f, out)?;
                (BaselineKind::File, None)
            }
            Ok(_) => (BaselineKind::TooLarge, None),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (BaselineKind::New, None),
            Err(e) => return Err(e),
        },
    })
}

/// 1MB를 넘으면 쓰지 않고 `TooLarge`
fn write_small(out: &mut File, bytes: &[u8], kind: BaselineKind) -> io::Result<BaselineKind> {
    if bytes.len() as u64 > MAX_DIFF_BYTES {
        return Ok(BaselineKind::TooLarge);
    }
    out.write_all(bytes)?;
    Ok(kind)
}

/// 절대 경로를 `base` 아래로 옮긴다. `Normal` component만 이어서 잘못된 경로도 `base` 밖을 가리키지 않는다
fn mirror(base: &Path, path: &Path) -> PathBuf {
    let path = nfc(path);
    let mut out = base.to_path_buf();
    out.extend(path.components().filter(|c| matches!(c, Component::Normal(_))));
    out
}

fn mtime_ms(path: &Path) -> Option<u64> {
    let t = fs::metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

fn invalid_path(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("세션 기록에 쓸 수 없는 경로: {}", path.display()),
    )
}
