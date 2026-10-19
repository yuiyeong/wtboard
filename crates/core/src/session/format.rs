//! 세션 기록 폴더 형식. `<data dir>/sessions/<session_id>/` 하나가 Claude Code 세션 하나다.
//!
//! hook은 짧게 도는 프로세스라 여러 개가 동시에 같은 폴더에 쓴다. 파일마다 쓰는 방식이 정해져 있다.
//! - `meta.json`: turn 경계 hook만 쓴다 (한 세션 안에서 겹치지 않는다). tmp에 쓰고 rename
//! - `events.jsonl`: 줄 하나를 `O_APPEND` write 한 번으로
//! - `baseline/<abs>`: `create_new`로 먼저 만든 쪽이 이긴다. 이긴 쪽만 `baseline.jsonl`에 한 줄 append
//! - `turns/<n>/<abs>`, `turns/<n>/index.jsonl`: turn 경계 hook만 쓴다
//! - `bash/<tool_use_id>.json`: pre-bash가 tmp에 쓰고 rename, 같은 tool 호출의 post-bash가 읽고 지운다
//!
//! 경로 key는 NFC 절대 경로다. 내용 파일은 절대 경로에서 앞의 `/`를 뗀 경로에 둔다.
//!
//! 기록에는 세션이 편집한 파일(`.env` 등)의 원본이 들어가므로 다른 사용자가 못 읽게 폴더는 0700,
//! 파일은 0600으로 만든다 (`create_private_dir`, `private_file`).
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
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
const TURN_INDEX: &str = "index.jsonl";
const BASH_DIR: &str = "bash";
const DIR_MODE: u32 = 0o700;
const FILE_MODE: u32 = 0o600;

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
    /// `TooLarge`일 때 원본 크기와 blake3 hash
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
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
    /// 1MB를 넘어 내용 대신 크기와 blake3 hash만 남김
    TooLarge { size: u64, hash: String },
}

/// `turns/<n>/index.jsonl`의 한 줄. 내용 파일이 없는 경로만 적는다
#[derive(Serialize, Deserialize)]
struct TurnEntry {
    path: PathBuf,
    #[serde(flatten)]
    state: TurnState,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TurnState {
    Deleted,
    TooLarge { size: u64, hash: String },
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
        create_private_dir(&self.root)?;
        write_atomic(&self.root.join(META), &serde_json::to_vec(meta)?)
    }

    /// 경로는 `valid_abs_path`를 통과한 것만 NFC로 남긴다.
    /// 남은 경로가 없는 `Edit`·`Bash` event는 쓰지 않는다 (turn event는 원래 경로가 없다)
    pub fn append_event(&self, event: &Event) -> io::Result<()> {
        let paths: Vec<PathBuf> = event.paths.iter().filter_map(|p| valid_abs_path(p)).collect();
        if paths.is_empty() && matches!(event.kind, EventKind::Edit | EventKind::Bash) {
            return Ok(());
        }
        self.append_line(EVENTS, &Event { paths, ..event.clone() })
    }

    /// 못 읽는 줄은 건너뛴다
    pub fn events(&self) -> Vec<Event> {
        read_lines(&self.root.join(EVENTS))
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
            create_private_dir(dir)?;
        }
        let mut out = match private_file().create_new(true).open(&file) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(e) => return Err(e),
        };
        let recorded = write_baseline(&mut out, path, content)
            .and_then(|b| self.append_line(BASELINES, &b));
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
        for b in read_lines::<Baseline>(&self.root.join(BASELINES)) {
            map.entry(b.path.clone()).or_insert(b);
        }
        map
    }

    pub fn baseline_file(&self, path: &Path) -> PathBuf {
        mirror(&self.root.join(BASELINE_DIR), path)
    }

    /// turn이 끝난 시점의 내용을 기록한다. 그 turn이 건드린 경로를 한 번에 넘긴다
    /// (`index.jsonl`은 이번 호출의 목록으로 바뀐다). 경로마다 따로 처리해서 한 경로의 오류
    /// (읽기 실패, 그사이 사라짐 등)가 다른 경로를 막지 않는다. 그런 경로와 잘못된 경로는 건너뛴다
    pub fn snapshot_turn(&self, turn: u32, paths: &[PathBuf]) -> io::Result<()> {
        let dir = self.turn_dir(turn);
        create_private_dir(&dir)?;
        let mut index = Vec::new();
        for path in paths.iter().filter_map(|p| valid_abs_path(p)) {
            if let Ok(Some(state)) = snapshot_file(&path, &mirror(&dir, &path)) {
                index.extend(json_line(&TurnEntry { path, state })?);
            }
        }
        write_atomic(&dir.join(TURN_INDEX), &index)
    }

    /// turn `n`에 기록된 경로 하나. 그 turn이 건드리지 않았으면 `None`.
    /// index를 먼저 본다. 같은 turn을 다시 찍었을 때 남은 예전 내용 파일보다 이번 상태가 앞선다
    pub fn turn_snapshot(&self, turn: u32, path: &Path) -> Option<Snapshot> {
        let dir = self.turn_dir(turn);
        let path = nfc(path);
        let index = read_lines::<TurnEntry>(&dir.join(TURN_INDEX));
        if let Some(entry) = index.into_iter().find(|e| e.path == path) {
            return Some(match entry.state {
                TurnState::Deleted => Snapshot::Deleted,
                TurnState::TooLarge { size, hash } => Snapshot::TooLarge { size, hash },
            });
        }
        let file = mirror(&dir, &path);
        file.is_file().then_some(Snapshot::Content(file))
    }

    /// pre-bash의 git status 스냅샷을 저장한다. 내용 형식은 hook이 정한다
    pub fn write_bash_snapshot(&self, tool_use_id: &str, bytes: &[u8]) -> io::Result<()> {
        let file = self.bash_file(tool_use_id)?;
        create_private_dir(&self.root.join(BASH_DIR))?;
        write_atomic(&file, bytes)
    }

    /// pre-bash 스냅샷을 읽고 지운다. 없거나 id가 잘못됐으면 `None`
    pub fn take_bash_snapshot(&self, tool_use_id: &str) -> Option<Vec<u8>> {
        let file = self.bash_file(tool_use_id).ok()?;
        let bytes = fs::read(&file).ok()?;
        let _ = fs::remove_file(&file);
        Some(bytes)
    }

    /// `tool_use_id`는 hook 입력에서 오므로 session id와 같은 규칙으로 걸러 파일 이름으로 쓴다
    fn bash_file(&self, tool_use_id: &str) -> io::Result<PathBuf> {
        if !is_valid_session_id(tool_use_id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("잘못된 tool_use_id: {tool_use_id:?}"),
            ));
        }
        Ok(self.root.join(BASH_DIR).join(format!("{tool_use_id}.json")))
    }

    fn turn_dir(&self, turn: u32) -> PathBuf {
        self.root.join(TURNS).join(turn.to_string())
    }

    /// 줄 전체를 write 한 번으로 쓴다. `O_APPEND`라 병렬 hook의 줄이 섞이지 않는다
    fn append_line(&self, name: &str, value: &impl Serialize) -> io::Result<()> {
        let line = json_line(value)?;
        create_private_dir(&self.root)?;
        let mut f = private_file().create(true).append(true).open(self.root.join(name))?;
        let n = f.write(&line)?;
        if n != line.len() {
            return Err(io::Error::new(io::ErrorKind::WriteZero, "줄 일부만 씀"));
        }
        Ok(())
    }
}

fn json_line(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    Ok(line)
}

/// 못 읽는 줄은 건너뛴다
fn read_lines<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    let Ok(bytes) = fs::read(path) else { return Vec::new() };
    bytes.split(|b| *b == b'\n').filter_map(|l| serde_json::from_slice(l).ok()).collect()
}

/// tmp에 쓰고 rename한다. 읽는 쪽이 반쯤 쓴 파일을 보지 않는다
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = create_private(&tmp)
        .and_then(|mut f| f.write_all(bytes))
        .and_then(|()| fs::rename(&tmp, path));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// 원본 내용을 쓰고 `baseline.jsonl`에 적을 기록을 돌려준다
fn write_baseline(out: &mut File, path: PathBuf, content: BaselineContent) -> io::Result<Baseline> {
    let mut b = Baseline { path, kind: BaselineKind::New, head: None, size: None, hash: None };
    match content {
        BaselineContent::New => {}
        BaselineContent::Bytes(bytes) => {
            copy_or_hash(out, bytes, bytes.len() as u64, BaselineKind::File, &mut b)?
        }
        BaselineContent::Uncertain { head, bytes } => {
            b.head = head;
            let bytes = bytes.unwrap_or_default();
            copy_or_hash(out, bytes, bytes.len() as u64, BaselineKind::Uncertain, &mut b)?
        }
        // 일반 파일만 연다 (FIFO는 열면 멈춘다). 그사이 없어졌거나 파일이 아니면 `New`
        BaselineContent::CopyOf(src) => match fs::metadata(src) {
            Ok(m) if m.is_file() => {
                copy_or_hash(out, File::open(src)?, m.len(), BaselineKind::File, &mut b)?
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        },
    }
    Ok(b)
}

/// 1MB 이하면 `out`에 쓰고 `kind`로, 넘으면 쓰지 않고 크기와 hash를 남겨 `TooLarge`로 기록한다
fn copy_or_hash(
    out: &mut File,
    mut src: impl io::Read,
    len: u64,
    kind: BaselineKind,
    b: &mut Baseline,
) -> io::Result<()> {
    if len > MAX_DIFF_BYTES {
        let (size, hash) = hash_reader(src)?;
        (b.kind, b.size, b.hash) = (BaselineKind::TooLarge, Some(size), Some(hash));
    } else {
        io::copy(&mut src, out)?;
        b.kind = kind;
    }
    Ok(())
}

/// 파일 하나를 `dest`에 복사한다. 내용 파일 대신 index에 적을 경로면 그 상태를 돌려준다
fn snapshot_file(path: &Path, dest: &Path) -> io::Result<Option<TurnState>> {
    let meta = match fs::metadata(path) {
        Ok(m) if m.is_file() => m,
        // 폴더·FIFO 등은 파일로서는 없는 것이다
        Ok(_) => return Ok(Some(TurnState::Deleted)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Some(TurnState::Deleted)),
        Err(e) => return Err(e),
    };
    if meta.len() > MAX_DIFF_BYTES {
        let (size, hash) = hash_reader(File::open(path)?)?;
        return Ok(Some(TurnState::TooLarge { size, hash }));
    }
    // `fs::copy`는 원본 권한을 따라가므로 0600으로 만든 파일에 내용을 붓는다
    let mut src = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Some(TurnState::Deleted)),
        Err(e) => return Err(e),
    };
    if let Some(parent) = dest.parent() {
        create_private_dir(parent)?;
    }
    io::copy(&mut src, &mut create_private(dest)?)?;
    Ok(None)
}

/// 없는 폴더를 0700으로 만든다 (중간 폴더 포함). 이미 있는 폴더의 권한은 건드리지 않는다
fn create_private_dir(dir: &Path) -> io::Result<()> {
    fs::DirBuilder::new().recursive(true).mode(DIR_MODE).create(dir)
}

/// 새로 만들 때 0600인 쓰기용 `OpenOptions`
fn private_file() -> OpenOptions {
    let mut opts = OpenOptions::new();
    opts.write(true).mode(FILE_MODE);
    opts
}

/// 0600으로 만들거나 비우고 연다
fn create_private(path: &Path) -> io::Result<File> {
    private_file().create(true).truncate(true).open(path)
}

/// 읽은 바이트 수와 blake3 hash (hex)
fn hash_reader(src: impl io::Read) -> io::Result<(u64, String)> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(src)?;
    Ok((hasher.count(), hasher.finalize().to_hex().to_string()))
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
