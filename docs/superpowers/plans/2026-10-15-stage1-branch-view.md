# wtboard 1단계: 브랜치 전체 뷰 + 실시간 갱신 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 레포 1개·worktree 1개를 감시하며 base 대비 PR 스타일 split/unified diff를 보여 주고, Claude Code가 파일을 고치면 1초 안에 화면이 바뀌는 Tauri 앱을 만든다.

**Architecture:** Rust `wtboard-core` crate가 git CLI 읽기 전용 호출, 변경 목록·파일 diff 계산, watcher(FSEvents + debounce + 30초 poll), worktree 상태를 맡는다. `wtboard-app`(Tauri 2)은 core를 Tauri command/event로 노출만 한다. React 화면은 TanStack Query로 command 결과를 cache하고 `worktree-changed` event가 오면 해당 query만 invalidate한다.

**Tech Stack:** Rust (edition 2021), Tauri 2, `notify` 8 + `notify-debouncer-full` 0.5, `blake3`, `toml` + `serde`, `unicode-normalization`; React + TypeScript + Vite + Tailwind CSS 4, TanStack Query 5, Zustand, `react-diff-view` 3 + `refractor` 3.6, Vitest.

**Spec:** `docs/draft.md`

## 범위

`docs/draft.md` 13절의 5단계 중 **1단계만** 다룬다. 2~5단계는 1단계 결과(특히 diff 렌더링 품질, watcher 동작)를 보고 단계마다 별도 plan을 쓴다. 맨 아래 "이후 단계 로드맵" 참고.

1단계에서 의도적으로 미루는 것:

| 미루는 것 | 1단계 처리 | 다룰 단계 |
|---|---|---|
| base 결정의 `gh pr view` 단계 | config `base` → `origin/HEAD` 두 단계만 | 2 |
| 프로필(FastAPI/Next/Flutter)별 제외 폴더·접기 | 모든 프로젝트 공통 제외 폴더만 (`build`, `out`, `ios/Pods`는 미포함) | 2 |
| `linguist-generated` 접기 | 없음 | 2 |
| 여러 worktree 발견, `worktrees/` 감시, 사이드바, 파일 트리 | config 첫 레포의 경로 하나, 평면 파일 목록 | 2 |
| rename 감지 | `--no-renames` (삭제 + 추가로 보임) | 2 이후 필요 시 |

spec과 다르게 정한 것 하나: spec 4절은 감시를 `app` crate에 두지만, Tauri 없이 테스트하려고 watcher를 `core::watch`에 둔다. `app`은 event를 화면으로 넘기기만 한다.

## Global Constraints

- 앱은 레포에 **어떤 것도 쓰지 않는다** (index, objects, config 전부).
- 모든 git 호출에 `GIT_OPTIONAL_LOCKS=0`을 붙인다.
- 변경 판단은 `git status`가 아니라 **파일별 diff hash**(`blake3`)로 한다.
- 파일 diff는 화면에 보일 때만 계산한다 (lazy loading).
- 제외 폴더는 감시 단계(event 분류)에서 걸러 낸다.
- debounce 300ms, 백업 재계산 주기 `[watch] fallback_poll_secs` (기본 30초).
- 바이너리 파일, 1MB(1,048,576 bytes) 초과 파일은 diff 대신 "변경됨 (크기 변화)"만 표시한다.
- 설정 파일은 `~/.wtboard/config.toml`.
- git ≥ 2.31 (`rev-parse --path-format=absolute` 사용). 현재 머신은 2.54.
- 1단계 완료 기준: **Claude Code가 파일을 고치면 1초 안에 화면 diff가 바뀐다.**

## Review Focus

1. **사용자 global gitconfig** (`color.ui=always`, `color.diff=always`, `diff.noprefix=true`, `diff.mnemonicPrefix=true`)가 있어도 diff 파싱과 화면이 깨지지 않아야 한다 → Task 5 테스트.
2. **특이한 파일 이름** (공백, 한글, `*`, `[` 포함)이 그대로 나오고, pathspec이 glob으로 해석되지 않아야 한다 → Task 4 테스트.
3. **macOS 경로 표기 차이**: FSEvents가 NFD 한글 경로를 주거나 tempdir이 `/var` ↔ `/private/var` symlink를 거쳐도 파일이 매칭되어야 하고, 작업 폴더의 `Cargo.lock`/`uv.lock`은 git lock 파일로 오인하지 않아야 한다 → Task 6 테스트.
4. **폴더 단위 변경** (`rm -rf src`, codegen이 새 폴더째 생성)이 event 하나로 와도 목록에 빠짐없이 반영되어야 한다 → Task 7 테스트.
5. **설정 부족** (config 파일 없음, origin도 config `base`도 없음, 경로 없음)일 때 앱이 죽지 않고 무엇을 고치면 되는지 보여야 한다 → Task 2·3·7 테스트, Task 9 수동 확인.

---

## 파일 구조

```
wtboard/
├─ Cargo.toml                      # workspace
├─ package.json                    # pnpm workspace root, @tauri-apps/cli
├─ pnpm-workspace.yaml
├─ crates/
│  ├─ core/
│  │  ├─ Cargo.toml
│  │  ├─ src/
│  │  │  ├─ lib.rs
│  │  │  ├─ domain.rs              # ChangedFile, FileStatus, FilePatch, PatchBody, MAX_DIFF_BYTES
│  │  │  ├─ git/
│  │  │  │  ├─ mod.rs              # Git runner (env, 공통 인자, exit code)
│  │  │  │  ├─ base.rs             # base 결정, merge-base
│  │  │  │  ├─ changes.rs          # 변경 파일 목록
│  │  │  │  └─ patch.rs            # 파일 하나의 diff + hash
│  │  │  ├─ profile.rs             # 공통 제외 폴더
│  │  │  ├─ store/
│  │  │  │  ├─ mod.rs
│  │  │  │  └─ config.rs           # config.toml
│  │  │  ├─ watch/
│  │  │  │  ├─ mod.rs              # WatchLayout, Target, event 경로 분류
│  │  │  │  └─ watcher.rs          # notify debouncer + worker thread
│  │  │  └─ worktree.rs            # worktree 상태, refresh, WorktreeChanged
│  │  └─ tests/
│  │     ├─ common/mod.rs          # TestRepo helper
│  │     ├─ git_runner.rs
│  │     ├─ config.rs
│  │     ├─ base.rs
│  │     ├─ changes.rs
│  │     ├─ patch.rs
│  │     ├─ watch_layout.rs
│  │     ├─ worktree.rs
│  │     └─ watcher.rs
│  └─ app/
│     ├─ Cargo.toml
│     ├─ build.rs
│     ├─ tauri.conf.json
│     ├─ capabilities/default.json
│     ├─ app-icon.svg
│     ├─ icons/                    # tauri icon으로 생성
│     └─ src/main.rs               # Tauri command, event bridge
└─ ui/
   ├─ package.json
   ├─ vite.config.ts
   ├─ tsconfig.json
   ├─ index.html
   └─ src/
      ├─ main.tsx
      ├─ index.css
      ├─ api.ts                    # Tauri command wrapper + 타입
      ├─ store.ts                  # Zustand (split/unified)
      ├─ useWorktreeEvents.ts      # event → query invalidate
      ├─ App.tsx
      ├─ components/
      │  ├─ FileList.tsx
      │  └─ FileDiff.tsx
      └─ lib/
         ├─ anchor.ts
         ├─ events.ts (+ events.test.ts)
         ├─ language.ts (+ language.test.ts)
         ├─ highlight.ts (+ highlight.test.ts)
         └─ useInView.ts
```

---

### Task 0: 툴체인 설치 + workspace 뼈대

현재 머신에 Rust와 pnpm이 없다 (node 24는 Volta로 설치돼 있음).

**Files:**
- Create: `Cargo.toml`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `.gitignore`

**Interfaces:**
- Produces: crate `wtboard-core` (lib 이름 `wtboard_core`). 이후 모든 task가 이 crate에 모듈을 추가한다.

- [ ] **Step 1: Rust 설치**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustc --version && cargo --version
```
Expected: 두 버전이 출력된다.

- [ ] **Step 2: pnpm 설치**

```bash
volta install pnpm
pnpm --version
```

- [ ] **Step 3: workspace 파일 작성**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/core"]

[workspace.package]
version = "0.1.0"
edition = "2021"
```

`crates/core/Cargo.toml`:
```toml
[package]
name = "wtboard-core"
version.workspace = true
edition.workspace = true

[dependencies]
blake3 = "1.5"
notify = "8"
notify-debouncer-full = "0.5"
serde = { version = "1", features = ["derive"] }
thiserror = "2"
toml = "0.8"
unicode-normalization = "0.1"

[dev-dependencies]
serde_json = "1"
tempfile = "3"
```

`crates/core/src/lib.rs`:
```rust
//! wtboard 공용 레이어: git 읽기, diff 계산, 감시, 설정.
```

`.gitignore`:
```
/target
node_modules/
/ui/dist
/crates/app/gen
.DS_Store
```

- [ ] **Step 4: 빌드 확인**

Run: `cargo test -p wtboard-core`
Expected: 의존성 컴파일 후 `test result: ok. 0 passed`.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml Cargo.lock crates/core .gitignore docs
git commit -m "chore: workspace 뼈대와 설계 문서"
```

---

### Task 1: Git runner

모든 git 호출이 지나가는 한 곳. lock 회피 env, literal pathspec, 출력 형식을 고정하는 공통 인자를 여기서 붙인다.

**Files:**
- Create: `crates/core/src/git/mod.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/git_runner.rs`

**Interfaces:**
- Produces:
  - `pub struct Git` — `Git::new(cwd: impl Into<PathBuf>) -> Git`, `fn cwd(&self) -> &Path`
  - `fn command(&self, args: &[&str]) -> std::process::Command`
  - `fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError>` (exit 0만 성공)
  - `fn run_allow(&self, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, GitError>`
  - `fn run_str(&self, args: &[&str]) -> Result<String, GitError>` (stdout trim)
  - `pub enum GitError { Spawn(io::Error), Failed { args, code: Option<i32>, stderr }, Unexpected(String) }`
  - `pub(crate) const DIFF_FLAGS: &[&str]` — 모든 `git diff` 호출에 붙이는 인자

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/git_runner.rs`:
```rust
use std::ffi::OsStr;
use std::process::Command;

use wtboard_core::git::{Git, GitError};

fn env_of(cmd: &Command, key: &str) -> Option<String> {
    cmd.get_envs()
        .find(|(k, _)| *k == OsStr::new(key))
        .and_then(|(_, v)| v)
        .map(|v| v.to_string_lossy().into_owned())
}

#[test]
fn every_command_avoids_locks_and_fixes_output_format() {
    let cmd = Git::new("/tmp").command(&["status"]);
    assert_eq!(env_of(&cmd, "GIT_OPTIONAL_LOCKS").as_deref(), Some("0"));
    assert_eq!(env_of(&cmd, "GIT_LITERAL_PATHSPECS").as_deref(), Some("1"));
    assert_eq!(env_of(&cmd, "GIT_TERMINAL_PROMPT").as_deref(), Some("0"));
    let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(
        args,
        [
            "-c", "core.quotePath=false",
            "-c", "diff.noprefix=false",
            "-c", "diff.mnemonicPrefix=false",
            "status",
        ]
    );
}

#[test]
fn run_allow_accepts_listed_exit_codes_only() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "hello\n").unwrap();
    let git = Git::new(dir.path());
    // diff --no-index는 차이가 있으면 exit 1
    let args = ["diff", "--no-index", "--", "/dev/null", "a.txt"];
    let out = git.run_allow(&args, &[0, 1]).unwrap();
    assert!(String::from_utf8_lossy(&out).contains("+hello"));
    let err = git.run(&args).unwrap_err();
    assert!(matches!(err, GitError::Failed { code: Some(1), .. }), "{err:?}");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test git_runner`
Expected: FAIL — `unresolved import wtboard_core::git`.

- [ ] **Step 3: 구현**

`crates/core/src/git/mod.rs`:
```rust
use std::path::{Path, PathBuf};
use std::process::Command;

/// 사용자 gitconfig와 무관하게 출력 형식을 고정하는 공통 인자
const GLOBAL_ARGS: &[&str] = &[
    "-c", "core.quotePath=false",
    "-c", "diff.noprefix=false",
    "-c", "diff.mnemonicPrefix=false",
];

/// 모든 `git diff` 호출에 붙인다. 색, 외부 diff 도구, textconv, prefix 설정을 무력화한다.
pub(crate) const DIFF_FLAGS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--no-relative",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--no-renames",
];

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git 실행 실패: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("git {args} 실패 (exit {code:?}): {stderr}")]
    Failed { args: String, code: Option<i32>, stderr: String },
    #[error("예상하지 못한 git 출력: {0}")]
    Unexpected(String),
}

/// 읽기 전용 git CLI 래퍼. 모든 호출은 이 타입을 거친다.
#[derive(Debug, Clone)]
pub struct Git {
    cwd: PathBuf,
}

impl Git {
    pub fn new(cwd: impl Into<PathBuf>) -> Self {
        Self { cwd: cwd.into() }
    }

    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.args(GLOBAL_ARGS)
            .args(args)
            .current_dir(&self.cwd)
            // 읽기 명령이 index lock을 잡지 않게 한다 (Claude Code의 git 작업과 충돌 방지)
            .env("GIT_OPTIONAL_LOCKS", "0")
            // 파일 이름의 `*`, `[`를 glob으로 해석하지 않는다
            .env("GIT_LITERAL_PATHSPECS", "1")
            .env("GIT_TERMINAL_PROMPT", "0");
        cmd
    }

    pub fn run(&self, args: &[&str]) -> Result<Vec<u8>, GitError> {
        self.run_allow(args, &[0])
    }

    pub fn run_allow(&self, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, GitError> {
        let out = self.command(args).output()?;
        match out.status.code() {
            Some(code) if ok_codes.contains(&code) => Ok(out.stdout),
            code => Err(GitError::Failed {
                args: args.join(" "),
                code,
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            }),
        }
    }

    pub fn run_str(&self, args: &[&str]) -> Result<String, GitError> {
        let out = self.run(args)?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }
}
```

`crates/core/src/lib.rs`에 추가:
```rust
pub mod git;
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test git_runner`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): 읽기 전용 git runner"
```

---

### Task 2: config.toml 읽기

**Files:**
- Create: `crates/core/src/store/mod.rs`, `crates/core/src/store/config.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/config.rs`

**Interfaces:**
- Produces:
  - `Config { repos: Vec<RepoConfig>, watch: WatchConfig }` — `Config::parse(&str)`, `Config::load(&Path)`, `Config::default_path() -> PathBuf`
  - `RepoConfig { path: String, base: Option<String> }` — `fn resolved_path(&self) -> PathBuf`
  - `WatchConfig { fallback_poll_secs: u64 }` (기본 30)
  - `ConfigError { Io { path, source }, Parse(toml::de::Error) }`
  - `pub fn expand_tilde(p: &str, home: &Path) -> PathBuf`

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/config.rs`:
```rust
use std::path::Path;

use wtboard_core::store::config::{expand_tilde, Config};

#[test]
fn parses_repos_and_fills_defaults() {
    let cfg = Config::parse(
        r#"
[[repo]]
path = "~/dev/my-api"
base = "origin/main"

[[repo]]
path = "/abs/my-app"
"#,
    )
    .unwrap();
    assert_eq!(cfg.repos.len(), 2);
    assert_eq!(cfg.repos[0].base.as_deref(), Some("origin/main"));
    assert_eq!(cfg.repos[1].base, None);
    assert_eq!(cfg.watch.fallback_poll_secs, 30);
}

#[test]
fn ignores_sections_of_later_stages() {
    let cfg = Config::parse(
        "[sessions]\nkeep_days = 7\n[review]\nauto_attach_comments = true\n[[repo]]\npath = \"/x\"\n",
    )
    .unwrap();
    assert_eq!(cfg.repos.len(), 1);
}

#[test]
fn expands_tilde_only_at_start() {
    let home = Path::new("/Users/me");
    assert_eq!(expand_tilde("~/dev/a", home), Path::new("/Users/me/dev/a"));
    assert_eq!(expand_tilde("~", home), Path::new("/Users/me"));
    assert_eq!(expand_tilde("/abs/~x", home), Path::new("/abs/~x"));
}

#[test]
fn load_names_the_missing_file() {
    let err = Config::load(Path::new("/nonexistent/config.toml")).unwrap_err();
    assert!(err.to_string().contains("/nonexistent/config.toml"), "{err}");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test config`
Expected: FAIL — `unresolved import wtboard_core::store`.

- [ ] **Step 3: 구현**

`crates/core/src/store/mod.rs`:
```rust
pub mod config;
```

`crates/core/src/store/config.rs`:
```rust
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("설정 파일을 읽을 수 없음: {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("설정 파일 형식 오류: {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default, rename = "repo")]
    pub repos: Vec<RepoConfig>,
    #[serde(default)]
    pub watch: WatchConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepoConfig {
    pub path: String,
    pub base: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct WatchConfig {
    pub fallback_poll_secs: u64,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self { fallback_poll_secs: 30 }
    }
}

impl Config {
    pub fn parse(s: &str) -> Result<Self, ConfigError> {
        Ok(toml::from_str(s)?)
    }

    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let s = std::fs::read_to_string(path)
            .map_err(|source| ConfigError::Io { path: path.to_path_buf(), source })?;
        Self::parse(&s)
    }

    pub fn default_path() -> PathBuf {
        home_dir().join(".wtboard/config.toml")
    }
}

impl RepoConfig {
    pub fn resolved_path(&self) -> PathBuf {
        expand_tilde(&self.path, &home_dir())
    }
}

pub fn expand_tilde(p: &str, home: &Path) -> PathBuf {
    if p == "~" {
        home.to_path_buf()
    } else if let Some(rest) = p.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(p)
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}
```

`crates/core/src/lib.rs`에 추가:
```rust
pub mod store;
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test config`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): config.toml 읽기"
```

---

### Task 3: TestRepo helper + base / merge-base 결정

**Files:**
- Create: `crates/core/tests/common/mod.rs`, `crates/core/src/git/base.rs`
- Modify: `crates/core/src/git/mod.rs`
- Test: `crates/core/tests/base.rs`

**Interfaces:**
- Consumes: `Git`, `GitError` (Task 1)
- Produces:
  - `pub fn resolve_base(git: &Git, configured: Option<&str>) -> Result<String, BaseError>` — config 값 → `origin/HEAD`가 가리키는 브랜치(예: `"origin/main"`)
  - `pub fn merge_base(git: &Git, base: &str) -> Result<String, GitError>` — commit sha
  - `pub enum BaseError { NotFound, Git(GitError) }`
  - 테스트 helper `TestRepo` (`new`, `clone_of`, `path`, `git`, `write`, `remove`, `commit_all`, `head`) — 이후 모든 core 테스트가 쓴다

- [ ] **Step 1: TestRepo helper 작성**

`crates/core/tests/common/mod.rs`:
```rust
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 테스트용 임시 git 레포. 사용자 global gitconfig의 영향을 받지 않는다.
pub struct TestRepo {
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl TestRepo {
    /// `main` 브랜치에 `README.md`("hello\n") 커밋 하나가 있는 레포
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir(&root).unwrap();
        let repo = TestRepo { _dir: dir, root };
        repo.git(&["init", "-q", "-b", "main"]);
        repo.configure();
        repo.write("README.md", "hello\n");
        repo.commit_all("init");
        repo
    }

    /// `upstream`을 clone한 레포. `origin/HEAD` → `origin/main`
    pub fn clone_of(upstream: &TestRepo) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("clone");
        run(dir.path(), &["clone", "-q", upstream.root.to_str().unwrap(), root.to_str().unwrap()]);
        let repo = TestRepo { _dir: dir, root };
        repo.configure();
        repo
    }

    fn configure(&self) {
        self.git(&["config", "user.email", "test@example.com"]);
        self.git(&["config", "user.name", "test"]);
        self.git(&["config", "commit.gpgsign", "false"]);
        self.git(&["config", "core.hooksPath", "/dev/null"]);
    }

    /// tempdir 원래 경로. macOS에서는 canonicalize되지 않은 `/var/...` 형태다.
    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn git(&self, args: &[&str]) -> String {
        run(&self.root, args)
    }

    pub fn write(&self, rel: &str, contents: impl AsRef<[u8]>) {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, contents).unwrap();
    }

    pub fn remove(&self, rel: &str) {
        let p = self.root.join(rel);
        if p.is_dir() {
            fs::remove_dir_all(p).unwrap();
        } else {
            fs::remove_file(p).unwrap();
        }
    }

    pub fn commit_all(&self, msg: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", msg]);
    }

    pub fn head(&self) -> String {
        self.git(&["rev-parse", "HEAD"])
    }
}

fn run(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}
```

- [ ] **Step 2: 실패하는 테스트 작성**

`crates/core/tests/base.rs`:
```rust
mod common;

use common::TestRepo;
use wtboard_core::git::base::{merge_base, resolve_base};
use wtboard_core::git::Git;

#[test]
fn configured_base_wins() {
    let repo = TestRepo::new();
    assert_eq!(resolve_base(&Git::new(repo.path()), Some("main")).unwrap(), "main");
}

#[test]
fn falls_back_to_origin_head() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    assert_eq!(resolve_base(&Git::new(clone.path()), None).unwrap(), "origin/main");
}

#[test]
fn explains_how_to_fix_when_no_base_can_be_found() {
    let repo = TestRepo::new(); // origin 없음, config base 없음
    let err = resolve_base(&Git::new(repo.path()), None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("base") && msg.contains("config.toml"), "{msg}");
}

#[test]
fn merge_base_is_the_fork_point() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let fork = clone.head();
    clone.git(&["checkout", "-q", "-b", "feat"]);
    clone.write("a.txt", "a\n");
    clone.commit_all("feat");
    assert_eq!(merge_base(&Git::new(clone.path()), "origin/main").unwrap(), fork);
}
```

- [ ] **Step 3: 실패 확인**

Run: `cargo test -p wtboard-core --test base`
Expected: FAIL — `could not find base in git`.

- [ ] **Step 4: 구현**

`crates/core/src/git/base.rs`:
```rust
use super::{Git, GitError};

#[derive(Debug, thiserror::Error)]
pub enum BaseError {
    #[error(
        "base 브랜치를 정할 수 없음: ~/.wtboard/config.toml의 [[repo]]에 base를 적거나, \
         레포에서 `git remote set-head origin --auto`를 실행하세요"
    )]
    NotFound,
    #[error(transparent)]
    Git(#[from] GitError),
}

/// base 결정 순서: config의 `base` → `origin/HEAD`. (`gh pr view` 단계는 2단계에서 추가)
pub fn resolve_base(git: &Git, configured: Option<&str>) -> Result<String, BaseError> {
    if let Some(base) = configured {
        return Ok(base.to_owned());
    }
    git.run_str(&["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .map_err(|_| BaseError::NotFound)
}

pub fn merge_base(git: &Git, base: &str) -> Result<String, GitError> {
    git.run_str(&["merge-base", base, "HEAD"])
}
```

`crates/core/src/git/mod.rs` 맨 위에 추가:
```rust
pub mod base;
```

- [ ] **Step 5: 통과 확인**

Run: `cargo test -p wtboard-core --test base`
Expected: PASS (4 tests).

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -m "feat(core): base와 merge-base 결정"
```

---

### Task 4: 변경 파일 목록

merge-base와 **현재 작업 트리**를 비교해 커밋된 변경과 커밋 안 된 변경을 한 번에 얻고, untracked 파일을 더한다. 경로를 주면 그 경로로 제한한다 (watcher가 파일 하나만 다시 볼 때 사용).

**Files:**
- Create: `crates/core/src/domain.rs`, `crates/core/src/git/changes.rs`
- Modify: `crates/core/src/lib.rs`, `crates/core/src/git/mod.rs`
- Test: `crates/core/tests/changes.rs`

**Interfaces:**
- Consumes: `Git`, `DIFF_FLAGS` (Task 1)
- Produces:
  - `domain::MAX_DIFF_BYTES: u64 = 1_048_576`
  - `domain::FileStatus { Added, Modified, Deleted, Untracked }` (JSON: 소문자)
  - `domain::ChangedFile { path: String, status: FileStatus, additions: Option<u32>, deletions: Option<u32> }` (JSON: camelCase. `additions == None`이면 바이너리 또는 1MB 넘는 untracked)
  - `git::changes::list_changes(git: &Git, merge_base: &str, paths: &[&str]) -> Result<Vec<ChangedFile>, GitError>` — path 순 정렬. `paths`가 비면 전체

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/changes.rs`:
```rust
mod common;

use common::TestRepo;
use wtboard_core::domain::{ChangedFile, FileStatus};
use wtboard_core::git::changes::list_changes;
use wtboard_core::git::Git;

/// main에 keep.txt, gone.txt, .gitignore를 커밋하고 feat 브랜치로 옮긴다. merge-base sha를 돌려준다.
fn setup() -> (TestRepo, String) {
    let repo = TestRepo::new();
    repo.write("keep.txt", "1\n2\n3\n");
    repo.write("gone.txt", "bye\n");
    repo.write(".gitignore", "ignored/\n");
    repo.commit_all("base");
    let mb = repo.head();
    repo.git(&["checkout", "-q", "-b", "feat"]);
    (repo, mb)
}

fn find<'a>(files: &'a [ChangedFile], path: &str) -> &'a ChangedFile {
    files.iter().find(|f| f.path == path).unwrap_or_else(|| panic!("{path} not in {files:?}"))
}

#[test]
fn combines_committed_uncommitted_and_untracked() {
    let (repo, mb) = setup();
    repo.write("committed.txt", "c\n");
    repo.commit_all("feat commit");
    repo.write("keep.txt", "1\nTWO\n3\n4\n"); // 커밋 안 된 수정
    repo.remove("gone.txt");
    repo.write("new.txt", "a\nb\n"); // untracked
    repo.write("ignored/x.txt", "x\n"); // gitignore 대상

    let files = list_changes(&Git::new(repo.path()), &mb, &[]).unwrap();

    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["committed.txt", "gone.txt", "keep.txt", "new.txt"]);
    assert_eq!(find(&files, "committed.txt").status, FileStatus::Added);
    assert_eq!(find(&files, "gone.txt").status, FileStatus::Deleted);
    let keep = find(&files, "keep.txt");
    assert_eq!((keep.status, keep.additions, keep.deletions), (FileStatus::Modified, Some(2), Some(1)));
    let new = find(&files, "new.txt");
    assert_eq!((new.status, new.additions, new.deletions), (FileStatus::Untracked, Some(2), Some(0)));
}

#[test]
fn binary_files_have_no_line_counts() {
    let (repo, mb) = setup();
    repo.write("img.bin", [0u8, 1, 2, 0, 255]);
    repo.commit_all("bin");
    repo.write("untracked.bin", [0u8, 9, 0]);
    let files = list_changes(&Git::new(repo.path()), &mb, &[]).unwrap();
    assert_eq!(find(&files, "img.bin").additions, None);
    assert_eq!(find(&files, "untracked.bin").additions, None);
}

#[test]
fn unusual_file_names_are_listed_and_matched_literally() {
    let (repo, mb) = setup();
    let names = ["with space.txt", "한글.txt", "star*.txt", "[brackets].txt"];
    for name in names {
        repo.write(name, "x\n");
    }
    repo.commit_all("names");
    repo.write("star.txt", "glob이었다면 star*.txt에 걸린다\n");
    let git = Git::new(repo.path());

    let all = list_changes(&git, &mb, &[]).unwrap();
    for name in names {
        find(&all, name);
    }
    let only = list_changes(&git, &mb, &["star*.txt"]).unwrap();
    assert_eq!(only.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["star*.txt"]);
}

#[test]
fn pathspec_returns_nothing_for_clean_or_ignored_paths() {
    let (repo, mb) = setup();
    repo.write("keep.txt", "changed\n");
    repo.write("ignored/x.txt", "x\n");
    let git = Git::new(repo.path());
    assert_eq!(list_changes(&git, &mb, &["keep.txt"]).unwrap().len(), 1);
    assert!(list_changes(&git, &mb, &["README.md"]).unwrap().is_empty());
    assert!(list_changes(&git, &mb, &["ignored/x.txt"]).unwrap().is_empty());
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test changes`
Expected: FAIL — `unresolved import wtboard_core::domain`.

- [ ] **Step 3: 구현**

`crates/core/src/domain.rs`:
```rust
use serde::Serialize;

/// 이보다 큰 파일은 diff 대신 크기 변화만 보여 준다
pub const MAX_DIFF_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Untracked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    pub status: FileStatus,
    /// `None`이면 바이너리이거나, `MAX_DIFF_BYTES`를 넘어 줄 수를 세지 않은 untracked 파일
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
}
```

`crates/core/src/git/changes.rs`:
```rust
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

/// untracked 텍스트 파일의 줄 수. 바이너리이거나 너무 크면 `None`
fn count_text_lines(path: &Path) -> Option<u32> {
    if fs::metadata(path).ok()?.len() > MAX_DIFF_BYTES {
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
```

`crates/core/src/git/mod.rs` 맨 위 모듈 선언을 다음으로 바꾼다:
```rust
pub mod base;
pub mod changes;
```

`crates/core/src/lib.rs`에 추가:
```rust
pub mod domain;
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test changes`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): merge-base 대비 변경 파일 목록"
```

---

### Task 5: 파일 하나의 diff + hash

**Files:**
- Create: `crates/core/src/git/patch.rs`
- Modify: `crates/core/src/domain.rs`, `crates/core/src/git/mod.rs`
- Test: `crates/core/tests/patch.rs`

**Interfaces:**
- Consumes: `Git`, `DIFF_FLAGS`, `ChangedFile`, `FileStatus`, `MAX_DIFF_BYTES`, `list_changes` (Task 1, 4)
- Produces:
  - `domain::PatchBody` — `Text { diff: String }` / `Binary { old_size: Option<u64>, new_size: Option<u64> }` / `TooLarge { old_size, new_size }`. JSON: `{"kind":"text","diff":...}`, `{"kind":"tooLarge","oldSize":1,"newSize":null}`
  - `domain::FilePatch { path: String, body: PatchBody, hash: String }`
  - `git::patch::file_patch(git: &Git, merge_base: &str, file: &ChangedFile) -> Result<FilePatch, GitError>`

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/patch.rs`:
```rust
mod common;

use common::TestRepo;
use wtboard_core::domain::{FilePatch, PatchBody, MAX_DIFF_BYTES};
use wtboard_core::git::changes::list_changes;
use wtboard_core::git::patch::file_patch;
use wtboard_core::git::Git;

fn patch_for(repo: &TestRepo, mb: &str, path: &str) -> FilePatch {
    let git = Git::new(repo.path());
    let file = list_changes(&git, mb, &[path]).unwrap().into_iter().next().expect("file should be changed");
    file_patch(&git, mb, &file).unwrap()
}

fn text(p: &FilePatch) -> String {
    match &p.body {
        PatchBody::Text { diff } => diff.clone(),
        other => panic!("expected text, got {other:?}"),
    }
}

#[test]
fn tracked_diff_includes_committed_and_uncommitted() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("a.py", "one\n");
    repo.commit_all("add");
    repo.write("a.py", "one\ntwo\n");
    let d = text(&patch_for(&repo, &mb, "a.py"));
    assert!(d.contains("+one") && d.contains("+two"), "{d}");
}

#[test]
fn untracked_diff_is_against_dev_null() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("new file.py", "hi\n");
    let d = text(&patch_for(&repo, &mb, "new file.py"));
    assert!(d.contains("--- /dev/null") && d.contains("+hi"), "{d}");
}

#[test]
fn deleted_file_diff() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.remove("README.md");
    assert!(text(&patch_for(&repo, &mb, "README.md")).contains("-hello"));
}

#[test]
fn binary_and_large_files_skip_text_diff() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("img.bin", [0u8, 1, 0, 2]);
    repo.write("big.txt", "x\n".repeat(MAX_DIFF_BYTES as usize / 2 + 1));
    assert!(matches!(
        patch_for(&repo, &mb, "img.bin").body,
        PatchBody::Binary { old_size: None, new_size: Some(4) }
    ));
    assert!(matches!(patch_for(&repo, &mb, "big.txt").body, PatchBody::TooLarge { .. }));
}

#[test]
fn hash_changes_only_when_diff_changes() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.write("README.md", "hello\nworld\n");
    let h1 = patch_for(&repo, &mb, "README.md").hash;
    assert_eq!(patch_for(&repo, &mb, "README.md").hash, h1);
    repo.write("README.md", "hello\nWORLD\n"); // +1/-0, status M 그대로. 내용만 다름
    assert_ne!(patch_for(&repo, &mb, "README.md").hash, h1);
}

#[test]
fn user_git_config_does_not_change_output() {
    let repo = TestRepo::new();
    let mb = repo.head();
    repo.git(&["config", "color.ui", "always"]);
    repo.git(&["config", "color.diff", "always"]);
    repo.git(&["config", "diff.noprefix", "true"]);
    repo.git(&["config", "diff.mnemonicPrefix", "true"]);
    repo.write("README.md", "hello\nworld\n");
    let d = text(&patch_for(&repo, &mb, "README.md"));
    assert!(!d.contains('\u{1b}'), "color codes leaked: {d:?}");
    assert!(d.contains("--- a/README.md") && d.contains("+++ b/README.md"), "{d}");
}

#[test]
fn serializes_for_the_ui() {
    let body = PatchBody::TooLarge { old_size: Some(1), new_size: None };
    assert_eq!(
        serde_json::to_value(&body).unwrap(),
        serde_json::json!({"kind": "tooLarge", "oldSize": 1, "newSize": null})
    );
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test patch`
Expected: FAIL — `cannot find FilePatch in domain`.

- [ ] **Step 3: 구현**

`crates/core/src/domain.rs` 끝에 추가:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum PatchBody {
    Text { diff: String },
    Binary { old_size: Option<u64>, new_size: Option<u64> },
    TooLarge { old_size: Option<u64>, new_size: Option<u64> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilePatch {
    pub path: String,
    pub body: PatchBody,
    /// diff 내용의 blake3 hash. 바이너리/큰 파일은 파일 내용 기준
    pub hash: String,
}
```

`crates/core/src/git/patch.rs`:
```rust
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
```

`crates/core/src/git/mod.rs` 모듈 선언:
```rust
pub mod base;
pub mod changes;
pub mod patch;
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test patch`
Expected: PASS (7 tests). `user_git_config_does_not_change_output`가 실패하면 `DIFF_FLAGS`/`GLOBAL_ARGS`에 빠진 override가 있는 것이다.

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): 파일 diff와 hash"
```

---

### Task 6: 감시 경로 layout + event 분류

worktree 안의 `.git`은 파일이고 실제 HEAD/index는 메인 clone의 `.git/worktrees/<이름>/`에 있다. 그래서 감시 위치가 세 군데다. 이 task는 FS 없이 테스트할 수 있는 순수 로직(경로 → 무엇을 다시 계산할지)만 만든다.

참고: 커밋은 `HEAD` 파일이 아니라 `refs/heads/<브랜치>`를 바꾼다 (HEAD는 `ref: refs/heads/...` symref). 그래서 refs 변경은 fetch뿐 아니라 커밋도 잡는다. spec 8절 표의 "git-dir (HEAD, index) → 커밋"은 실제로는 refs 감시가 담당한다.

**Files:**
- Create: `crates/core/src/profile.rs`, `crates/core/src/watch/mod.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/watch_layout.rs`

**Interfaces:**
- Consumes: `Git`, `GitError::Unexpected` (Task 1)
- Produces:
  - `profile::COMMON_EXCLUDED_DIRS: &[&str]`
  - `watch::Target { File(String), List, Base, Ignore }` — `File`의 경로는 worktree root 기준 상대 경로, `/` 구분, NFC
  - `watch::WatchLayout { pub root, pub git_dir, pub common_dir }` (모두 canonical + NFC)
    - `WatchLayout::detect(git: &Git, excluded_dirs: &[&str]) -> Result<WatchLayout, GitError>`
    - `fn classify(&self, path: &Path) -> Target`
    - `fn watch_roots(&self) -> Vec<(PathBuf, notify::RecursiveMode)>`

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/watch_layout.rs`:
```rust
mod common;

use std::path::Path;

use common::TestRepo;
use unicode_normalization::UnicodeNormalization;
use wtboard_core::git::Git;
use wtboard_core::watch::{Target, WatchLayout};

fn layout(path: &Path) -> WatchLayout {
    WatchLayout::detect(&Git::new(path), &["node_modules"]).unwrap()
}

fn file(p: &str) -> Target {
    Target::File(p.into())
}

#[test]
fn main_worktree_classification() {
    let repo = TestRepo::new();
    let l = layout(repo.path());
    let r = &l.root;
    assert_eq!(l.classify(&r.join("src/app.py")), file("src/app.py"));
    assert_eq!(l.classify(&r.join("Cargo.lock")), file("Cargo.lock")); // 작업 폴더의 .lock은 일반 파일
    assert_eq!(l.classify(&r.join("web/node_modules/x/index.js")), Target::Ignore);
    assert_eq!(l.classify(&r.join(".git/index")), Target::List);
    assert_eq!(l.classify(&r.join(".git/HEAD")), Target::List);
    assert_eq!(l.classify(&r.join(".git/index.lock")), Target::Ignore);
    assert_eq!(l.classify(&r.join(".git/refs/heads/main")), Target::Base);
    assert_eq!(l.classify(&r.join(".git/refs/remotes/origin/main")), Target::Base);
    assert_eq!(l.classify(&r.join(".git/packed-refs")), Target::Base);
    assert_eq!(l.classify(&r.join(".git/objects/ab/cdef")), Target::Ignore);
    assert_eq!(l.classify(&r.join(".git/logs/HEAD")), Target::Ignore);
    assert_eq!(l.classify(Path::new("/somewhere/else.txt")), Target::Ignore);
    assert_eq!(l.watch_roots().len(), 1); // .git이 root 안에 있으므로 root 하나로 충분
}

#[test]
fn linked_worktree_uses_its_own_git_dir_and_shared_refs() {
    let repo = TestRepo::new();
    let wt_path = repo.path().parent().unwrap().join("wt-feat");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    let l = layout(&wt_path);

    assert_ne!(l.git_dir, l.common_dir);
    assert_eq!(l.classify(&l.git_dir.join("HEAD")), Target::List);
    assert_eq!(l.classify(&l.git_dir.join("index")), Target::List);
    assert_eq!(l.classify(&l.common_dir.join("refs/heads/feat")), Target::Base);
    assert_eq!(l.classify(&l.common_dir.join("refs/heads/feat.lock")), Target::Ignore);
    assert_eq!(l.classify(&l.common_dir.join("objects/ab/cd")), Target::Ignore);
    assert_eq!(l.classify(&l.root.join(".git")), Target::Ignore); // linked worktree의 .git 파일
    assert_eq!(l.classify(&l.root.join("a.txt")), file("a.txt"));

    let roots: Vec<_> = l.watch_roots().into_iter().map(|(p, _)| p).collect();
    assert!(roots.contains(&l.root));
    assert!(roots.contains(&l.git_dir));
    assert!(roots.contains(&l.common_dir.join("refs")));
}

#[test]
fn paths_are_canonical_and_nfc() {
    let repo = TestRepo::new();
    let l = layout(repo.path());
    // macOS tempdir은 /var/... → /private/var/... symlink
    assert_eq!(l.root, repo.path().canonicalize().unwrap());
    let nfd: String = "한글.txt".nfd().collect();
    assert_ne!(nfd, "한글.txt");
    assert_eq!(l.classify(&l.root.join(&nfd)), file("한글.txt"));
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test watch_layout`
Expected: FAIL — `unresolved import wtboard_core::watch`.

- [ ] **Step 3: 구현**

`crates/core/src/profile.rs`:
```rust
/// 모든 프로젝트에서 감시하지 않는 폴더. 이름이 같으면 깊이와 관계없이 제외한다.
/// `build`, `out`, `ios/Pods`처럼 프로젝트마다 의미가 다른 폴더는 2단계 프로필에서 다룬다.
pub const COMMON_EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    ".mypy_cache",
    ".next",
    ".turbo",
    ".dart_tool",
    ".fvm",
];
```

`crates/core/src/watch/mod.rs`:
```rust
use std::path::{Component, Path, PathBuf};

use notify::RecursiveMode;
use unicode_normalization::UnicodeNormalization;

use crate::git::{Git, GitError};

/// FS event 경로 하나가 요구하는 재계산 범위
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// 작업 폴더 파일. 그 파일만 다시 계산
    File(String),
    /// HEAD / index 변경. 파일 목록 전체 다시 계산
    List,
    /// refs 변경 (fetch, 커밋). merge-base부터 다시 계산
    Base,
    Ignore,
}

#[derive(Debug, Clone)]
pub struct WatchLayout {
    pub root: PathBuf,
    pub git_dir: PathBuf,
    pub common_dir: PathBuf,
    excluded_dirs: Vec<String>,
}

impl WatchLayout {
    pub fn detect(git: &Git, excluded_dirs: &[&str]) -> Result<Self, GitError> {
        let out = git.run_str(&[
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
        ])?;
        let mut lines = out.lines().map(canonical);
        let (Some(root), Some(git_dir), Some(common_dir)) = (lines.next(), lines.next(), lines.next()) else {
            return Err(GitError::Unexpected(format!("rev-parse: {out}")));
        };
        Ok(Self {
            root,
            git_dir,
            common_dir,
            excluded_dirs: excluded_dirs.iter().map(|s| s.to_string()).collect(),
        })
    }

    pub fn classify(&self, path: &Path) -> Target {
        let path = nfc(path);
        if let Ok(rel) = path.strip_prefix(&self.git_dir) {
            return classify_git_dir(rel, self.git_dir == self.common_dir);
        }
        if let Ok(rel) = path.strip_prefix(&self.common_dir) {
            return classify_common_dir(rel);
        }
        let Ok(rel) = path.strip_prefix(&self.root) else {
            return Target::Ignore;
        };
        let parts: Vec<&str> = rel
            .components()
            .filter_map(|c| match c {
                Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        let excluded = |p: &&str| *p == ".git" || self.excluded_dirs.iter().any(|e| e == p);
        if parts.is_empty() || parts.iter().any(excluded) {
            return Target::Ignore;
        }
        Target::File(parts.join("/"))
    }

    /// 감시할 위치. root 안에 들어 있는 위치는 root 감시로 충분하므로 뺀다.
    pub fn watch_roots(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut roots = vec![(self.root.clone(), RecursiveMode::Recursive)];
        if !self.git_dir.starts_with(&self.root) {
            roots.push((self.git_dir.clone(), RecursiveMode::Recursive));
        }
        if !self.common_dir.starts_with(&self.root) {
            roots.push((self.common_dir.join("refs"), RecursiveMode::Recursive));
            roots.push((self.common_dir.clone(), RecursiveMode::NonRecursive)); // packed-refs
        }
        roots
    }
}

/// git-dir 안의 경로. 메인 worktree는 git-dir == common-dir이라 refs도 여기서 판단한다.
fn classify_git_dir(rel: &Path, is_common: bool) -> Target {
    if is_lock(rel) {
        return Target::Ignore;
    }
    match first(rel) {
        Some("HEAD" | "index") => Target::List,
        Some("refs" | "packed-refs") if is_common => Target::Base,
        _ => Target::Ignore,
    }
}

fn classify_common_dir(rel: &Path) -> Target {
    if is_lock(rel) {
        return Target::Ignore;
    }
    match first(rel) {
        Some("refs" | "packed-refs") => Target::Base,
        _ => Target::Ignore,
    }
}

fn first(rel: &Path) -> Option<&str> {
    rel.components().next().and_then(|c| c.as_os_str().to_str())
}

/// git 내부 lock 파일 (`index.lock`, `refs/heads/x.lock`). 작업 폴더에는 적용하지 않는다.
fn is_lock(rel: &Path) -> bool {
    rel.extension().is_some_and(|e| e == "lock")
}

fn canonical(p: &str) -> PathBuf {
    let p = Path::new(p);
    nfc(&p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
}

/// macOS FSEvents는 NFD 경로를 줄 수 있고 git(core.precomposeUnicode)은 NFC를 쓴다
fn nfc(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().nfc().collect::<String>())
}
```

`crates/core/src/lib.rs`에 추가:
```rust
pub mod profile;
pub mod watch;
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test watch_layout`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): 감시 경로 layout과 event 분류"
```

---

### Task 7: worktree 상태와 refresh

worktree 하나의 현재 상태(base, merge-base, 변경 목록, 화면이 본 파일의 hash)를 들고, 분류된 event를 받아 무엇이 바뀌었는지 계산한다.

hash 규칙: hash는 **화면이 patch를 가져간 파일만** 저장한다. 화면이 안 본 파일은 목록 항목(status, +/-)이 바뀔 때만 알린다. 이렇게 하면 lazy loading을 지키면서도 "이미 M인 파일을 또 고친" 경우를 본 파일에 대해서는 정확히 잡는다.

**Files:**
- Create: `crates/core/src/worktree.rs`
- Modify: `crates/core/src/lib.rs`
- Test: `crates/core/tests/worktree.rs`

**Interfaces:**
- Consumes: `Git`, `resolve_base`, `merge_base`, `BaseError`, `list_changes`, `file_patch`, `WatchLayout`, `Target`, `COMMON_EXCLUDED_DIRS`
- Produces:
  - `Worktree::open(path: &Path, configured_base: Option<String>) -> Result<Arc<Worktree>, WorktreeError>`
  - `fn id(&self) -> &str` (canonical root 경로), `fn layout(&self) -> &WatchLayout`
  - `fn files(&self) -> Vec<ChangedFile>`, `fn summary(&self) -> WorktreeSummary`
  - `fn patch(&self, path: &str) -> Result<FilePatch, WorktreeError>` (hash 저장)
  - `fn handle(&self, targets: &[Target]) -> Result<Option<WorktreeChanged>, WorktreeError>`
  - `ChangeScope { File, List, Base }` (`File < List < Base`, JSON 소문자)
  - `WorktreeChanged { worktree_id: String, changed_paths: Vec<String>, scope: ChangeScope }` (JSON camelCase — spec 12절 `worktree-changed` payload)
  - `WorktreeSummary { id, path, branch: Option<String>, base, changed_files: usize }` (JSON camelCase)
  - `WorktreeError { Missing(PathBuf), Git(GitError), Base(BaseError), UnknownFile(String) }`

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/worktree.rs`:
```rust
mod common;

use std::path::Path;

use common::TestRepo;
use wtboard_core::watch::Target;
use wtboard_core::worktree::{ChangeScope, Worktree};

/// main에 src/a.py, src/b.py를 커밋하고 feat 브랜치로 옮긴 레포
fn feat_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.write("src/a.py", "a = 1\n");
    repo.write("src/b.py", "b = 1\n");
    repo.commit_all("base");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo
}

fn open(repo: &TestRepo) -> std::sync::Arc<Worktree> {
    Worktree::open(repo.path(), Some("main".into())).unwrap()
}

fn file(p: &str) -> Target {
    Target::File(p.into())
}

fn paths(wt: &Worktree) -> Vec<String> {
    wt.files().into_iter().map(|f| f.path).collect()
}

#[test]
fn opens_with_current_changes() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let s = open(&repo).summary();
    assert_eq!((s.branch.as_deref(), s.base.as_str(), s.changed_files), (Some("feat"), "main", 1));
}

#[test]
fn open_reports_missing_path() {
    let err = Worktree::open(Path::new("/nonexistent/repo"), None).err().unwrap();
    assert!(err.to_string().contains("/nonexistent/repo"), "{err}");
}

#[test]
fn file_edit_emits_file_scope() {
    let repo = feat_repo();
    let wt = open(&repo);
    repo.write("src/a.py", "a = 2\n");
    let ev = wt.handle(&[file("src/a.py")]).unwrap().expect("should emit");
    assert_eq!((ev.scope, ev.changed_paths), (ChangeScope::File, vec!["src/a.py".to_string()]));
    assert_eq!(paths(&wt), ["src/a.py"]);
}

#[test]
fn re_editing_a_viewed_modified_file_is_caught_by_hash() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.patch("src/a.py").unwrap(); // 화면에 보였음 → hash 저장
    repo.write("src/a.py", "a = 3\n"); // +1/-1, status M 그대로
    let ev = wt.handle(&[file("src/a.py")]).unwrap().expect("hash change should emit");
    assert_eq!(ev.changed_paths, vec!["src/a.py".to_string()]);
}

#[test]
fn events_without_real_change_are_silent() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    wt.patch("src/a.py").unwrap();
    assert!(wt.handle(&[file("src/a.py")]).unwrap().is_none()); // touch만
    assert!(wt.handle(&[file("src/b.py")]).unwrap().is_none()); // 깨끗한 파일
    repo.git(&["add", "-A"]);
    assert!(wt.handle(&[Target::List]).unwrap().is_none()); // stage는 브랜치 뷰를 안 바꾼다
    repo.git(&["commit", "-q", "-m", "wip"]);
    assert!(wt.handle(&[Target::Base]).unwrap().is_none()); // 커밋도 마찬가지
}

#[test]
fn reverting_a_file_removes_it_from_the_list() {
    let repo = feat_repo();
    repo.write("src/a.py", "a = 2\n");
    let wt = open(&repo);
    repo.write("src/a.py", "a = 1\n");
    let ev = wt.handle(&[file("src/a.py")]).unwrap().unwrap();
    assert_eq!(ev.changed_paths, vec!["src/a.py".to_string()]);
    assert!(wt.files().is_empty());
}

#[test]
fn deleting_a_directory_falls_back_to_full_list() {
    let repo = feat_repo();
    let wt = open(&repo);
    repo.remove("src"); // rm -rf src: event가 폴더 경로 하나로 올 수 있다
    let ev = wt.handle(&[file("src")]).unwrap().unwrap();
    assert_eq!(ev.scope, ChangeScope::List);
    assert_eq!(paths(&wt), ["src/a.py", "src/b.py"]);
}

#[test]
fn creating_a_directory_falls_back_to_full_list() {
    let repo = feat_repo();
    let wt = open(&repo);
    repo.write("gen/models/user.g.dart", "// gen\n"); // codegen이 폴더째 생성
    let ev = wt.handle(&[file("gen")]).unwrap().unwrap();
    assert_eq!(ev.scope, ChangeScope::List);
    assert_eq!(paths(&wt), ["gen/models/user.g.dart"]);
}

#[test]
fn moving_fork_point_recomputes_from_merge_base() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    clone.git(&["checkout", "-q", "-b", "feat"]);
    clone.write("feat.txt", "f\n");
    clone.commit_all("feat");
    let wt = Worktree::open(clone.path(), None).unwrap();
    assert_eq!(wt.summary().base, "origin/main");

    upstream.write("up.txt", "u\n");
    upstream.commit_all("upstream");
    clone.git(&["fetch", "-q"]);
    clone.git(&["merge", "-q", "--no-edit", "origin/main"]);
    let ev = wt.handle(&[Target::Base]).unwrap().unwrap();
    assert_eq!(ev.scope, ChangeScope::Base);
    assert_eq!(paths(&wt), ["feat.txt"]); // up.txt는 base에도 있으므로 변경 아님
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test worktree`
Expected: FAIL — `unresolved import wtboard_core::worktree`.

- [ ] **Step 3: 구현**

`crates/core/src/worktree.rs`:
```rust
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;

use crate::domain::{ChangedFile, FilePatch};
use crate::git::base::{merge_base, resolve_base, BaseError};
use crate::git::changes::list_changes;
use crate::git::patch::file_patch;
use crate::git::{Git, GitError};
use crate::profile::COMMON_EXCLUDED_DIRS;
use crate::watch::{Target, WatchLayout};

#[derive(Debug, thiserror::Error)]
pub enum WorktreeError {
    #[error("경로가 없음: {0}")]
    Missing(PathBuf),
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Base(#[from] BaseError),
    #[error("변경 목록에 없는 파일: {0}")]
    UnknownFile(String),
}

/// 재계산 범위. 여러 개를 합칠 때는 큰 쪽을 따른다 (`File < List < Base`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeScope {
    File,
    List,
    Base,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeChanged {
    pub worktree_id: String,
    pub changed_paths: Vec<String>,
    pub scope: ChangeScope,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub id: String,
    pub path: String,
    pub branch: Option<String>,
    pub base: String,
    pub changed_files: usize,
}

struct State {
    base: String,
    merge_base: String,
    files: BTreeMap<String, ChangedFile>,
    /// 화면이 patch를 가져간 파일의 마지막 hash
    hashes: HashMap<String, String>,
}

pub struct Worktree {
    id: String,
    git: Git,
    layout: WatchLayout,
    configured_base: Option<String>,
    state: Mutex<State>,
}

impl Worktree {
    pub fn open(path: &Path, configured_base: Option<String>) -> Result<Arc<Self>, WorktreeError> {
        if !path.is_dir() {
            return Err(WorktreeError::Missing(path.to_path_buf()));
        }
        let layout = WatchLayout::detect(&Git::new(path), COMMON_EXCLUDED_DIRS)?;
        let git = Git::new(&layout.root);
        let base = resolve_base(&git, configured_base.as_deref())?;
        let mb = merge_base(&git, &base)?;
        let files = to_map(list_changes(&git, &mb, &[])?);
        Ok(Arc::new(Self {
            id: layout.root.to_string_lossy().into_owned(),
            git,
            layout,
            configured_base,
            state: Mutex::new(State { base, merge_base: mb, files, hashes: HashMap::new() }),
        }))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn layout(&self) -> &WatchLayout {
        &self.layout
    }

    pub fn files(&self) -> Vec<ChangedFile> {
        self.state.lock().unwrap().files.values().cloned().collect()
    }

    pub fn summary(&self) -> WorktreeSummary {
        let branch = self
            .git
            .run_str(&["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .filter(|b| b != "HEAD"); // detached HEAD
        let st = self.state.lock().unwrap();
        WorktreeSummary {
            id: self.id.clone(),
            path: self.id.clone(),
            branch,
            base: st.base.clone(),
            changed_files: st.files.len(),
        }
    }

    pub fn patch(&self, path: &str) -> Result<FilePatch, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        let file = st.files.get(path).cloned().ok_or_else(|| WorktreeError::UnknownFile(path.to_owned()))?;
        let patch = file_patch(&self.git, &st.merge_base, &file)?;
        st.hashes.insert(path.to_owned(), patch.hash.clone());
        Ok(patch)
    }

    /// debounce된 event 묶음 하나를 처리한다. 화면에 알릴 변화가 없으면 `None`.
    pub fn handle(&self, targets: &[Target]) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut paths: Vec<String> = targets
            .iter()
            .filter_map(|t| match t {
                Target::File(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        paths.sort();
        paths.dedup();

        let mut events = Vec::new();
        if targets.contains(&Target::Base) {
            events.push(self.refresh_base()?);
        } else if targets.contains(&Target::List) {
            events.push(self.refresh_list()?);
        }
        // 목록 재계산은 status/+- 만 비교하므로, 같은 묶음의 파일 event는 hash 비교를 따로 한다
        if !paths.is_empty() {
            events.push(self.refresh_paths(&paths)?);
        }
        Ok(merge(events.into_iter().flatten()))
    }

    fn refresh_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let base = resolve_base(&self.git, self.configured_base.as_deref())?;
        let mb = merge_base(&self.git, &base)?;
        let unchanged = {
            let st = self.state.lock().unwrap();
            st.base == base && st.merge_base == mb
        };
        if unchanged {
            return self.refresh_list();
        }
        let files = to_map(list_changes(&self.git, &mb, &[])?);
        *self.state.lock().unwrap() = State { base, merge_base: mb, files, hashes: HashMap::new() };
        Ok(Some(self.event(ChangeScope::Base, Vec::new())))
    }

    fn refresh_list(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        let fresh = to_map(list_changes(&self.git, &st.merge_base, &[])?);
        let changed: Vec<String> = st
            .files
            .keys()
            .chain(fresh.keys())
            .filter(|p| st.files.get(*p) != fresh.get(*p))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if changed.is_empty() {
            return Ok(None);
        }
        for p in &changed {
            st.hashes.remove(p);
        }
        st.files = fresh;
        Ok(Some(self.event(ChangeScope::List, changed)))
    }

    fn refresh_paths(&self, paths: &[String]) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        // 폴더 경로 event (rm -rf로 사라진 폴더)
        let has_children = paths.iter().any(|p| {
            let prefix = format!("{p}/");
            st.files.keys().any(|k| k.starts_with(&prefix))
        });
        if has_children {
            drop(st);
            return self.refresh_list();
        }

        let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
        let fresh = to_map(list_changes(&self.git, &st.merge_base, &refs)?);
        // 폴더 경로 event (새로 생긴 폴더): pathspec이 그 아래 파일들을 돌려준다
        if fresh.keys().any(|k| !paths.contains(k)) {
            drop(st);
            return self.refresh_list();
        }

        let mut changed = Vec::new();
        for p in paths {
            let after = fresh.get(p);
            let mut differs = st.files.get(p) != after;
            if !differs {
                if let (Some(file), Some(known)) = (after, st.hashes.get(p)) {
                    differs = file_patch(&self.git, &st.merge_base, file)?.hash != *known;
                }
            }
            if differs {
                st.hashes.remove(p);
                match after {
                    Some(f) => {
                        st.files.insert(p.clone(), f.clone());
                    }
                    None => {
                        st.files.remove(p);
                    }
                }
                changed.push(p.clone());
            }
        }
        Ok((!changed.is_empty()).then(|| self.event(ChangeScope::File, changed)))
    }

    fn event(&self, scope: ChangeScope, changed_paths: Vec<String>) -> WorktreeChanged {
        WorktreeChanged { worktree_id: self.id.clone(), changed_paths, scope }
    }
}

fn to_map(files: Vec<ChangedFile>) -> BTreeMap<String, ChangedFile> {
    files.into_iter().map(|f| (f.path.clone(), f)).collect()
}

fn merge(events: impl Iterator<Item = WorktreeChanged>) -> Option<WorktreeChanged> {
    events.reduce(|mut acc, ev| {
        acc.scope = acc.scope.max(ev.scope);
        for p in ev.changed_paths {
            if !acc.changed_paths.contains(&p) {
                acc.changed_paths.push(p);
            }
        }
        acc
    })
}
```

`crates/core/src/lib.rs`에 추가:
```rust
pub mod worktree;
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test worktree`
Expected: PASS (10 tests).

- [ ] **Step 5: 전체 core 테스트**

Run: `cargo test -p wtboard-core`
Expected: 모든 테스트 PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/core
git commit -m "feat(core): worktree 상태와 event별 재계산"
```

---

### Task 8: watcher (FSEvents + debounce + poll)

**Files:**
- Create: `crates/core/src/watch/watcher.rs`
- Modify: `crates/core/src/watch/mod.rs`
- Test: `crates/core/tests/watcher.rs`

**Interfaces:**
- Consumes: `Worktree` (`layout()`, `handle()`, `id()`), `Target`, `WorktreeChanged`
- Produces:
  - `pub type EmitFn = Arc<dyn Fn(WorktreeChanged) + Send + Sync>`
  - `pub fn spawn_watcher(wt: Arc<Worktree>, poll: Duration, emit: EmitFn) -> notify::Result<WatchHandle>`
  - `pub struct WatchHandle` — drop하면 감시가 멈춘다. `Send`여야 한다 (Task 9에서 `Mutex`에 담음)

- [ ] **Step 1: 실패하는 테스트 작성**

`crates/core/tests/watcher.rs`:
```rust
mod common;

use std::path::Path;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use common::TestRepo;
use wtboard_core::watch::{spawn_watcher, WatchHandle};
use wtboard_core::worktree::{ChangeScope, Worktree, WorktreeChanged};

fn start(path: &Path) -> (WatchHandle, mpsc::Receiver<WorktreeChanged>) {
    let wt = Worktree::open(path, Some("main".into())).unwrap();
    let (tx, rx) = mpsc::channel();
    let handle = spawn_watcher(wt, Duration::from_secs(30), Arc::new(move |ev| {
        let _ = tx.send(ev);
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300)); // FSEvents stream 시작 대기
    (handle, rx)
}

#[test]
fn file_edit_reaches_emit_within_one_second() {
    let repo = TestRepo::new();
    repo.git(&["checkout", "-q", "-b", "feat"]);
    let (_h, rx) = start(repo.path());

    let started = Instant::now();
    repo.write("README.md", "hello\nchanged\n");
    let ev = rx.recv_timeout(Duration::from_secs(3)).expect("no event");
    let elapsed = started.elapsed();

    assert_eq!((ev.scope, ev.changed_paths), (ChangeScope::File, vec!["README.md".to_string()]));
    assert!(elapsed < Duration::from_secs(1), "took {elapsed:?}");
}

#[test]
fn excluded_and_gitignored_paths_stay_silent() {
    let repo = TestRepo::new();
    repo.write(".gitignore", "dist/\n");
    repo.commit_all("ignore");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    let (_h, rx) = start(repo.path());

    repo.write("node_modules/pkg/index.js", "x\n");
    repo.write("dist/bundle.js", "x\n");
    assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err());
}

#[test]
fn linked_worktree_edit_is_detected() {
    let repo = TestRepo::new();
    let wt_path = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    let (_h, rx) = start(&wt_path);

    std::fs::write(wt_path.join("new.txt"), "n\n").unwrap();
    let ev = rx.recv_timeout(Duration::from_secs(3)).expect("no event");
    assert_eq!(ev.changed_paths, vec!["new.txt".to_string()]);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test watcher`
Expected: FAIL — `cannot find function spawn_watcher in watch`.

- [ ] **Step 3: 구현**

`crates/core/src/watch/watcher.rs`:
```rust
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use notify::RecommendedWatcher;
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use super::Target;
use crate::worktree::{Worktree, WorktreeChanged};

pub type EmitFn = Arc<dyn Fn(WorktreeChanged) + Send + Sync>;

const DEBOUNCE: Duration = Duration::from_millis(300);

/// drop하면 감시가 멈추고 worker thread도 끝난다.
pub struct WatchHandle {
    _debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
}

enum Msg {
    Paths(Vec<PathBuf>),
    /// FSEvents가 event를 놓쳤다고 알림 → 전체 재계산
    Rescan,
}

pub fn spawn_watcher(wt: Arc<Worktree>, poll: Duration, emit: EmitFn) -> notify::Result<WatchHandle> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(DEBOUNCE, None, move |res: DebounceEventResult| {
        let msg = match res {
            Ok(events) if events.iter().any(|e| e.need_rescan()) => Msg::Rescan,
            Ok(events) => Msg::Paths(events.into_iter().flat_map(|e| e.event.paths).collect()),
            Err(_) => Msg::Rescan,
        };
        let _ = tx.send(msg);
    })?;
    for (path, mode) in wt.layout().watch_roots() {
        debouncer.watch(&path, mode)?;
    }
    std::thread::Builder::new()
        .name("wtboard-watch".into())
        .spawn(move || run_worker(wt, rx, poll, emit))
        .map_err(|e| notify::Error::io(e))?;
    Ok(WatchHandle { _debouncer: debouncer })
}

fn run_worker(wt: Arc<Worktree>, rx: mpsc::Receiver<Msg>, poll: Duration, emit: EmitFn) {
    let mut next_poll = Instant::now() + poll;
    loop {
        let targets: Vec<Target> = match rx.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
            Ok(Msg::Paths(paths)) => paths
                .iter()
                .map(|p| wt.layout().classify(p))
                .filter(|t| *t != Target::Ignore)
                .collect(),
            Ok(Msg::Rescan) => vec![Target::Base],
            // FSEvents 누락 대비 백업 재계산. Base는 merge-base가 그대로면 목록 재계산으로 끝난다
            Err(mpsc::RecvTimeoutError::Timeout) => {
                next_poll = Instant::now() + poll;
                vec![Target::Base]
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if targets.is_empty() {
            continue;
        }
        match wt.handle(&targets) {
            Ok(Some(ev)) => emit(ev),
            Ok(None) => {}
            Err(e) => eprintln!("[wtboard] {}: 재계산 실패: {e}", wt.id()),
        }
    }
}
```

`crates/core/src/watch/mod.rs` 맨 위에 추가:
```rust
mod watcher;

pub use watcher::{spawn_watcher, EmitFn, WatchHandle};
```

`notify-debouncer-full` 0.5 API가 위와 다르면 (예: `Debouncer::watch`가 없으면 `debouncer.watcher().watch(&path, mode)?`, `RecommendedCache` 대신 `FileIdMap`) `cargo doc -p notify-debouncer-full --open`으로 확인해 맞춘다. 동작은 바꾸지 않는다.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test watcher`
Expected: PASS (3 tests). `took ...` 로 실패하면 FSEvents latency 문제다. 수치를 기록하고, debounce를 줄이기 전에 사용자와 상의한다 (1초는 spec의 완료 기준).

- [ ] **Step 5: Commit**

```bash
git add crates/core
git commit -m "feat(core): FSEvents watcher와 백업 poll"
```

---

### Task 9: Tauri 앱 껍데기 + command / event 연결

**Files:**
- Create: `package.json`, `pnpm-workspace.yaml`, `crates/app/Cargo.toml`, `crates/app/build.rs`, `crates/app/tauri.conf.json`, `crates/app/capabilities/default.json`, `crates/app/app-icon.svg`, `crates/app/src/main.rs`, `ui/package.json`, `ui/vite.config.ts`, `ui/tsconfig.json`, `ui/index.html`, `ui/src/main.tsx`, `ui/src/index.css`, `ui/src/api.ts`, `ui/src/App.tsx`
- Modify: `Cargo.toml` (members)

**Interfaces:**
- Consumes: `Config`, `Worktree`, `WorktreeSummary`, `ChangedFile`, `FilePatch`, `spawn_watcher`, `EmitFn`, `WatchHandle`
- Produces (spec 12절, 1단계 부분):
  - command `list_repos() -> RepoSummary[]` — `{ name, worktrees: WorktreeSummary[] }`
  - command `list_files(worktreeId, view) -> ChangedFile[]` — `view`는 `"branch"`만
  - command `get_patch(worktreeId, view, path) -> FilePatch`
  - event `worktree-changed` — payload `WorktreeChanged`
  - `ui/src/api.ts`: 위 타입과 `listRepos`, `listFiles`, `getPatch`

- [ ] **Step 1: pnpm workspace + Tauri CLI**

`pnpm-workspace.yaml`:
```yaml
packages:
  - ui
```

`package.json`:
```json
{
  "name": "wtboard",
  "private": true,
  "scripts": {
    "tauri": "tauri",
    "test": "pnpm --dir ui test && cargo test"
  }
}
```

`ui/package.json`:
```json
{
  "name": "wtboard-ui",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc --noEmit && vite build",
    "test": "vitest run"
  }
}
```

```bash
pnpm add -Dw @tauri-apps/cli@^2
pnpm --dir ui add react react-dom @tauri-apps/api@^2 @tanstack/react-query@^5 zustand react-diff-view@^3 refractor@3.6.0
pnpm --dir ui add -D typescript vite @vitejs/plugin-react @types/react @types/react-dom tailwindcss @tailwindcss/vite vitest
```

- [ ] **Step 2: UI 최소 화면 작성**

`ui/vite.config.ts`:
```ts
/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  test: { environment: 'node' },
});
```

`ui/tsconfig.json`:
```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "isolatedModules": true,
    "types": ["vite/client"]
  },
  "include": ["src", "vite.config.ts"]
}
```

`ui/index.html`:
```html
<!doctype html>
<html lang="ko">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>wtboard</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`ui/src/index.css`:
```css
@import "tailwindcss";
```

`ui/src/api.ts`:
```ts
import { invoke } from '@tauri-apps/api/core';

export type FileStatus = 'added' | 'modified' | 'deleted' | 'untracked';

export interface ChangedFile {
  path: string;
  status: FileStatus;
  /** null이면 바이너리 (또는 1MB 넘는 untracked) */
  additions: number | null;
  deletions: number | null;
}

export type PatchBody =
  | { kind: 'text'; diff: string }
  | { kind: 'binary'; oldSize: number | null; newSize: number | null }
  | { kind: 'tooLarge'; oldSize: number | null; newSize: number | null };

export interface FilePatch {
  path: string;
  body: PatchBody;
  hash: string;
}

export interface WorktreeSummary {
  id: string;
  path: string;
  branch: string | null;
  base: string;
  changedFiles: number;
}

export interface RepoSummary {
  name: string;
  worktrees: WorktreeSummary[];
}

export type View = 'branch';

export interface WorktreeChanged {
  worktreeId: string;
  changedPaths: string[];
  scope: 'file' | 'list' | 'base';
}

export const listRepos = () => invoke<RepoSummary[]>('list_repos');

export const listFiles = (worktreeId: string, view: View) =>
  invoke<ChangedFile[]>('list_files', { worktreeId, view });

export const getPatch = (worktreeId: string, view: View, path: string) =>
  invoke<FilePatch>('get_patch', { worktreeId, view, path });
```

`ui/src/main.tsx`:
```tsx
import React from 'react';
import ReactDOM from 'react-dom/client';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { App } from './App';
import './index.css';

// 데이터는 worktree-changed event로만 갱신한다 (Task 10)
const queryClient = new QueryClient({
  defaultOptions: { queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false } },
});

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <App />
    </QueryClientProvider>
  </React.StrictMode>,
);
```

`ui/src/App.tsx` (Task 10에서 교체):
```tsx
import { useQuery } from '@tanstack/react-query';
import { listRepos } from './api';

export function App() {
  const repos = useQuery({ queryKey: ['repos'], queryFn: listRepos });
  if (repos.error) return <pre className="whitespace-pre-wrap p-4 text-red-600">{String(repos.error)}</pre>;
  return <pre className="p-4">{JSON.stringify(repos.data, null, 2)}</pre>;
}
```

- [ ] **Step 3: Tauri crate 작성**

`Cargo.toml`의 members를 바꾼다:
```toml
members = ["crates/core", "crates/app"]
```

`crates/app/Cargo.toml`:
```toml
[package]
name = "wtboard-app"
version.workspace = true
edition.workspace = true

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
serde = { version = "1", features = ["derive"] }
tauri = { version = "2", features = [] }
wtboard-core = { path = "../core" }
```

`crates/app/build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

`crates/app/tauri.conf.json`:
```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "wtboard",
  "version": "0.1.0",
  "identifier": "dev.wtboard.app",
  "build": {
    "beforeDevCommand": "pnpm --dir ui dev",
    "devUrl": "http://localhost:5173",
    "beforeBuildCommand": "pnpm --dir ui build",
    "frontendDist": "../../ui/dist"
  },
  "app": {
    "windows": [{ "label": "main", "title": "wtboard", "width": 1400, "height": 900 }],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.png"]
  }
}
```

`crates/app/capabilities/default.json`:
```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "windows": ["main"],
  "permissions": ["core:default"]
}
```

`crates/app/app-icon.svg`:
```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 1024">
  <rect width="1024" height="1024" rx="200" fill="#1f2937"/>
  <rect x="200" y="260" width="280" height="504" rx="40" fill="#ef4444"/>
  <rect x="544" y="260" width="280" height="504" rx="40" fill="#22c55e"/>
</svg>
```

```bash
pnpm tauri icon crates/app/app-icon.svg -o crates/app/icons
```
SVG 입력을 거부하면: `qlmanage -t -s 1024 -o crates/app crates/app/app-icon.svg`로 PNG를 만든 뒤 그 PNG로 다시 실행한다.

`crates/app/src/main.rs`:
```rust
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{Emitter, Manager, State};
use wtboard_core::domain::{ChangedFile, FilePatch};
use wtboard_core::store::config::Config;
use wtboard_core::watch::{spawn_watcher, EmitFn, WatchHandle};
use wtboard_core::worktree::{Worktree, WorktreeChanged, WorktreeSummary};

/// 1단계: config.toml의 첫 번째 레포 하나만 연다. 실패하면 이유를 화면에 보여 준다.
struct AppState {
    worktree: Result<Arc<Worktree>, String>,
    _watch: Mutex<Option<WatchHandle>>,
}

impl AppState {
    fn worktree(&self, id: &str) -> Result<&Arc<Worktree>, String> {
        let wt = self.worktree.as_ref().map_err(Clone::clone)?;
        if wt.id() == id {
            Ok(wt)
        } else {
            Err(format!("알 수 없는 worktree: {id}"))
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepoSummary {
    name: String,
    worktrees: Vec<WorktreeSummary>,
}

#[tauri::command]
async fn list_repos(state: State<'_, AppState>) -> Result<Vec<RepoSummary>, String> {
    let wt = state.worktree.as_ref().map_err(Clone::clone)?;
    let summary = wt.summary();
    let name = Path::new(&summary.path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(vec![RepoSummary { name, worktrees: vec![summary] }])
}

#[tauri::command]
async fn list_files(state: State<'_, AppState>, worktree_id: String, view: String) -> Result<Vec<ChangedFile>, String> {
    ensure_branch_view(&view)?;
    Ok(state.worktree(&worktree_id)?.files())
}

#[tauri::command]
async fn get_patch(
    state: State<'_, AppState>,
    worktree_id: String,
    view: String,
    path: String,
) -> Result<FilePatch, String> {
    ensure_branch_view(&view)?;
    state.worktree(&worktree_id)?.patch(&path).map_err(|e| e.to_string())
}

fn ensure_branch_view(view: &str) -> Result<(), String> {
    if view == "branch" {
        Ok(())
    } else {
        Err(format!("아직 지원하지 않는 view: {view}"))
    }
}

fn open_first_repo() -> Result<(Arc<Worktree>, Config), String> {
    let path = Config::default_path();
    let hint = format!("{} 에 다음을 추가하세요:\n\n[[repo]]\npath = \"~/dev/my-repo\"", path.display());
    let config = Config::load(&path).map_err(|e| format!("{e}\n\n{hint}"))?;
    let repo = config.repos.first().ok_or_else(|| format!("등록된 레포가 없습니다.\n\n{hint}"))?;
    let wt = Worktree::open(&repo.resolved_path(), repo.base.clone()).map_err(|e| e.to_string())?;
    Ok((wt, config))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let (worktree, watch) = match open_first_repo() {
                Ok((wt, config)) => {
                    let emit: EmitFn = Arc::new(move |ev: WorktreeChanged| {
                        let _ = handle.emit("worktree-changed", ev);
                    });
                    let poll = Duration::from_secs(config.watch.fallback_poll_secs);
                    match spawn_watcher(wt.clone(), poll, emit) {
                        Ok(w) => (Ok(wt), Some(w)),
                        Err(e) => (Err(format!("파일 감시를 시작할 수 없음: {e}")), None),
                    }
                }
                Err(e) => (Err(e), None),
            };
            app.manage(AppState { worktree, _watch: Mutex::new(watch) });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_repos, list_files, get_patch])
        .run(tauri::generate_context!())
        .expect("wtboard 실행 실패");
}
```

- [ ] **Step 4: 빌드 확인**

Run: `cargo build -p wtboard-app`
Expected: 경고 외 오류 없이 빌드된다.

- [ ] **Step 5: 실행 확인 — 설정 없음**

```bash
mv ~/.wtboard/config.toml ~/.wtboard/config.toml.bak 2>/dev/null; pnpm tauri dev
```
Expected: 창이 뜨고 "설정 파일을 읽을 수 없음 ... [[repo]] path = ..." 안내가 빨간 글씨로 보인다. 창을 닫는다.

CLI가 `crates/app`을 찾지 못하면 `TAURI_APP_PATH=crates/app pnpm tauri dev`로 실행하고, 되면 `package.json`의 `tauri` script를 `TAURI_APP_PATH=crates/app tauri`로 바꾼다.

- [ ] **Step 6: 실행 확인 — 레포 연결**

```bash
mkdir -p ~/.wtboard
[ -f ~/.wtboard/config.toml.bak ] && mv ~/.wtboard/config.toml.bak ~/.wtboard/config.toml
[ -f ~/.wtboard/config.toml ] || printf '[[repo]]\npath = "%s"\nbase = "main"\n' "$PWD" > ~/.wtboard/config.toml
pnpm tauri dev
```
Expected: 창에 `[{ "name": "wtboard", "worktrees": [{ "branch": "main", "base": "main", "changedFiles": 0, ... }] }]` 형태의 JSON이 보인다.

- [ ] **Step 7: Commit**

```bash
git add package.json pnpm-workspace.yaml pnpm-lock.yaml Cargo.toml Cargo.lock crates/app ui
git commit -m "feat(app): Tauri 껍데기와 command/event 연결"
```

---

### Task 10: PR 스타일 diff 화면 + 실시간 갱신

**Files:**
- Create: `ui/src/store.ts`, `ui/src/useWorktreeEvents.ts`, `ui/src/lib/anchor.ts`, `ui/src/lib/events.ts`, `ui/src/lib/events.test.ts`, `ui/src/lib/useInView.ts`, `ui/src/components/FileList.tsx`, `ui/src/components/FileDiff.tsx`
- Modify: `ui/src/App.tsx` (전체 교체), `ui/src/main.tsx`

**Interfaces:**
- Consumes: `api.ts`의 타입과 함수 (Task 9)
- Produces:
  - query key 규칙: `['repos']`, `['files', worktreeId, view]`, `['patch', worktreeId, path, view]`
  - `keysToInvalidate(ev: WorktreeChanged): QueryKey[]`
  - `useUiStore` — `{ viewType: 'split' | 'unified', setViewType }`
  - `FileDiff` props `{ worktreeId, file, viewType }` — Task 11이 하이라이팅을 넣는다

- [ ] **Step 1: 실패하는 테스트 작성**

`ui/src/lib/events.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { keysToInvalidate } from './events';

describe('keysToInvalidate', () => {
  it('file scope는 바뀐 파일의 patch만 무효화한다', () => {
    expect(keysToInvalidate({ worktreeId: 'w', scope: 'file', changedPaths: ['a.py', 'b.py'] })).toEqual([
      ['repos'],
      ['files', 'w'],
      ['patch', 'w', 'a.py'],
      ['patch', 'w', 'b.py'],
    ]);
  });

  it('base scope는 그 worktree의 patch 전체를 무효화한다', () => {
    expect(keysToInvalidate({ worktreeId: 'w', scope: 'base', changedPaths: [] })).toEqual([
      ['repos'],
      ['files', 'w'],
      ['patch', 'w'],
    ]);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `pnpm --dir ui test`
Expected: FAIL — `Failed to resolve import "./events"`.

- [ ] **Step 3: event → query invalidate 구현**

`ui/src/lib/events.ts`:
```ts
import type { QueryKey } from '@tanstack/react-query';
import type { WorktreeChanged } from '../api';

/** event 하나로 무효화할 query key prefix 목록 */
export function keysToInvalidate(ev: WorktreeChanged): QueryKey[] {
  const keys: QueryKey[] = [['repos'], ['files', ev.worktreeId]];
  if (ev.scope === 'base') {
    keys.push(['patch', ev.worktreeId]);
  } else {
    for (const path of ev.changedPaths) keys.push(['patch', ev.worktreeId, path]);
  }
  return keys;
}
```

`ui/src/useWorktreeEvents.ts`:
```ts
import { useEffect } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useQueryClient } from '@tanstack/react-query';
import type { WorktreeChanged } from './api';
import { keysToInvalidate } from './lib/events';

/** worktree-changed event가 오면 관련 query만 무효화한다. 화면에 보이는 query만 다시 불러온다. */
export function useWorktreeEvents() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlisten = listen<WorktreeChanged>('worktree-changed', ({ payload }) => {
      for (const queryKey of keysToInvalidate(payload)) queryClient.invalidateQueries({ queryKey });
    });
    return () => {
      unlisten.then((off) => off());
    };
  }, [queryClient]);
}
```

- [ ] **Step 4: 테스트 통과 확인**

Run: `pnpm --dir ui test`
Expected: PASS (2 tests).

- [ ] **Step 5: 화면 구성 요소 작성**

`ui/src/store.ts`:
```ts
import { create } from 'zustand';

export type ViewType = 'split' | 'unified';

interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
}

export const useUiStore = create<UiState>((set) => ({
  viewType: 'split',
  setViewType: (viewType) => set({ viewType }),
}));
```

`ui/src/lib/anchor.ts`:
```ts
export const fileAnchorId = (path: string) => `file-${encodeURIComponent(path)}`;
```

`ui/src/lib/useInView.ts`:
```ts
import { useEffect, useRef, useState } from 'react';

/** 요소가 화면(위아래 rootMargin 포함)에 들어와 있는지. patch lazy loading에 쓴다. */
export function useInView<T extends Element>(rootMargin = '400px') {
  const ref = useRef<T>(null);
  const [inView, setInView] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new IntersectionObserver(([entry]) => setInView(entry.isIntersecting), { rootMargin });
    observer.observe(el);
    return () => observer.disconnect();
  }, [rootMargin]);
  return [ref, inView] as const;
}
```

`ui/src/components/FileList.tsx`:
```tsx
import type { ChangedFile } from '../api';
import { fileAnchorId } from '../lib/anchor';

const STATUS_LABEL: Record<ChangedFile['status'], string> = {
  added: 'A',
  modified: 'M',
  deleted: 'D',
  untracked: 'U',
};

export function FileList({ files }: { files: ChangedFile[] }) {
  return (
    <ul className="text-sm">
      {files.map((f) => (
        <li key={f.path}>
          <button
            className="flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-gray-100"
            title={f.path}
            onClick={() => document.getElementById(fileAnchorId(f.path))?.scrollIntoView({ block: 'start' })}
          >
            <span className="w-4 font-mono text-gray-500">{STATUS_LABEL[f.status]}</span>
            <span className="flex-1 truncate">{f.path}</span>
            <LineCounts file={f} />
          </button>
        </li>
      ))}
    </ul>
  );
}

export function LineCounts({ file }: { file: ChangedFile }) {
  if (file.additions === null) return <span className="text-gray-400">bin</span>;
  return (
    <span className="tabular-nums">
      <span className="text-green-600">+{file.additions}</span>{' '}
      <span className="text-red-600">-{file.deletions}</span>
    </span>
  );
}
```

`ui/src/components/FileDiff.tsx`:
```tsx
import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Diff, Hunk, parseDiff } from 'react-diff-view';
import { getPatch, type ChangedFile, type PatchBody } from '../api';
import { fileAnchorId } from '../lib/anchor';
import { useInView } from '../lib/useInView';
import type { ViewType } from '../store';
import { LineCounts } from './FileList';

interface Props {
  worktreeId: string;
  file: ChangedFile;
  viewType: ViewType;
}

export function FileDiff({ worktreeId, file, viewType }: Props) {
  const [ref, inView] = useInView<HTMLElement>();
  const patch = useQuery({
    queryKey: ['patch', worktreeId, file.path, 'branch'],
    queryFn: () => getPatch(worktreeId, 'branch', file.path),
    enabled: inView, // 화면에 보일 때만 계산
  });

  return (
    <section ref={ref} id={fileAnchorId(file.path)} className="mb-4 rounded border border-gray-200">
      <header className="sticky top-0 z-10 flex items-center gap-3 border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm">
        <span className="flex-1 truncate font-mono">{file.path}</span>
        <LineCounts file={file} />
      </header>
      {patch.error ? (
        <p className="p-3 text-sm text-red-600">{String(patch.error)}</p>
      ) : patch.data ? (
        <PatchView body={patch.data.body} viewType={viewType} />
      ) : (
        <div className="h-20" />
      )}
    </section>
  );
}

function PatchView({ body, viewType }: { body: PatchBody; viewType: ViewType }) {
  if (body.kind === 'text') return <TextDiff diff={body.diff} viewType={viewType} />;
  const label = body.kind === 'binary' ? '바이너리 파일' : '1MB 초과 파일';
  return (
    <p className="p-3 text-sm text-gray-500">
      {label} 변경됨 ({formatSize(body.oldSize)} → {formatSize(body.newSize)})
    </p>
  );
}

function TextDiff({ diff, viewType }: { diff: string; viewType: ViewType }) {
  const file = useMemo(() => parseDiff(diff)[0], [diff]);
  if (!file || file.hunks.length === 0) {
    return <p className="p-3 text-sm text-gray-500">내용 변경 없음 (빈 파일 또는 권한 변경)</p>;
  }
  return (
    <Diff viewType={viewType} diffType={file.type} hunks={file.hunks}>
      {(hunks) => hunks.map((hunk) => <Hunk key={hunk.content} hunk={hunk} />)}
    </Diff>
  );
}

function formatSize(n: number | null) {
  if (n === null) return '없음';
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
```

`ui/src/App.tsx` (전체 교체):
```tsx
import { useQuery } from '@tanstack/react-query';
import { listFiles, listRepos, type WorktreeSummary } from './api';
import { FileDiff } from './components/FileDiff';
import { FileList } from './components/FileList';
import { useUiStore } from './store';
import { useWorktreeEvents } from './useWorktreeEvents';

export function App() {
  useWorktreeEvents();
  const repos = useQuery({ queryKey: ['repos'], queryFn: listRepos });
  if (repos.error) return <ErrorScreen message={String(repos.error)} />;
  const repo = repos.data?.[0];
  const worktree = repo?.worktrees[0];
  if (!repo || !worktree) return <p className="p-4 text-gray-500">불러오는 중…</p>;
  return <WorktreeView repoName={repo.name} worktree={worktree} />;
}

function WorktreeView({ repoName, worktree }: { repoName: string; worktree: WorktreeSummary }) {
  const { viewType, setViewType } = useUiStore();
  const files = useQuery({
    queryKey: ['files', worktree.id, 'branch'],
    queryFn: () => listFiles(worktree.id, 'branch'),
  });

  return (
    <div className="flex h-screen flex-col">
      <header className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <strong>{repoName}</strong>
        <span className="font-mono">{worktree.branch ?? '(detached)'}</span>
        <span className="text-gray-500">← {worktree.base}</span>
        <span className="flex-1" />
        <div className="flex overflow-hidden rounded border border-gray-300">
          {(['split', 'unified'] as const).map((v) => (
            <button
              key={v}
              onClick={() => setViewType(v)}
              className={`px-2 py-1 ${viewType === v ? 'bg-gray-800 text-white' : ''}`}
            >
              {v}
            </button>
          ))}
        </div>
      </header>
      <div className="flex min-h-0 flex-1">
        <aside className="w-80 shrink-0 overflow-y-auto border-r border-gray-200">
          <p className="px-3 py-2 text-xs text-gray-500">변경 파일 {files.data?.length ?? 0}개</p>
          <FileList files={files.data ?? []} />
        </aside>
        <main className="flex-1 overflow-y-auto p-4">
          {files.error && <p className="text-red-600">{String(files.error)}</p>}
          {files.data?.length === 0 && <p className="text-gray-500">base 대비 변경 사항이 없습니다.</p>}
          {files.data?.map((f) => (
            <FileDiff key={f.path} worktreeId={worktree.id} file={f} viewType={viewType} />
          ))}
        </main>
      </div>
    </div>
  );
}

function ErrorScreen({ message }: { message: string }) {
  return (
    <div className="p-6">
      <h1 className="mb-2 font-bold">wtboard를 시작할 수 없습니다</h1>
      <pre className="whitespace-pre-wrap text-sm text-red-600">{message}</pre>
    </div>
  );
}
```

`ui/src/main.tsx`의 `import './index.css';` 바로 위에 추가:
```tsx
import 'react-diff-view/style/index.css';
```

- [ ] **Step 6: 타입 체크 + 테스트**

Run: `pnpm --dir ui build && pnpm --dir ui test`
Expected: 오류 없이 빌드, 테스트 PASS.

- [ ] **Step 7: 수동 확인**

```bash
git checkout -b scratch-check
echo "임시" >> docs/draft.md
pnpm tauri dev
```
Expected: 왼쪽에 `M docs/draft.md +1 -0`, 오른쪽에 split diff. 앱을 띄운 채로 다른 터미널에서 `echo "또" >> docs/draft.md` → 1초 안에 diff에 줄이 추가된다. `unified` 버튼이 동작한다. 확인 후:
```bash
git checkout -- docs/draft.md && git checkout main && git branch -D scratch-check
```
(이 확인은 `base = "main"`인 wtboard 레포 자신을 대상으로 한다.)

- [ ] **Step 8: Commit**

```bash
git add ui
git commit -m "feat(ui): PR 스타일 diff 화면과 실시간 갱신"
```

---

### Task 11: 문법 하이라이팅 + 1단계 완료 검증

**Files:**
- Create: `ui/src/lib/language.ts`, `ui/src/lib/language.test.ts`, `ui/src/lib/highlight.ts`, `ui/src/lib/highlight.test.ts`
- Modify: `ui/src/components/FileDiff.tsx`, `ui/src/index.css`, `docs/draft.md` (3절 diff 렌더링 행)

**Interfaces:**
- Consumes: `FileDiff` 내부 `TextDiff` (Task 10)
- Produces:
  - `languageForPath(path: string): string | null` — refractor 언어 이름
  - `tokenizeHunks(hunks: HunkData[], language: string | null)` — 실패하거나 모르는 언어면 `undefined` (하이라이팅 없이 렌더)

- [ ] **Step 1: 실패하는 테스트 작성**

`ui/src/lib/language.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { languageForPath } from './language';

describe('languageForPath', () => {
  it.each([
    ['app/api/users.py', 'python'],
    ['src/App.tsx', 'tsx'],
    ['lib/user.g.dart', 'dart'],
    ['pnpm-lock.yaml', 'yaml'],
    ['README.MD', 'markdown'],
    ['Dockerfile', null],
    ['.env', null],
  ])('%s → %s', (path, lang) => {
    expect(languageForPath(path)).toBe(lang);
  });
});
```

`ui/src/lib/highlight.test.ts`:
```ts
import { describe, expect, it } from 'vitest';
import { parseDiff } from 'react-diff-view';
import { tokenizeHunks } from './highlight';

const DIFF = [
  'diff --git a/a.py b/a.py',
  'index 1111111..2222222 100644',
  '--- a/a.py',
  '+++ b/a.py',
  '@@ -1 +1 @@',
  '-x = 1',
  '+x = 2',
  '',
].join('\n');

describe('tokenizeHunks', () => {
  const [file] = parseDiff(DIFF);

  it('python 변경을 토큰화한다', () => {
    expect(tokenizeHunks(file.hunks, 'python')).toBeDefined();
  });

  it('모르는 언어는 하이라이팅을 건너뛴다', () => {
    expect(tokenizeHunks(file.hunks, null)).toBeUndefined();
    expect(tokenizeHunks(file.hunks, 'no-such-language')).toBeUndefined();
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `pnpm --dir ui test`
Expected: FAIL — `Failed to resolve import "./language"`, `"./highlight"`.

- [ ] **Step 3: 구현**

`ui/src/lib/language.ts`:
```ts
const BY_EXTENSION: Record<string, string> = {
  py: 'python',
  ts: 'typescript',
  tsx: 'tsx',
  js: 'javascript',
  jsx: 'jsx',
  mjs: 'javascript',
  dart: 'dart',
  json: 'json',
  toml: 'toml',
  yaml: 'yaml',
  yml: 'yaml',
  md: 'markdown',
  css: 'css',
  html: 'markup',
  rs: 'rust',
  sh: 'bash',
  sql: 'sql',
};

/** 파일 경로 → refractor 언어 이름. 모르면 null */
export function languageForPath(path: string): string | null {
  const name = path.split('/').pop() ?? '';
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return null;
  return BY_EXTENSION[name.slice(dot + 1).toLowerCase()] ?? null;
}
```

`ui/src/lib/highlight.ts`:
```ts
import refractor from 'refractor';
import { tokenize, type HunkData } from 'react-diff-view';

/** 하이라이팅 토큰. 모르는 언어거나 실패하면 undefined (하이라이팅 없이 렌더) */
export function tokenizeHunks(hunks: HunkData[], language: string | null) {
  if (!language || !refractor.registered(language)) return undefined;
  try {
    return tokenize(hunks, { highlight: true, refractor, language });
  } catch (e) {
    console.warn(`[wtboard] ${language} 하이라이팅 실패`, e);
    return undefined;
  }
}
```
`tsc`가 `refractor` 타입을 못 찾으면 `ui/src/refractor.d.ts`에 `declare module 'refractor';` 한 줄을 추가한다.

`ui/src/components/FileDiff.tsx`에서 import에 추가:
```tsx
import { tokenizeHunks } from '../lib/highlight';
import { languageForPath } from '../lib/language';
```

`PatchView`가 `path`를 받아 `TextDiff`로 넘기도록 바꾼다. `FileDiff` 안의 호출부:
```tsx
        <PatchView body={patch.data.body} viewType={viewType} path={file.path} />
```

`PatchView`와 `TextDiff`를 다음으로 교체:
```tsx
function PatchView({ body, viewType, path }: { body: PatchBody; viewType: ViewType; path: string }) {
  if (body.kind === 'text') return <TextDiff diff={body.diff} viewType={viewType} path={path} />;
  const label = body.kind === 'binary' ? '바이너리 파일' : '1MB 초과 파일';
  return (
    <p className="p-3 text-sm text-gray-500">
      {label} 변경됨 ({formatSize(body.oldSize)} → {formatSize(body.newSize)})
    </p>
  );
}

function TextDiff({ diff, viewType, path }: { diff: string; viewType: ViewType; path: string }) {
  const file = useMemo(() => parseDiff(diff)[0], [diff]);
  const tokens = useMemo(() => (file ? tokenizeHunks(file.hunks, languageForPath(path)) : undefined), [file, path]);
  if (!file || file.hunks.length === 0) {
    return <p className="p-3 text-sm text-gray-500">내용 변경 없음 (빈 파일 또는 권한 변경)</p>;
  }
  return (
    <Diff viewType={viewType} diffType={file.type} hunks={file.hunks} tokens={tokens}>
      {(hunks) => hunks.map((hunk) => <Hunk key={hunk.content} hunk={hunk} />)}
    </Diff>
  );
}
```

`ui/src/index.css` 끝에 추가:
```css
.token.comment, .token.prolog { color: #6a737d; }
.token.keyword, .token.important, .token.atrule { color: #d73a49; }
.token.string, .token.char, .token.attr-value { color: #032f62; }
.token.function, .token.class-name { color: #6f42c1; }
.token.number, .token.boolean, .token.constant { color: #005cc5; }
.token.tag, .token.selector { color: #22863a; }
```

- [ ] **Step 4: 통과 확인**

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: 테스트 PASS, 빌드 성공.

- [ ] **Step 5: 1단계 완료 기준 검증 (사람이 직접)**

1. `~/.wtboard/config.toml`의 `path`를 실제로 Claude Code를 돌릴 worktree로 바꾼다 (FastAPI / Next / Flutter 중 하나).
2. `pnpm tauri dev`로 앱을 띄운다.
3. 그 worktree에서 `claude`를 실행하고 파일 하나를 고치게 한다.
4. 확인할 것:
   - [ ] Edit 도구가 끝난 뒤 **1초 안에** 화면 diff가 바뀐다
   - [ ] 이미 M인 파일을 다시 고쳐도 바뀐다
   - [ ] 새 파일, 파일 삭제, 폴더 삭제가 목록에 반영된다
   - [ ] Claude가 커밋해도 diff가 그대로다 (merge-base 기준이므로)
   - [ ] `git fetch` 후 base가 앞서 나가도 앱이 멈추지 않는다
5. 렌더 품질 확인: 2,000줄 이상 바뀐 파일, 한글이 들어간 파일, split/unified 전환, 스크롤 성능.

- [ ] **Step 6: diff 라이브러리 결정 기록**

`docs/draft.md` 3절 표의 diff 렌더링 행 "이유" 칸을 Step 5 결과로 바꾼다. 예:
```
| diff 렌더링 | `react-diff-view` (+ `refractor` 하이라이팅) | split/unified 지원. 1단계에서 확인 (2,000줄 파일 스크롤 문제없음) → 확정 |
```
문제가 있었다면 무엇이 문제였는지 적고, 2단계 plan에서 대안을 다룬다.

- [ ] **Step 7: Commit**

```bash
git add ui docs/draft.md
git commit -m "feat(ui): 문법 하이라이팅, 1단계 완료"
```

---

## 이후 단계 로드맵 (단계마다 별도 plan)

| 단계 | 핵심 작업 | 1단계 결과에 따라 달라지는 것 |
|---|---|---|
| 2 | `git worktree list --porcelain`으로 worktree 발견, common dir `worktrees/` 감시, 여러 레포, 사이드바, 파일 트리, 프로필 3종(제외 폴더·접기), `linguist-generated`(`git check-attr`), base 결정에 `gh pr view` 추가, 큰 레포용 `core.fsmonitor` 안내 | watcher를 worktree마다 하나씩 둘지 하나로 합칠지 |
| 3 | `state.sqlite`(`rusqlite`), 봤음 체크(`set_viewed`), "다시 바뀜" 표시 | 봤음 체크한 파일은 hash가 항상 있어야 하므로 Task 7의 "본 파일만 hash" 규칙을 "봤음 체크한 파일 + 본 파일"로 넓힌다 |
| 4 | `crates/hook` CLI, `plugin/` (hooks.json, run.sh), 세션 기록, 이번 세션 / 마지막 응답 탭, Bash 변경 추적 | `list_files`/`get_patch`의 `view` 인자를 `session`/`turn`으로 확장 |
| 5 | 라인 코멘트, UserPromptSubmit 자동 첨부, 마크다운 복사, 메뉴바 아이콘, 항상 위에 띄우기 | — |
