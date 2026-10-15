# wtboard 2단계: 여러 레포·worktree 자동 발견 + sidebar + profile Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** config에 등록한 모든 레포의 모든 worktree를 sidebar에 보여 주고 실시간으로 감시하며, profile(Python / Node / Flutter)과 `linguist-generated`로 생성 파일을 접고, PR base와 fsmonitor 안내까지 제공한다.

**Architecture:** core에 `Registry`(config 전체) → `Repo`(common dir 하나) → `Worktree`(1단계 그대로 + profile·접기·PR base) 계층을 만든다. `Repo`는 `<common dir>/worktrees/`를 감시해 worktree 추가·삭제를 반영한다. app은 `Registry`를 background에서 열고 Tauri command/event로 노출하며, UI는 sidebar에서 worktree를 고른다.

**Tech Stack:** Rust (edition 2021), Tauri 2, `notify` 8 + `notify-debouncer-full` 0.5, `blake3`, `toml` + `serde`; React 19 + TypeScript + Vite + Tailwind CSS 4, TanStack Query 5, Zustand 5 (`persist`), `react-diff-view` 3, Vitest. 외부 명령: `git` ≥ 2.36 (`worktree list -z`), `gh` (있으면).

**Spec:** `docs/superpowers/specs/2026-10-15-stage2-multi-repo-design.md` (상위 설계 `docs/draft.md`)

## Global Constraints

- 앱은 레포에 **어떤 것도 쓰지 않는다**. 모든 git 호출은 `Git` 래퍼를 거친다. `gh`도 `GIT_OPTIONAL_LOCKS=0`으로 실행한다.
- 변경 판단은 `git status`가 아니라 파일별 diff hash. 파일 diff는 화면에 보일 때만 계산한다. 접힌 파일은 펼칠 때만 계산한다.
- profile 판별 순서: `pubspec.yaml` → Flutter, `package.json` → Node, `pyproject.toml` 또는 `requirements.txt` → Python, 그 외 기본.
- profile 표 (spec 1절)의 제외 폴더·접기 pattern을 글자 그대로 쓴다. 모든 profile에 1단계 `COMMON_EXCLUDED_DIRS`를 더한다.
- base 결정: config `base` → gh PR base(`origin/<이름>`이 로컬에 있을 때만) → `origin/HEAD`. config에 `base`가 있으면 gh를 부르지 않는다.
- gh: PATH → `/opt/homebrew/bin` → `/usr/local/bin` 순서로 찾는다. `GH_PROMPT_DISABLED=1`, timeout 5초. 모든 실패는 조용히 `None`.
- 느린 목록 기준 500ms (`WTBOARD_SLOW_LIST_MS`로 바꿀 수 있음, UI 확인용). `fallback_poll_secs` 최소 5초. config 경로는 `WTBOARD_CONFIG`로 바꿀 수 있다.
- 사용자에게 보이는 문구는 한국어. 기술 용어는 영어 그대로 (`worktree`, `base`, `profile`).
- 테스트를 먼저 쓴다 (실패 확인 → 구현 → 통과). 실제 git, 실제 FSEvents를 쓴다.
- **매 task 마무리 절차** (사용자 지시):
  1. commit 직후 `simplify` skill을 실행한다. 바뀐 것이 있으면 테스트를 다시 돌리고 `refactor: ...` commit으로 따로 남긴다.
  2. UI가 바뀌는 task(12~16)는 아래 "UI 확인 절차"로 실제 앱 화면을 찍어 보고 나서 완료로 친다.

### UI 확인 절차

`$SCRATCH`는 세션 scratchpad 디렉터리다 (없으면 `SCRATCH=$(mktemp -d)`).

```bash
# 1. fixture (Task 12에서 만든 script, 한 번만. 다시 만들면 초기화된다)
scripts/fixtures.sh "$SCRATCH/fixtures"
# 2. 앱 실행 (background로. 첫 빌드는 1~2분)
source ~/.cargo/env && WTBOARD_CONFIG="$SCRATCH/fixtures/config.toml" pnpm tauri dev
# 3. 창을 앞으로 가져와 화면 전체를 찍는다
osascript -e 'tell application "System Events" to set frontmost of (first process whose name is "wtboard-app") to true'
mkdir -p "$SCRATCH/shots" && screencapture -x "$SCRATCH/shots/task<N>-<설명>.png"
# 4. Read tool로 png를 열어 확인한다. 클릭이 필요하면 computer-use를 쓴다
# 5. 끝나면 종료
pkill -f 'target/debug/wtboard-app'; pkill -f 'tauri dev'; pkill -f 'ui dev'
```

## Review Focus

spec이 직접 말하지 않지만 사용자가 겪을 가능성이 큰 다섯 가지. 각각 담당 task에 테스트를 넣었다.

1. **Finder에서 띄운 앱**: PATH에 `/opt/homebrew/bin`이 없어도 gh를 찾아야 한다 → Task 4 `find_gh_in` 테스트.
2. **config entry가 가리키던 worktree가 사라짐** (지금 `omofictions-back-1371` 상황): 같은 레포의 나머지 worktree는 계속 보여야 한다. git을 common dir에서 실행하는 것으로 해결 → Task 11 테스트.
3. **경로에 공백·한글**: `worktree list` 파싱과 worktree id 매칭이 깨지지 않아야 한다 → Task 8 테스트.
4. **`.gitattributes` 수정**: 다른 파일의 접기 판정이 바로 바뀌어야 한다 (30초 poll까지 기다리지 않음) → Task 6 테스트.
5. **detached HEAD worktree**: sidebar에 빈 이름이 아니라 폴더 이름이 보여야 하고, gh를 부르지 않아야 한다 → Task 7, Task 13 테스트.

## 파일 구조

| 파일 | 상태 | 책임 |
|---|---|---|
| `crates/core/src/store/config.rs` | 수정 | `WTBOARD_CONFIG`, poll 최소값 |
| `crates/core/src/profile.rs` | 수정 | `Profile` 판별, 제외 pattern, 접기 pattern |
| `crates/core/src/git/mod.rs` | 수정 | `run_with_input` (stdin) |
| `crates/core/src/git/attrs.rs` | 신규 | `linguist_generated` |
| `crates/core/src/gh.rs` | 신규 | `Gh` (PR base 조회) |
| `crates/core/src/git/base.rs` | 수정 | `Base`, `BaseSource`, `remote_branch` |
| `crates/core/src/git/worktrees.rs` | 신규 | `list_worktrees`, `common_dir` |
| `crates/core/src/domain.rs` | 수정 | `ChangedFile.collapsed` |
| `crates/core/src/worktree.rs` | 수정 | profile·접기·meta·PR base |
| `crates/core/src/watch/mod.rs` | 수정 | 제외 pattern 매칭, `excluding`, `canonical`/`nfc` 공개 |
| `crates/core/src/watch/watcher.rs` | 수정 | `spawn_watcher_with` (on_gone), PR lookup |
| `crates/core/src/watch/repo_watcher.rs` | 신규 | `<common dir>/worktrees/` 감시 |
| `crates/core/src/repo.rs` | 신규 | `Repo`, `Sink`, `RepoSummary` |
| `crates/core/src/registry.rs` | 신규 | `Registry`, `EntryError` |
| `crates/app/src/main.rs` | 수정 | Registry 연결, `spawn_blocking`, `worktrees-updated` |
| `scripts/fixtures.sh` | 신규 | UI 확인용 fixture 레포 |
| `ui/src/api.ts` | 수정 | 새 타입 |
| `ui/src/store.ts` | 수정 | 선택(persist), banner 닫기 |
| `ui/src/lib/selection.ts`, `time.ts`, `banner.ts`, `useNow.ts` | 신규 | 순수 로직 |
| `ui/src/components/Sidebar.tsx`, `WorktreeView.tsx`, `FsmonitorBanner.tsx` | 신규 | 화면 |
| `ui/src/components/FileTree.tsx`, `FileDiff.tsx` | 수정 | 접힌 파일 |

---

### Task 1: config — `WTBOARD_CONFIG`와 poll 최소값

**Files:**
- Modify: `crates/core/src/store/config.rs`
- Test: `crates/core/tests/config.rs`

**Interfaces:**
- Produces: `pub fn config_path(env: Option<OsString>, home: &Path) -> PathBuf`, `Config::default_path()`가 `WTBOARD_CONFIG`를 따름, `pub const MIN_POLL_SECS: u64 = 5`, `WatchConfig::poll_interval(&self) -> Duration`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/config.rs` 맨 위 `use`를 바꾸고 테스트 두 개를 추가한다.

```rust
use std::ffi::OsString;
use std::path::Path;
use std::time::Duration;

use wtboard_core::store::config::{config_path, expand_tilde, Config};

#[test]
fn wtboard_config_env_overrides_default_path() {
    let home = Path::new("/Users/me");
    assert_eq!(config_path(Some(OsString::from("/tmp/x.toml")), home), Path::new("/tmp/x.toml"));
    assert_eq!(config_path(Some(OsString::new()), home), Path::new("/Users/me/.wtboard/config.toml"));
    assert_eq!(config_path(None, home), Path::new("/Users/me/.wtboard/config.toml"));
}

#[test]
fn poll_interval_has_a_floor_of_five_seconds() {
    let zero = Config::parse("[watch]\nfallback_poll_secs = 0\n").unwrap();
    assert_eq!(zero.watch.poll_interval(), Duration::from_secs(5));
    let minute = Config::parse("[watch]\nfallback_poll_secs = 60\n").unwrap();
    assert_eq!(minute.watch.poll_interval(), Duration::from_secs(60));
}
```

- [ ] **Step 2: 실패 확인**

Run: `source ~/.cargo/env && cargo test -p wtboard-core --test config`
Expected: 컴파일 실패 (`config_path`, `poll_interval` 없음)

- [ ] **Step 3: 구현**

`crates/core/src/store/config.rs`:

```rust
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;
```

`WatchConfig` 아래에 추가:

```rust
/// 0 같은 값이면 worker가 쉬지 않고 재계산한다
pub const MIN_POLL_SECS: u64 = 5;

impl WatchConfig {
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.fallback_poll_secs.max(MIN_POLL_SECS))
    }
}
```

`Config::default_path`를 바꾸고 `config_path`를 추가:

```rust
    pub fn default_path() -> PathBuf {
        config_path(std::env::var_os("WTBOARD_CONFIG"), &home_dir())
    }
```

```rust
/// `WTBOARD_CONFIG`가 있으면 그 경로, 없으면 `~/.wtboard/config.toml`
pub fn config_path(env: Option<OsString>, home: &Path) -> PathBuf {
    env.filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".wtboard/config.toml"))
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test config`
Expected: PASS (6개)

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/store/config.rs crates/core/tests/config.rs
git commit -m "feat(core): WTBOARD_CONFIG로 config 경로를 바꾸고 poll 최소 5초"
```

그다음 `simplify` skill 실행.

---

### Task 2: profile 판별과 제외·접기 pattern

**Files:**
- Modify: `crates/core/src/profile.rs`, `crates/core/src/watch/mod.rs`
- Create: `crates/core/tests/profile.rs`
- Test: `crates/core/tests/watch_layout.rs`

**Interfaces:**
- Produces:
  - `pub enum Profile { Flutter, Node, Python, Default }` (`Serialize`, lowercase)
  - `Profile::detect(root: &Path) -> Profile`
  - `Profile::excluded_dirs(self) -> Vec<&'static str>` (공통 + profile 고유)
  - `Profile::collapses(self, path: &str) -> bool`
  - `pub fn is_excluded(parts: &[&str], patterns: &[String]) -> bool`
  - `WatchLayout::excluding(self, dirs: &[&str]) -> Self`
  - `watch::canonical(p: impl AsRef<Path>) -> PathBuf`, `watch::nfc(&Path) -> PathBuf` 를 `pub(crate)`로

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/profile.rs` (신규):

```rust
use std::fs;

use wtboard_core::profile::{is_excluded, Profile};

fn root_with(files: &[&str]) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for f in files {
        fs::write(d.path().join(f), "").unwrap();
    }
    d
}

#[test]
fn detects_profile_from_marker_files() {
    assert_eq!(Profile::detect(root_with(&["pubspec.yaml", "package.json"]).path()), Profile::Flutter);
    assert_eq!(Profile::detect(root_with(&["package.json"]).path()), Profile::Node);
    assert_eq!(Profile::detect(root_with(&["requirements.txt"]).path()), Profile::Python);
    assert_eq!(Profile::detect(root_with(&["pyproject.toml"]).path()), Profile::Python);
    assert_eq!(Profile::detect(root_with(&["README.md"]).path()), Profile::Default);
}

#[test]
fn excluded_patterns_match_segment_runs_at_any_depth() {
    let pats: Vec<String> = Profile::Flutter.excluded_dirs().into_iter().map(String::from).collect();
    let ex = |p: &str| is_excluded(&p.split('/').collect::<Vec<_>>(), &pats);
    assert!(ex("ios/Pods/Alamofire/x.swift"));
    assert!(ex("packages/app/ios/Pods/x"));
    assert!(ex("build/app.apk"));
    assert!(ex("node_modules/x/index.js")); // 공통 제외
    assert!(!ex("lib/ios/podsfile.dart"));
    assert!(!ex("Pods/x")); // ios 없이 Pods만
    assert!(!ex("lib/builder.dart"));
}

#[test]
fn profile_specific_dirs_apply_only_to_that_profile() {
    let has = |p: Profile, d: &str| p.excluded_dirs().contains(&d);
    assert!(has(Profile::Python, "venv"));
    assert!(!has(Profile::Node, "venv"));
    assert!(has(Profile::Node, "out"));
    assert!(!has(Profile::Flutter, "out"));
    assert!(!has(Profile::Default, "build"));
    assert!(has(Profile::Default, "node_modules"));
}

#[test]
fn collapse_patterns_match_file_names() {
    assert!(Profile::Flutter.collapses("lib/models/user.g.dart"));
    assert!(Profile::Flutter.collapses("lib/user.freezed.dart"));
    assert!(Profile::Flutter.collapses("ios/Runner/GeneratedPluginRegistrant.m"));
    assert!(Profile::Flutter.collapses("pubspec.lock"));
    assert!(Profile::Flutter.collapses("ios/Podfile.lock"));
    assert!(!Profile::Flutter.collapses("lib/user.dart"));
    assert!(Profile::Node.collapses("package-lock.json"));
    assert!(Profile::Node.collapses("apps/web/pnpm-lock.yaml"));
    assert!(Profile::Node.collapses("yarn.lock"));
    assert!(Profile::Node.collapses("next-env.d.ts"));
    assert!(!Profile::Python.collapses("package-lock.json"));
    assert!(Profile::Python.collapses("uv.lock"));
    assert!(Profile::Python.collapses("poetry.lock"));
    assert!(!Profile::Default.collapses("uv.lock"));
}
```

`crates/core/tests/watch_layout.rs` 끝에 추가:

```rust
#[test]
fn multi_segment_excluded_pattern_is_ignored() {
    let repo = TestRepo::new();
    let l = WatchLayout::detect(&Git::new(repo.path()), &["ios/Pods"]).unwrap();
    assert_eq!(l.classify(&l.root.join("ios/Pods/A/x.h")), Target::Ignore);
    assert_eq!(l.classify(&l.root.join("ios/Runner/x.h")), file("ios/Runner/x.h"));
}

#[test]
fn excluding_replaces_the_excluded_dirs() {
    let repo = TestRepo::new();
    let l = layout(repo.path()).excluding(&["build"]);
    assert_eq!(l.classify(&l.root.join("build/x.txt")), Target::Ignore);
    assert_eq!(l.classify(&l.root.join("node_modules/x.js")), file("node_modules/x.js"));
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test profile --test watch_layout`
Expected: 컴파일 실패 (`Profile`, `is_excluded`, `excluding` 없음)

- [ ] **Step 3: 구현**

`crates/core/src/profile.rs` 전체:

```rust
use std::path::Path;

use serde::Serialize;

/// 모든 프로젝트에서 감시하지 않는 폴더. 이름이 같으면 깊이와 관계없이 제외한다.
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

/// worktree root의 marker 파일로 정하는 프로젝트 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Flutter,
    Node,
    Python,
    Default,
}

impl Profile {
    /// 위에서부터 처음 맞는 것. Flutter 레포에 `package.json`이 있어도 Flutter다
    pub fn detect(root: &Path) -> Self {
        let has = |name: &str| root.join(name).is_file();
        if has("pubspec.yaml") {
            Self::Flutter
        } else if has("package.json") {
            Self::Node
        } else if has("pyproject.toml") || has("requirements.txt") {
            Self::Python
        } else {
            Self::Default
        }
    }

    /// 감시에서 뺄 폴더 pattern. `/`가 있으면 연속된 segment로 맞춘다 (`ios/Pods`)
    pub fn excluded_dirs(self) -> Vec<&'static str> {
        let own: &[&str] = match self {
            Self::Flutter => &[".dart_tool", "build", "ios/Pods", ".fvm"],
            Self::Node => &["node_modules", ".next", "out", ".turbo"],
            Self::Python => &["venv", ".venv", "__pycache__", ".pytest_cache", ".ruff_cache", ".mypy_cache"],
            Self::Default => &[],
        };
        COMMON_EXCLUDED_DIRS.iter().chain(own).copied().collect()
    }

    /// 생성·lock 파일이라 접어서 보여 줄 파일인지. 파일 이름(마지막 segment)에 glob으로 맞춘다
    pub fn collapses(self, path: &str) -> bool {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.collapse_patterns().iter().any(|p| glob_name(p, name))
    }

    fn collapse_patterns(self) -> &'static [&'static str] {
        match self {
            Self::Flutter => &[
                "*.g.dart",
                "*.freezed.dart",
                "pubspec.lock",
                "Podfile.lock",
                "GeneratedPluginRegistrant.*",
            ],
            Self::Node => &["package-lock.json", "pnpm-lock.yaml", "yarn.lock", "next-env.d.ts"],
            Self::Python => &["uv.lock", "poetry.lock"],
            Self::Default => &[],
        }
    }
}

/// `*.g.dart`(끝), `GeneratedPluginRegistrant.*`(시작), 그 외는 이름 전체
fn glob_name(pattern: &str, name: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix('*') {
        name.ends_with(suffix)
    } else if let Some(prefix) = pattern.strip_suffix('*') {
        name.starts_with(prefix)
    } else {
        name == pattern
    }
}

/// 경로 segment 안에 제외 pattern의 segment가 연속으로 나오는지. 깊이는 상관없다
pub fn is_excluded(parts: &[&str], patterns: &[String]) -> bool {
    patterns.iter().any(|p| {
        let pat: Vec<&str> = p.split('/').collect();
        parts.windows(pat.len()).any(|w| w == pat.as_slice())
    })
}
```

`crates/core/src/watch/mod.rs`:

1. `use crate::profile::is_excluded;` 추가.
2. `WatchLayout` impl에 추가:

```rust
    /// 제외 폴더를 바꾼다. profile은 root를 알아야 정할 수 있어서 `detect` 뒤에 붙인다
    pub fn excluding(mut self, dirs: &[&str]) -> Self {
        self.excluded_dirs = dirs.iter().map(|s| s.to_string()).collect();
        self
    }
```

3. `classify`의 제외 판단을 바꾼다:

```rust
        // 기존
        let excluded = |p: &&str| *p == ".git" || self.excluded_dirs.iter().any(|e| e == p);
        if parts.is_empty() || parts.iter().any(excluded) {
            return Target::Ignore;
        }
        // 변경
        if parts.is_empty() || parts.contains(&".git") || is_excluded(&parts, &self.excluded_dirs) {
            return Target::Ignore;
        }
```

4. `canonical`, `nfc`를 다른 모듈에서 쓰도록 바꾼다:

```rust
pub(crate) fn canonical(p: impl AsRef<Path>) -> PathBuf {
    let p = p.as_ref();
    nfc(&p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
}

/// macOS FSEvents는 NFD 경로를 줄 수 있고 git(core.precomposeUnicode)은 NFC를 쓴다
pub(crate) fn nfc(path: &Path) -> PathBuf {
    PathBuf::from(path.to_string_lossy().nfc().collect::<String>())
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: 전체 PASS (기존 watcher/worktree 테스트 포함)

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/profile.rs crates/core/src/watch/mod.rs crates/core/tests/profile.rs crates/core/tests/watch_layout.rs
git commit -m "feat(core): Python/Node/Flutter profile 판별과 제외·접기 pattern"
```

그다음 `simplify` skill 실행.

---

### Task 3: `linguist-generated` 판정

**Files:**
- Modify: `crates/core/src/git/mod.rs`
- Create: `crates/core/src/git/attrs.rs`, `crates/core/tests/attrs.rs`

**Interfaces:**
- Produces: `Git::run_with_input(&self, args: &[&str], input: &[u8]) -> Result<Vec<u8>, GitError>`, `pub fn linguist_generated(git: &Git, paths: &[&str]) -> Result<HashSet<String>, GitError>`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/attrs.rs`:

```rust
mod common;

use common::TestRepo;
use wtboard_core::git::attrs::linguist_generated;
use wtboard_core::git::Git;

#[test]
fn reads_linguist_generated_like_github() {
    let repo = TestRepo::new();
    repo.write(
        ".gitattributes",
        "gen/** linguist-generated\ny.txt linguist-generated=true\nx.txt -linguist-generated\ngen/keep.ts -linguist-generated\n",
    );
    let paths = ["gen/a.ts", "gen/keep.ts", "x.txt", "y.txt", "z.txt", "공백 있는 파일.txt"];
    let mut got: Vec<String> = linguist_generated(&Git::new(repo.path()), &paths).unwrap().into_iter().collect();
    got.sort();
    assert_eq!(got, ["gen/a.ts", "y.txt"]);
}

#[test]
fn empty_path_list_needs_no_git_call() {
    let got = linguist_generated(&Git::new("/nonexistent"), &[]).unwrap();
    assert!(got.is_empty());
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test attrs`
Expected: 컴파일 실패 (`attrs` 모듈 없음)

- [ ] **Step 3: 구현**

`crates/core/src/git/mod.rs`:

1. 맨 위에 `pub mod attrs;` 추가, `use std::io::Write;`, `use std::process::{Command, Output, Stdio};`로 바꾼다.
2. `run_allow`의 상태 검사를 `finish`로 빼고 `run_with_input`을 추가한다:

```rust
    pub fn run_allow(&self, args: &[&str], ok_codes: &[i32]) -> Result<Vec<u8>, GitError> {
        let out = self.command(args).output()?;
        finish(args, out, ok_codes)
    }

    /// stdin으로 입력을 넘긴다 (`check-attr --stdin`)
    pub fn run_with_input(&self, args: &[&str], input: &[u8]) -> Result<Vec<u8>, GitError> {
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        // 입력을 다 쓰기 전에 stdout pipe가 차면 git이 멈춘다. 쓰기는 따로 한다
        let mut stdin = child.stdin.take().expect("stdin is piped");
        let input = input.to_vec();
        let writer = std::thread::spawn(move || stdin.write_all(&input));
        let out = child.wait_with_output()?;
        let _ = writer.join();
        finish(args, out, &[0])
    }
```

```rust
fn finish(args: &[&str], out: Output, ok_codes: &[i32]) -> Result<Vec<u8>, GitError> {
    match out.status.code() {
        Some(code) if ok_codes.contains(&code) => Ok(out.stdout),
        code => Err(GitError::Failed {
            args: args.join(" "),
            code,
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        }),
    }
}
```

`crates/core/src/git/attrs.rs`:

```rust
use std::collections::HashSet;

use super::{Git, GitError};

/// `.gitattributes`에서 `linguist-generated`가 켜진 경로. GitHub PR과 같은 기준으로 생성 파일을 접는다.
pub fn linguist_generated(git: &Git, paths: &[&str]) -> Result<HashSet<String>, GitError> {
    if paths.is_empty() {
        return Ok(HashSet::new());
    }
    let mut input = Vec::new();
    for p in paths {
        input.extend_from_slice(p.as_bytes());
        input.push(0);
    }
    let out = git.run_with_input(&["check-attr", "-z", "--stdin", "linguist-generated"], &input)?;
    // `<path>\0<attr>\0<value>\0` 반복. value는 set / unset / unspecified / 지정한 문자열
    let fields: Vec<&[u8]> = out.split(|b| *b == 0).collect();
    Ok(fields
        .chunks(3)
        .filter_map(|c| match c {
            [path, _, value] if matches!(*value, b"set" | b"true") => Some(String::from_utf8_lossy(path).into_owned()),
            _ => None,
        })
        .collect())
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: 전체 PASS

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/git/mod.rs crates/core/src/git/attrs.rs crates/core/tests/attrs.rs
git commit -m "feat(core): check-attr로 linguist-generated 파일을 찾는다"
```

그다음 `simplify` skill 실행.

---

### Task 4: `gh`로 PR base 조회

**Files:**
- Create: `crates/core/src/gh.rs`, `crates/core/tests/gh.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Produces:
  - `pub struct Gh` (`Debug`, `Clone`)
  - `Gh::detect() -> Gh`, `Gh::disabled() -> Gh`, `Gh::with_bin(Option<PathBuf>) -> Gh`, `Gh::timeout(self, Duration) -> Gh`, `Gh::enabled(&self) -> bool`
  - `Gh::pr_base(&self, cwd: &Path) -> Option<String>` (예: `"develop"`)
  - `pub fn find_gh_in(path_env: Option<OsString>, fallback_dirs: &[&Path]) -> Option<PathBuf>`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/gh.rs`:

```rust
use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wtboard_core::gh::{find_gh_in, Gh};

/// `dir/gh` 실행 파일을 만든다
fn stub(dir: &Path, body: &str) -> PathBuf {
    let p = dir.join("gh");
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[test]
fn returns_pr_base_branch() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), r#"[ "$1 $2 $3 $4" = "pr view --json baseRefName" ] && echo develop"#)));
    assert_eq!(gh.pr_base(d.path()).as_deref(), Some("develop"));
}

#[test]
fn runs_without_prompts_or_index_locks() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), r#"[ "$GH_PROMPT_DISABLED" = 1 ] && [ "$GIT_OPTIONAL_LOCKS" = 0 ] && echo main"#)));
    assert_eq!(gh.pr_base(d.path()).as_deref(), Some("main"));
}

#[test]
fn no_pr_is_none() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), "echo 'no pull requests found' >&2; exit 1")));
    assert_eq!(gh.pr_base(d.path()), None);
}

#[test]
fn missing_or_disabled_gh_is_none() {
    assert_eq!(Gh::with_bin(Some(PathBuf::from("/nonexistent/gh"))).pr_base(Path::new("/")), None);
    assert!(!Gh::disabled().enabled());
    assert_eq!(Gh::disabled().pr_base(Path::new("/")), None);
}

#[test]
fn slow_gh_is_killed_after_timeout() {
    let d = tempfile::tempdir().unwrap();
    let gh = Gh::with_bin(Some(stub(d.path(), "sleep 10; echo develop"))).timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(gh.pr_base(d.path()), None);
    assert!(started.elapsed() < Duration::from_secs(2), "took {:?}", started.elapsed());
}

/// Review Focus 1: Finder에서 띄운 앱은 PATH에 Homebrew 경로가 없다
#[test]
fn finds_gh_in_fallback_dirs_when_path_lacks_it() {
    let brew = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let gh = stub(brew.path(), "true");
    let path_env = Some(OsString::from(empty.path()));
    assert_eq!(find_gh_in(path_env.clone(), &[brew.path()]), Some(gh.clone()));
    assert_eq!(find_gh_in(path_env, &[]), None);
    // PATH에 있으면 PATH가 먼저
    let in_path = tempfile::tempdir().unwrap();
    let first = stub(in_path.path(), "true");
    assert_eq!(find_gh_in(Some(OsString::from(in_path.path())), &[brew.path()]), Some(first));
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test gh`
Expected: 컴파일 실패 (`gh` 모듈 없음)

- [ ] **Step 3: 구현**

`crates/core/src/lib.rs`에 `pub mod gh;` 추가.

`crates/core/src/gh.rs`:

```rust
//! `gh` CLI로 현재 브랜치 PR의 base를 묻는다. network 호출이라 느릴 수 있고, 어떤 실패도 `None`이다.
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TIMEOUT: Duration = Duration::from_secs(5);
/// Finder에서 띄운 앱은 PATH에 Homebrew 경로가 없다
const FALLBACK_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];

#[derive(Debug, Clone)]
pub struct Gh {
    bin: Option<PathBuf>,
    timeout: Duration,
}

impl Gh {
    pub fn detect() -> Self {
        let fallback: Vec<&Path> = FALLBACK_DIRS.iter().map(Path::new).collect();
        Self::with_bin(find_gh_in(std::env::var_os("PATH"), &fallback))
    }

    pub fn disabled() -> Self {
        Self::with_bin(None)
    }

    pub fn with_bin(bin: Option<PathBuf>) -> Self {
        Self { bin, timeout: TIMEOUT }
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn enabled(&self) -> bool {
        self.bin.is_some()
    }

    /// 현재 브랜치 PR의 base 브랜치 이름 (`develop`). gh 없음, 로그인 안 됨, PR 없음, timeout이면 `None`
    pub fn pr_base(&self, cwd: &Path) -> Option<String> {
        let mut child = Command::new(self.bin.as_ref()?)
            .args(["pr", "view", "--json", "baseRefName", "--jq", ".baseRefName"])
            .current_dir(cwd)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("NO_COLOR", "1")
            // gh가 내부에서 부르는 git도 index lock을 잡지 않게 한다
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        };
        if !status.success() {
            return None;
        }
        let mut out = String::new();
        child.stdout.take()?.read_to_string(&mut out).ok()?;
        let name = out.trim();
        (!name.is_empty()).then(|| name.to_owned())
    }
}

/// PATH의 디렉터리들, 그다음 `fallback_dirs`에서 `gh` 실행 파일을 찾는다
pub fn find_gh_in(path_env: Option<OsString>, fallback_dirs: &[&Path]) -> Option<PathBuf> {
    let path_dirs: Vec<PathBuf> = path_env.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    path_dirs
        .into_iter()
        .chain(fallback_dirs.iter().map(|d| d.to_path_buf()))
        .map(|d| d.join("gh"))
        .find(|p| p.is_file())
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test gh`
Expected: PASS (6개)

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/lib.rs crates/core/src/gh.rs crates/core/tests/gh.rs
git commit -m "feat(core): gh pr view로 PR base를 묻는다"
```

그다음 `simplify` skill 실행.

---

### Task 5: base 출처(`BaseSource`)와 `remote_branch`

**Files:**
- Modify: `crates/core/src/git/base.rs`, `crates/core/src/worktree.rs`
- Test: `crates/core/tests/base.rs`, `crates/core/tests/worktree.rs`

**Interfaces:**
- Consumes: 없음
- Produces:
  - `pub enum BaseSource { Config, Pr, Origin }` (`Serialize`, lowercase, `Copy`)
  - `pub struct Base { pub name: String, pub source: BaseSource }` (`Clone`, `PartialEq`)
  - `resolve_base(git: &Git, configured: Option<&str>, pr: Option<&str>) -> Result<Base, BaseError>` (인자 하나 추가)
  - `pub fn remote_branch(git: &Git, branch: &str) -> Option<String>`
  - `WorktreeSummary.base_source: BaseSource`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/base.rs`를 다음으로 바꾼다 (`merge_base_is_the_fork_point`는 그대로 둔다):

```rust
mod common;

use common::TestRepo;
use wtboard_core::git::base::{merge_base, remote_branch, resolve_base, BaseSource};
use wtboard_core::git::Git;

#[test]
fn configured_base_wins() {
    let repo = TestRepo::new();
    let b = resolve_base(&Git::new(repo.path()), Some("main"), Some("origin/develop")).unwrap();
    assert_eq!((b.name.as_str(), b.source), ("main", BaseSource::Config));
}

#[test]
fn pr_base_beats_origin_head() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let b = resolve_base(&Git::new(clone.path()), None, Some("origin/develop")).unwrap();
    assert_eq!((b.name.as_str(), b.source), ("origin/develop", BaseSource::Pr));
}

#[test]
fn falls_back_to_origin_head() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    let b = resolve_base(&Git::new(clone.path()), None, None).unwrap();
    assert_eq!((b.name.as_str(), b.source), ("origin/main", BaseSource::Origin));
}

#[test]
fn explains_how_to_fix_when_no_base_can_be_found() {
    let repo = TestRepo::new(); // origin 없음, config base 없음
    let err = resolve_base(&Git::new(repo.path()), None, None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("base") && msg.contains("config.toml"), "{msg}");
}

#[test]
fn remote_branch_requires_a_local_remote_ref() {
    let upstream = TestRepo::new();
    upstream.git(&["branch", "develop"]);
    let clone = TestRepo::clone_of(&upstream);
    let git = Git::new(clone.path());
    assert_eq!(remote_branch(&git, "develop").as_deref(), Some("origin/develop"));
    assert_eq!(remote_branch(&git, "release"), None);
}
```

`crates/core/tests/worktree.rs`의 `opens_with_current_changes` 끝에 한 줄 추가하고, 맨 위에 `use wtboard_core::git::base::BaseSource;`를 추가한다:

```rust
    assert_eq!(s.base_source, BaseSource::Config);
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test base --test worktree`
Expected: 컴파일 실패 (`BaseSource`, `remote_branch`, 인자 수)

- [ ] **Step 3: 구현**

`crates/core/src/git/base.rs`:

```rust
use serde::Serialize;

use super::{Git, GitError};

// BaseError는 그대로

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseSource {
    Config,
    Pr,
    Origin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Base {
    pub name: String,
    pub source: BaseSource,
}

/// base 결정 순서: config의 `base` → PR base(`remote_branch`로 확인한 것) → `origin/HEAD`
pub fn resolve_base(git: &Git, configured: Option<&str>, pr: Option<&str>) -> Result<Base, BaseError> {
    if let Some(name) = configured {
        return Ok(Base { name: name.to_owned(), source: BaseSource::Config });
    }
    if let Some(name) = pr {
        return Ok(Base { name: name.to_owned(), source: BaseSource::Pr });
    }
    let name = git
        .run_str(&["rev-parse", "--abbrev-ref", "origin/HEAD"])
        .map_err(|_| BaseError::NotFound)?;
    Ok(Base { name, source: BaseSource::Origin })
}

/// PR base 브랜치 이름(`develop`)을 로컬에 있는 remote 브랜치(`origin/develop`)로 바꾼다. 없으면 `None`
pub fn remote_branch(git: &Git, branch: &str) -> Option<String> {
    let name = format!("origin/{branch}");
    git.run(&["rev-parse", "--verify", "--quiet", &format!("refs/remotes/{name}")])
        .ok()
        .map(|_| name)
}

// merge_base는 그대로
```

`crates/core/src/worktree.rs`:

1. `use crate::git::base::{merge_base, resolve_base, Base, BaseError, BaseSource};`
2. `WorktreeSummary`에 `pub base_source: BaseSource,` 추가 (`base` 다음).
3. `State.base`의 타입을 `Base`로 바꾼다.
4. `open`:

```rust
        let base = resolve_base(&git, configured_base.as_deref(), None)?;
        let mb = merge_base(&git, &base.name)?;
```

5. `summary`: `base: st.base.name.clone(), base_source: st.base.source,`
6. `refresh_base`:

```rust
        let base = resolve_base(&self.git, self.configured_base.as_deref(), None)?;
        let mb = merge_base(&self.git, &base.name)?;
```

(`st.base == base` 비교는 `Base: PartialEq`라 그대로 컴파일된다.)

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core && cargo build -p wtboard-app`
Expected: 전체 PASS, app 빌드 성공

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/git/base.rs crates/core/src/worktree.rs crates/core/tests/base.rs crates/core/tests/worktree.rs
git commit -m "feat(core): base 출처를 기록하고 PR base 인자를 받는다"
```

그다음 `simplify` skill 실행.

---

### Task 6: Worktree에 profile·접기·meta 붙이기

**Files:**
- Modify: `crates/core/src/domain.rs`, `crates/core/src/git/changes.rs`, `crates/core/src/worktree.rs`
- Test: `crates/core/tests/worktree.rs`, `crates/core/tests/read_only.rs`

**Interfaces:**
- Consumes: `Profile` (Task 2), `linguist_generated` (Task 3), `Base`/`BaseSource` (Task 5)
- Produces:
  - `ChangedFile.collapsed: bool`
  - `WorktreeSummary` 새 필드: `profile: Profile`, `last_changed_at: Option<u64>` (Unix ms), `slow_list_ms: Option<u64>`, `fsmonitor: bool`, `error: Option<String>`
  - `WorktreeSummary::failed(path: &Path, branch: Option<String>, error: String) -> WorktreeSummary`
  - `Worktree::profile(&self) -> Profile`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/worktree.rs` 맨 위에 `use wtboard_core::profile::Profile;`를 추가하고 끝에 추가:

```rust
#[test]
fn generated_and_lock_files_are_marked_collapsed() {
    let repo = TestRepo::new();
    repo.write("pubspec.yaml", "name: app\n");
    repo.write(".gitattributes", "lib/api/** linguist-generated\n");
    repo.commit_all("flutter");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    repo.write("lib/user.dart", "class User {}\n");
    repo.write("lib/user.g.dart", "// gen\n");
    repo.write("lib/api/client.dart", "// gen\n");
    repo.write("pubspec.lock", "x\n");
    let wt = open(&repo);
    let got: Vec<(String, bool)> = wt.files().into_iter().map(|f| (f.path, f.collapsed)).collect();
    let want = [("lib/api/client.dart", true), ("lib/user.dart", false), ("lib/user.g.dart", true), ("pubspec.lock", true)];
    assert_eq!(got, want.map(|(p, c)| (p.to_string(), c)));
    assert_eq!(wt.summary().profile, Profile::Flutter);
}

#[test]
fn single_file_refresh_keeps_collapsed_flag() {
    let repo = TestRepo::new();
    repo.write("package.json", "{}\n");
    repo.commit_all("node");
    repo.git(&["checkout", "-q", "-b", "feat"]);
    let wt = open(&repo);
    repo.write("package-lock.json", "{}\n");
    wt.handle(&[file("package-lock.json")]).unwrap().expect("should emit");
    assert!(wt.files()[0].collapsed);
}

/// Review Focus 4: .gitattributes를 고치면 다른 파일의 접기 판정이 바로 바뀐다
#[test]
fn editing_gitattributes_recomputes_collapsed_flags() {
    let repo = feat_repo();
    repo.write("src/gen.py", "x = 1\n");
    let wt = open(&repo);
    assert!(!wt.files().iter().any(|f| f.collapsed));
    repo.write(".gitattributes", "src/gen.py linguist-generated\n");
    let ev = wt.handle(&[file(".gitattributes")]).unwrap().expect("should emit");
    assert!(ev.changed_paths.contains(&"src/gen.py".to_string()), "{ev:?}");
    assert!(wt.files().iter().find(|f| f.path == "src/gen.py").unwrap().collapsed);
}

#[test]
fn flutter_build_dir_is_not_watched() {
    let repo = TestRepo::new();
    repo.write("pubspec.yaml", "name: app\n");
    repo.commit_all("flutter");
    let wt = open(&repo);
    let l = wt.layout();
    assert_eq!(l.classify(&l.root.join("build/app/x.txt")), Target::Ignore);
    assert_eq!(l.classify(&l.root.join("lib/build.dart")), file("lib/build.dart"));
}

#[test]
fn summary_reports_last_change_time_and_fsmonitor() {
    let repo = feat_repo();
    let wt = open(&repo);
    let s = wt.summary();
    assert_eq!((s.last_changed_at, s.slow_list_ms, s.fsmonitor, s.error), (None, None, false, None));
    repo.write("src/a.py", "a = 2\n");
    wt.handle(&[file("src/a.py")]).unwrap().expect("should emit");
    assert!(wt.summary().last_changed_at.is_some());
    // `git config --get`만 하므로 fsmonitor daemon은 뜨지 않는다
    repo.git(&["config", "core.fsmonitor", "true"]);
    assert!(wt.summary().fsmonitor);
}
```

`crates/core/tests/read_only.rs`:

1. 기존 `ChangedFile` literal에 `collapsed: false,`를 추가한다.
2. 맨 위에 `use wtboard_core::git::attrs::linguist_generated;`, `use wtboard_core::worktree::Worktree;` 추가.
3. 끝에 추가:

```rust
#[test]
fn linked_worktree_reads_and_check_attr_leave_both_indexes_untouched() {
    let repo = TestRepo::new();
    repo.write(".gitattributes", "gen/** linguist-generated\n");
    repo.commit_all("attrs");
    let wt_path = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    std::fs::create_dir_all(wt_path.join("gen")).unwrap();
    std::fs::write(wt_path.join("gen/a.ts"), "x\n").unwrap();
    std::fs::write(wt_path.join("README.md"), "changed\n").unwrap();

    let main_index = repo.path().join(".git/index");
    let wt_index = repo.path().join(".git/worktrees/wt/index");
    let (main_before, wt_before) = (std::fs::read(&main_index).unwrap(), std::fs::read(&wt_index).unwrap());

    let wt = Worktree::open(&wt_path, Some("main".into())).unwrap();
    for f in wt.files() {
        wt.patch(&f.path).unwrap();
    }
    linguist_generated(&Git::new(&wt_path), &["gen/a.ts", "README.md"]).unwrap();

    assert_eq!(std::fs::read(&main_index).unwrap(), main_before, "main index was rewritten");
    assert_eq!(std::fs::read(&wt_index).unwrap(), wt_before, "worktree index was rewritten");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test worktree --test read_only`
Expected: 컴파일 실패 (`collapsed`, `profile` 필드 없음)

- [ ] **Step 3: 구현**

`crates/core/src/domain.rs`의 `ChangedFile`에 추가:

```rust
    /// 생성·lock 파일이라 접어서 보여 준다 (profile pattern 또는 `linguist-generated`)
    pub collapsed: bool,
```

`crates/core/src/git/changes.rs`: `ChangedFile { ... }` literal 두 곳에 `collapsed: false`를 넣는다 (판정은 `Worktree`가 한다).

```rust
            Some(ChangedFile { path, status, additions, deletions, collapsed: false })
```

```rust
        files.push(ChangedFile {
            path,
            status: FileStatus::Untracked,
            additions,
            deletions: additions.map(|_| 0),
            collapsed: false,
        });
```

`crates/core/src/worktree.rs` — 바뀌는 부분:

```rust
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::domain::{ChangedFile, FilePatch};
use crate::git::attrs::linguist_generated;
use crate::git::base::{merge_base, resolve_base, Base, BaseError, BaseSource};
use crate::git::changes::list_changes;
use crate::git::patch::file_patch;
use crate::git::{Git, GitError};
use crate::profile::Profile;
use crate::watch::{Target, WatchLayout};
```

`WorktreeSummary`를 다음으로 바꾸고 `failed`를 추가:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSummary {
    pub id: String,
    pub path: String,
    pub branch: Option<String>,
    pub base: String,
    pub base_source: BaseSource,
    pub changed_files: usize,
    pub profile: Profile,
    /// 앱이 마지막으로 변경을 알린 시각 (Unix ms). 시작 직후에는 없다
    pub last_changed_at: Option<u64>,
    /// 마지막 전체 목록 계산 시간. 느릴 때만 있다 (fsmonitor 안내용)
    pub slow_list_ms: Option<u64>,
    pub fsmonitor: bool,
    /// 열지 못한 이유. 있으면 나머지 필드는 기본값이다
    pub error: Option<String>,
}

impl WorktreeSummary {
    pub fn failed(path: &Path, branch: Option<String>, error: String) -> Self {
        let path = path.to_string_lossy().into_owned();
        Self {
            id: path.clone(),
            path,
            branch,
            base: String::new(),
            base_source: BaseSource::Origin,
            changed_files: 0,
            profile: Profile::Default,
            last_changed_at: None,
            slow_list_ms: None,
            fsmonitor: false,
            error: Some(error),
        }
    }
}

/// 전체 목록 계산이 이보다 오래 걸리면 fsmonitor 안내를 띄운다. `WTBOARD_SLOW_LIST_MS`로 바꿀 수 있다 (UI 확인용)
const SLOW_LIST_MS: u64 = 500;
```

`State` 아래에 `Meta`를 추가하고 `Worktree`에 필드 두 개를 추가:

```rust
/// 재계산 상태와 따로 쌓이는 정보. lock 순서: state → meta (반대로 잡지 않는다)
#[derive(Default)]
struct Meta {
    list_ms: u64,
    last_changed_at: Option<u64>,
}

pub struct Worktree {
    id: String,
    git: Git,
    layout: WatchLayout,
    profile: Profile,
    configured_base: Option<String>,
    state: Mutex<State>,
    meta: Mutex<Meta>,
}
```

`open`:

```rust
    pub fn open(path: &Path, configured_base: Option<String>) -> Result<Arc<Self>, WorktreeError> {
        if !path.is_dir() {
            return Err(WorktreeError::Missing(path.to_path_buf()));
        }
        let layout = WatchLayout::detect(&Git::new(path), &[])?;
        let profile = Profile::detect(&layout.root);
        let layout = layout.excluding(&profile.excluded_dirs());
        let git = Git::new(&layout.root);
        let base = resolve_base(&git, configured_base.as_deref(), None)?;
        let mb = merge_base(&git, &base.name)?;
        let (files, list_ms) = compute_list(&git, profile, &mb, &[])?;
        Ok(Arc::new(Self {
            id: layout.root.to_string_lossy().into_owned(),
            git,
            layout,
            profile,
            configured_base,
            state: Mutex::new(State { base, merge_base: mb, files, hashes: HashMap::new() }),
            meta: Mutex::new(Meta { list_ms, last_changed_at: None }),
        }))
    }

    pub fn profile(&self) -> Profile {
        self.profile
    }
```

`summary`와 `branch`:

```rust
    pub fn summary(&self) -> WorktreeSummary {
        let branch = self.branch();
        let fsmonitor = self
            .git
            .run_str(&["config", "--get", "core.fsmonitor"])
            .is_ok_and(|v| !matches!(v.as_str(), "" | "false" | "no" | "off" | "0"));
        let slow_ms = std::env::var("WTBOARD_SLOW_LIST_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(SLOW_LIST_MS);
        let (list_ms, last_changed_at) = {
            let meta = self.meta.lock().unwrap();
            (meta.list_ms, meta.last_changed_at)
        };
        let st = self.state.lock().unwrap();
        WorktreeSummary {
            id: self.id.clone(),
            path: self.id.clone(),
            branch,
            base: st.base.name.clone(),
            base_source: st.base.source,
            changed_files: st.files.len(),
            profile: self.profile,
            last_changed_at,
            slow_list_ms: (list_ms > slow_ms).then_some(list_ms),
            fsmonitor,
            error: None,
        }
    }

    fn branch(&self) -> Option<String> {
        self.git
            .run_str(&["rev-parse", "--abbrev-ref", "HEAD"])
            .ok()
            .filter(|b| b != "HEAD") // detached HEAD
    }
```

`handle`: `.gitattributes`가 바뀌면 목록 전체를 다시 계산하고, 알릴 event가 있으면 시각을 기록한다.

```rust
        // .gitattributes가 바뀌면 다른 파일의 접기 판정도 달라진다
        let attrs_changed = paths.iter().any(|p| p == ".gitattributes" || p.ends_with("/.gitattributes"));

        let mut events = Vec::new();
        if targets.contains(&Target::Base) {
            events.push(self.refresh_base()?);
        } else if too_many || attrs_changed || targets.contains(&Target::List) {
            events.push(self.refresh_list()?);
        }
        if !paths.is_empty() {
            events.push(self.refresh_paths(&paths)?);
        }
        let ev = merge(events.into_iter().flatten());
        if ev.is_some() {
            self.meta.lock().unwrap().last_changed_at = Some(now_ms());
        }
        Ok(ev)
```

`refresh_base` / `refresh_list` / `refresh_paths`에서 `list_changes` + `to_map`을 `compute_list`로 바꾼다:

```rust
    fn refresh_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let base = resolve_base(&self.git, self.configured_base.as_deref(), None)?;
        let mb = merge_base(&self.git, &base.name)?;
        let unchanged = {
            let st = self.state.lock().unwrap();
            st.base == base && st.merge_base == mb
        };
        if unchanged {
            return self.refresh_list();
        }
        let (files, list_ms) = compute_list(&self.git, self.profile, &mb, &[])?;
        *self.state.lock().unwrap() = State { base, merge_base: mb, files, hashes: HashMap::new() };
        self.meta.lock().unwrap().list_ms = list_ms;
        Ok(Some(self.event(ChangeScope::Base, Vec::new())))
    }

    fn refresh_list(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let mut st = self.state.lock().unwrap();
        let (fresh, list_ms) = compute_list(&self.git, self.profile, &st.merge_base, &[])?;
        self.meta.lock().unwrap().list_ms = list_ms;
        // 이하 기존과 같음
```

`refresh_paths`:

```rust
        let (fresh, _) = compute_list(&self.git, self.profile, &st.merge_base, &refs)?;
```

파일 끝(`to_map` 근처)에 추가:

```rust
/// 변경 목록을 계산하고 접기를 표시한다. 걸린 시간(ms)도 돌려준다
fn compute_list(
    git: &Git,
    profile: Profile,
    merge_base: &str,
    paths: &[&str],
) -> Result<(BTreeMap<String, ChangedFile>, u64), WorktreeError> {
    let started = Instant::now();
    let mut files = list_changes(git, merge_base, paths)?;
    let all: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    // check-attr가 실패해도 목록은 보여 준다. 생성 파일이 아닌 것으로 본다
    let generated = linguist_generated(git, &all).unwrap_or_default();
    for f in &mut files {
        f.collapsed = profile.collapses(&f.path) || generated.contains(&f.path);
    }
    Ok((to_map(files), started.elapsed().as_millis() as u64))
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}
```

`use crate::profile::COMMON_EXCLUDED_DIRS;`는 더 이상 쓰지 않으므로 지운다.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core && cargo build -p wtboard-app`
Expected: 전체 PASS, app 빌드 성공

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/domain.rs crates/core/src/git/changes.rs crates/core/src/worktree.rs crates/core/tests/worktree.rs crates/core/tests/read_only.rs
git commit -m "feat(core): worktree가 profile로 감시를 거르고 생성 파일을 접는다"
```

그다음 `simplify` skill 실행.

---

### Task 7: PR base 반영과 watcher (on_gone, PR lookup)

**Files:**
- Modify: `crates/core/src/worktree.rs`, `crates/core/src/watch/watcher.rs`, `crates/core/src/watch/mod.rs`
- Test: `crates/core/tests/worktree.rs`, `crates/core/tests/watcher.rs`

**Interfaces:**
- Consumes: `Gh` (Task 4), `remote_branch` (Task 5)
- Produces:
  - `Worktree::open_with(path: &Path, configured_base: Option<String>, gh: Gh) -> Result<Arc<Worktree>, WorktreeError>` (`open`은 `Gh::disabled()`로 위임)
  - `Worktree::claim_pr_lookup(&self) -> bool`
  - `Worktree::refresh_pr_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError>`
  - `pub type GoneFn = Arc<dyn Fn() + Send + Sync>;`
  - `spawn_watcher_with(wt: Arc<Worktree>, poll: Duration, emit: EmitFn, on_gone: GoneFn) -> notify::Result<WatchHandle>` (`spawn_watcher`는 no-op `on_gone`으로 위임)

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/common/mod.rs` 끝에 gh stub helper를 추가한다 (worktree, watcher, repo 테스트가 같이 쓴다):

```rust
/// `echo <base>`만 하는 가짜 gh. 반환한 TempDir가 살아 있는 동안 유효하다
pub fn gh_stub(base: &str) -> (tempfile::TempDir, wtboard_core::gh::Gh) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("gh");
    fs::write(&p, format!("#!/bin/sh\necho {base}\n")).unwrap();
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    (dir, wtboard_core::gh::Gh::with_bin(Some(p)))
}

/// `develop` 브랜치가 있는 upstream의 clone에서 `feat` 브랜치로 옮긴 레포. (upstream, clone)
pub fn clone_on_feat_with_develop() -> (TestRepo, TestRepo) {
    let upstream = TestRepo::new();
    upstream.git(&["branch", "develop"]);
    let clone = TestRepo::clone_of(&upstream);
    clone.git(&["checkout", "-q", "-b", "feat"]);
    (upstream, clone)
}
```

`crates/core/tests/worktree.rs` 끝에 추가 (맨 위 `use common::{clone_on_feat_with_develop, gh_stub, TestRepo};`):

```rust
#[test]
fn pr_base_replaces_origin_head_and_emits_base_scope() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh).unwrap();
    assert_eq!(wt.summary().base_source, BaseSource::Origin);

    assert!(wt.claim_pr_lookup());
    assert!(!wt.claim_pr_lookup(), "같은 브랜치는 다시 묻지 않는다");
    let ev = wt.refresh_pr_base().unwrap().expect("base changed");
    assert_eq!(ev.scope, ChangeScope::Base);
    let s = wt.summary();
    assert_eq!((s.base.as_str(), s.base_source), ("origin/develop", BaseSource::Pr));

    clone.git(&["checkout", "-q", "-b", "feat2"]);
    assert!(wt.claim_pr_lookup(), "브랜치가 바뀌면 다시 묻는다");
}

#[test]
fn pr_base_without_local_remote_ref_keeps_origin_head() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("release"); // origin/release 없음
    let wt = Worktree::open_with(clone.path(), None, gh).unwrap();
    assert!(wt.claim_pr_lookup());
    assert_eq!(wt.refresh_pr_base().unwrap(), None);
    assert_eq!(wt.summary().base_source, BaseSource::Origin);
}

#[test]
fn configured_base_never_asks_gh() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), Some("origin/main".into()), gh).unwrap();
    assert!(!wt.claim_pr_lookup());
}

/// Review Focus 5: detached HEAD에는 PR이 없다
#[test]
fn detached_head_never_asks_gh() {
    let (_up, clone) = clone_on_feat_with_develop();
    clone.git(&["checkout", "-q", "--detach"]);
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh).unwrap();
    assert!(!wt.claim_pr_lookup());
}
```

`crates/core/tests/watcher.rs`: `use`를 바꾸고 끝에 추가:

```rust
use common::{clone_on_feat_with_develop, gh_stub, TestRepo};
use wtboard_core::git::base::BaseSource;
use wtboard_core::watch::{spawn_watcher, spawn_watcher_with, WatchHandle};
```

```rust
#[test]
fn pr_base_lookup_runs_in_background_after_start() {
    let (_up, clone) = clone_on_feat_with_develop();
    let (_d, gh) = gh_stub("develop");
    let wt = Worktree::open_with(clone.path(), None, gh).unwrap();
    let (tx, rx) = mpsc::channel();
    let _h = spawn_watcher(wt.clone(), Duration::from_secs(30), Arc::new(move |ev| {
        let _ = tx.send(ev);
    }))
    .unwrap();
    let ev = rx.recv_timeout(Duration::from_secs(3)).expect("no base event");
    assert_eq!(ev.scope, ChangeScope::Base);
    assert_eq!(wt.summary().base_source, BaseSource::Pr);
}

#[test]
fn deleted_worktree_folder_calls_on_gone() {
    let repo = TestRepo::new();
    let wt_path = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt_path.to_str().unwrap()]);
    let wt = Worktree::open(&wt_path, Some("main".into())).unwrap();
    let (tx, rx) = mpsc::channel::<()>();
    let _h = spawn_watcher_with(wt, Duration::from_secs(30), Arc::new(|_| {}), Arc::new(move || {
        let _ = tx.send(());
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300));

    std::fs::remove_dir_all(&wt_path).unwrap();
    rx.recv_timeout(Duration::from_secs(3)).expect("on_gone not called");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test worktree --test watcher`
Expected: 컴파일 실패 (`open_with`, `claim_pr_lookup`, `spawn_watcher_with` 없음)

- [ ] **Step 3: 구현**

`crates/core/src/worktree.rs`:

1. `use crate::gh::Gh;`, `use crate::git::base::{merge_base, remote_branch, resolve_base, Base, BaseError, BaseSource};`
2. `Meta`에 필드 추가:

```rust
#[derive(Default)]
struct Meta {
    list_ms: u64,
    last_changed_at: Option<u64>,
    /// gh가 알려 준 PR base를 로컬 remote 브랜치로 바꾼 것 (`origin/develop`)
    pr_base: Option<String>,
    /// PR base를 마지막으로 물어본 브랜치
    pr_checked: Option<String>,
}
```

3. `Worktree`에 `gh: Gh,` 필드 추가. `open`을 `open_with`로 옮긴다:

```rust
    pub fn open(path: &Path, configured_base: Option<String>) -> Result<Arc<Self>, WorktreeError> {
        Self::open_with(path, configured_base, Gh::disabled())
    }

    /// `gh`가 켜져 있으면 `claim_pr_lookup` / `refresh_pr_base`로 PR base를 반영할 수 있다
    pub fn open_with(path: &Path, configured_base: Option<String>, gh: Gh) -> Result<Arc<Self>, WorktreeError> {
        // 기존 open 본문. Self { ... } 에 `gh,` 추가, Meta는 `Meta { list_ms, ..Default::default() }`
    }
```

4. PR 메서드 추가:

```rust
    /// PR base를 물어야 하면 이 브랜치를 "물었음"으로 표시하고 true.
    /// config base가 있거나, gh가 없거나, detached HEAD거나, 이 브랜치를 이미 물었으면 false
    pub fn claim_pr_lookup(&self) -> bool {
        if self.configured_base.is_some() || !self.gh.enabled() {
            return false;
        }
        let Some(branch) = self.branch() else { return false };
        let mut meta = self.meta.lock().unwrap();
        if meta.pr_checked.as_deref() == Some(branch.as_str()) {
            return false;
        }
        meta.pr_checked = Some(branch);
        true
    }

    /// gh로 PR base를 물어 base에 반영한다. network 호출이라 수 초 걸릴 수 있다. base가 바뀌면 Base event
    pub fn refresh_pr_base(&self) -> Result<Option<WorktreeChanged>, WorktreeError> {
        let pr = self.gh.pr_base(self.git.cwd()).and_then(|b| remote_branch(&self.git, &b));
        self.meta.lock().unwrap().pr_base = pr;
        self.refresh_base()
    }
```

5. `refresh_base`의 첫 줄을 바꾼다:

```rust
        let pr = self.meta.lock().unwrap().pr_base.clone();
        let base = resolve_base(&self.git, self.configured_base.as_deref(), pr.as_deref())?;
```

`crates/core/src/watch/watcher.rs`:

```rust
pub type EmitFn = Arc<dyn Fn(WorktreeChanged) + Send + Sync>;
/// worktree 폴더가 사라졌을 때 부른다 (`rm -rf`). 그 뒤 worker는 멈춘다
pub type GoneFn = Arc<dyn Fn() + Send + Sync>;

pub fn spawn_watcher(wt: Arc<Worktree>, poll: Duration, emit: EmitFn) -> notify::Result<WatchHandle> {
    spawn_watcher_with(wt, poll, emit, Arc::new(|| {}))
}

pub fn spawn_watcher_with(
    wt: Arc<Worktree>,
    poll: Duration,
    emit: EmitFn,
    on_gone: GoneFn,
) -> notify::Result<WatchHandle> {
    // 기존 spawn_watcher 본문 (debouncer 생성, watch_roots 등록)
    lookup_pr(&wt, &emit);
    std::thread::Builder::new()
        .name("wtboard-watch".into())
        .spawn(move || run_worker(wt, rx, poll, emit, on_gone))
        .map_err(notify::Error::io)?;
    Ok(WatchHandle { _debouncer: debouncer })
}
```

`run_worker`의 시그니처에 `on_gone: GoneFn`을 추가하고, `handle` 처리 부분을 바꾼다:

```rust
        match wt.handle(&targets) {
            Ok(Some(ev)) => emit(ev),
            Ok(None) => {}
            Err(_) if !wt.layout().root.is_dir() => {
                on_gone();
                return;
            }
            Err(e) => eprintln!("[wtboard] {}: 재계산 실패: {e}", wt.id()),
        }
        // HEAD가 바뀌었으면(브랜치 전환) PR base를 다시 묻는다. 같은 브랜치면 claim이 막는다
        if targets.contains(&Target::Base) {
            lookup_pr(&wt, &emit);
        }
```

파일 끝에 추가:

```rust
/// 이 브랜치의 PR base를 아직 묻지 않았으면 background thread에서 gh를 부른다
fn lookup_pr(wt: &Arc<Worktree>, emit: &EmitFn) {
    if !wt.claim_pr_lookup() {
        return;
    }
    let (wt, emit) = (wt.clone(), emit.clone());
    let _ = std::thread::Builder::new().name("wtboard-gh".into()).spawn(move || match wt.refresh_pr_base() {
        Ok(Some(ev)) => emit(ev),
        Ok(None) => {}
        Err(e) => eprintln!("[wtboard] {}: PR base 반영 실패: {e}", wt.id()),
    });
}
```

`crates/core/src/watch/mod.rs`: `pub use watcher::{spawn_watcher, spawn_watcher_with, EmitFn, GoneFn, WatchHandle};`

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core && cargo build -p wtboard-app`
Expected: 전체 PASS

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/worktree.rs crates/core/src/watch crates/core/tests
git commit -m "feat(core): PR base를 background에서 반영하고 사라진 worktree를 알린다"
```

그다음 `simplify` skill 실행.

---

### Task 8: worktree 목록과 common dir

**Files:**
- Create: `crates/core/src/git/worktrees.rs`, `crates/core/tests/worktrees.rs`
- Modify: `crates/core/src/git/mod.rs`, `crates/core/tests/common/mod.rs`

**Interfaces:**
- Produces:
  - `pub struct WorktreeEntry { pub path: PathBuf, pub branch: Option<String> }` (`Debug, Clone, PartialEq, Eq`). `path`는 canonical + NFC
  - `pub fn list_worktrees(git: &Git) -> Result<Vec<WorktreeEntry>, GitError>` (git 출력 순서, main이 먼저)
  - `pub fn common_dir(git: &Git) -> Result<PathBuf, GitError>` (canonical)
  - 테스트 helper `common::git_at(cwd: &Path, args: &[&str]) -> String`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/common/mod.rs`의 `fn run`을 `pub fn git_at`로 이름을 바꾸고 공개한다 (`TestRepo` 안의 호출 세 곳도 `git_at`으로).

`crates/core/tests/worktrees.rs`:

```rust
mod common;

use common::{git_at, TestRepo};
use wtboard_core::git::worktrees::{common_dir, list_worktrees};
use wtboard_core::git::Git;

fn names(entries: &[wtboard_core::git::worktrees::WorktreeEntry]) -> Vec<(String, Option<String>)> {
    entries
        .iter()
        .map(|e| (e.path.file_name().unwrap().to_string_lossy().into_owned(), e.branch.clone()))
        .collect()
}

/// Review Focus 3: 공백·한글 경로
#[test]
fn lists_main_and_linked_worktrees_skipping_prunable() {
    let repo = TestRepo::new();
    let parent = repo.path().parent().unwrap();
    let feat = parent.join("wt 기능");
    let gone = parent.join("wt-gone");
    let detached = parent.join("wt-detached");
    repo.git(&["worktree", "add", "-q", "-b", "feat", feat.to_str().unwrap()]);
    repo.git(&["worktree", "add", "-q", "-b", "gone", gone.to_str().unwrap()]);
    repo.git(&["worktree", "add", "-q", "--detach", detached.to_str().unwrap()]);
    std::fs::remove_dir_all(&gone).unwrap();

    let common = common_dir(&Git::new(&feat)).unwrap();
    let entries = list_worktrees(&Git::new(&common)).unwrap();
    assert_eq!(
        names(&entries),
        [
            ("repo".to_string(), Some("main".to_string())),
            ("wt 기능".to_string(), Some("feat".to_string())),
            ("wt-detached".to_string(), None),
        ]
    );
    assert!(entries.iter().all(|e| e.path.is_absolute() && e.path.is_dir()));
}

#[test]
fn common_dir_is_shared_by_all_worktrees() {
    let repo = TestRepo::new();
    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let main = common_dir(&Git::new(repo.path())).unwrap();
    assert_eq!(common_dir(&Git::new(&wt)).unwrap(), main);
    assert!(main.ends_with(".git"));
}

#[test]
fn bare_main_is_skipped() {
    let upstream = TestRepo::new();
    let d = tempfile::tempdir().unwrap();
    git_at(d.path(), &["clone", "-q", "--bare", upstream.path().to_str().unwrap(), "bare.git"]);
    let bare = d.path().join("bare.git");
    git_at(&bare, &["worktree", "add", "-q", "../wt", "main"]);
    assert_eq!(names(&list_worktrees(&Git::new(&bare)).unwrap()), [("wt".to_string(), Some("main".to_string()))]);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test worktrees`
Expected: 컴파일 실패 (`worktrees` 모듈 없음)

- [ ] **Step 3: 구현**

`crates/core/src/git/mod.rs`에 `pub mod worktrees;` 추가.

`crates/core/src/git/worktrees.rs`:

```rust
use std::path::PathBuf;

use super::{Git, GitError};
use crate::watch::canonical;

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
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: 전체 PASS

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/git crates/core/tests/worktrees.rs crates/core/tests/common/mod.rs
git commit -m "feat(core): git worktree list로 worktree를 찾는다"
```

그다음 `simplify` skill 실행.

---

### Task 9: `<common dir>/worktrees/` 감시

**Files:**
- Create: `crates/core/src/watch/repo_watcher.rs`, `crates/core/tests/repo_watcher.rs`
- Modify: `crates/core/src/watch/mod.rs`

**Interfaces:**
- Consumes: `canonical`, `nfc` (Task 2), `common_dir` (Task 8)
- Produces: `pub type RediscoverFn = Arc<dyn Fn() + Send + Sync>;`, `pub struct RepoWatchHandle`, `pub fn spawn_repo_watcher(common_dir: &Path, poll: Duration, on_change: RediscoverFn) -> notify::Result<RepoWatchHandle>`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/repo_watcher.rs`:

```rust
mod common;

use std::sync::{mpsc, Arc};
use std::time::Duration;

use common::TestRepo;
use wtboard_core::git::worktrees::common_dir;
use wtboard_core::git::Git;
use wtboard_core::watch::{spawn_repo_watcher, RepoWatchHandle};

fn start(repo: &TestRepo, poll: Duration) -> (RepoWatchHandle, mpsc::Receiver<()>) {
    let common = common_dir(&Git::new(repo.path())).unwrap();
    let (tx, rx) = mpsc::channel();
    let h = spawn_repo_watcher(&common, poll, Arc::new(move || {
        let _ = tx.send(());
    }))
    .unwrap();
    std::thread::sleep(Duration::from_millis(300)); // FSEvents stream 시작 대기
    (h, rx)
}

fn drain(rx: &mpsc::Receiver<()>) {
    std::thread::sleep(Duration::from_millis(600));
    while rx.try_recv().is_ok() {}
}

#[test]
fn worktree_add_and_remove_trigger_rediscover() {
    let repo = TestRepo::new();
    let (_h, rx) = start(&repo, Duration::from_secs(30));
    let wt = repo.path().parent().unwrap().join("wt");

    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("add not seen");
    drain(&rx);

    repo.git(&["worktree", "remove", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("remove not seen");
}

#[test]
fn ordinary_git_activity_is_ignored() {
    let repo = TestRepo::new();
    let (_h, rx) = start(&repo, Duration::from_secs(30));
    repo.write("a.txt", "a\n");
    repo.commit_all("a"); // objects, refs, index, logs
    assert!(rx.recv_timeout(Duration::from_millis(1200)).is_err());
}

#[test]
fn poll_triggers_rediscover() {
    let repo = TestRepo::new();
    let (_h, rx) = start(&repo, Duration::from_millis(500));
    rx.recv_timeout(Duration::from_secs(2)).expect("poll not fired");
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test repo_watcher`
Expected: 컴파일 실패

- [ ] **Step 3: 구현**

`crates/core/src/watch/mod.rs`:

```rust
mod repo_watcher;
mod watcher;

pub use repo_watcher::{spawn_repo_watcher, RediscoverFn, RepoWatchHandle};
pub use watcher::{spawn_watcher, spawn_watcher_with, EmitFn, GoneFn, WatchHandle};
```

`crates/core/src/watch/repo_watcher.rs`:

```rust
use std::path::{Component, Path};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};

use super::{canonical, nfc};

pub type RediscoverFn = Arc<dyn Fn() + Send + Sync>;

const DEBOUNCE: Duration = Duration::from_millis(300);

/// drop하면 감시가 멈추고 worker thread도 끝난다.
pub struct RepoWatchHandle {
    _debouncer: Debouncer<RecommendedWatcher, NoCache>,
}

/// worktree가 생기거나 사라질 때(`<common dir>/worktrees/<이름>`), 그리고 `poll`마다 `on_change`를 부른다.
/// `worktrees/`는 첫 worktree를 만들 때 생기므로 common dir 전체를 감시하고 경로로 거른다.
/// file id cache를 쓰지 않아(`NoCache`) 시작할 때 `.git/objects`를 훑지 않는다.
pub fn spawn_repo_watcher(common_dir: &Path, poll: Duration, on_change: RediscoverFn) -> notify::Result<RepoWatchHandle> {
    let common_dir = canonical(common_dir);
    let filter_dir = common_dir.clone();
    let (tx, rx) = mpsc::channel::<()>();
    let mut debouncer = new_debouncer_opt::<_, RecommendedWatcher, NoCache>(
        DEBOUNCE,
        None,
        move |res: DebounceEventResult| {
            let relevant = match res {
                Ok(events) => events
                    .iter()
                    .any(|e| e.need_rescan() || e.event.paths.iter().any(|p| is_worktree_entry(&filter_dir, p))),
                Err(_) => true,
            };
            if relevant {
                let _ = tx.send(());
            }
        },
        NoCache,
        notify::Config::default(),
    )?;
    debouncer.watch(&common_dir, RecursiveMode::Recursive)?;
    std::thread::Builder::new()
        .name("wtboard-repo-watch".into())
        .spawn(move || loop {
            match rx.recv_timeout(poll) {
                Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => on_change(),
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        })
        .map_err(notify::Error::io)?;
    Ok(RepoWatchHandle { _debouncer: debouncer })
}

/// `worktrees` 자신이나 `worktrees/<이름>`. 그 안의 파일(`HEAD`, `index`)은 커밋마다 바뀌므로 보지 않는다
fn is_worktree_entry(common_dir: &Path, path: &Path) -> bool {
    let path = nfc(path);
    let Ok(rel) = path.strip_prefix(common_dir) else { return false };
    let parts: Vec<Component> = rel.components().collect();
    matches!(parts.first(), Some(Component::Normal(first)) if *first == "worktrees") && parts.len() <= 2
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test repo_watcher`
Expected: PASS (3개). `ordinary_git_activity_is_ignored`가 실패하면 event 경로를 `eprintln!`으로 찍어 어떤 경로가 걸렸는지 확인한다.

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/watch crates/core/tests/repo_watcher.rs
git commit -m "feat(core): common dir의 worktrees/ 변화를 감시한다"
```

그다음 `simplify` skill 실행.

---

### Task 10: `Repo` — worktree 묶음

**Files:**
- Create: `crates/core/src/repo.rs`, `crates/core/tests/repo.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Worktree::open_with` (Task 7), `spawn_watcher_with`/`GoneFn` (Task 7), `list_worktrees`/`WorktreeEntry` (Task 8), `spawn_repo_watcher` (Task 9), `WorktreeSummary::failed` (Task 6)
- Produces:
  - `pub type UpdatedFn = Arc<dyn Fn() + Send + Sync>;`
  - `#[derive(Clone)] pub struct Sink { pub changed: EmitFn, pub updated: UpdatedFn }`
  - `#[derive(Debug, Clone)] pub struct RepoOptions { pub base: Option<String>, pub gh: Gh, pub poll: Duration }`
  - `pub enum RepoError` (`Git`, `Watch`)
  - `pub struct RepoSummary { pub id: String, pub name: String, pub profile: Profile, pub worktrees: Vec<WorktreeSummary> }` (`Serialize`, camelCase)
  - `Repo::open(common_dir: &Path, opts: RepoOptions, sink: Sink) -> Result<Arc<Repo>, RepoError>`
  - `Repo::rediscover(self: &Arc<Self>) -> Result<bool, GitError>`
  - `Repo::summary(&self) -> RepoSummary`, `Repo::worktree(&self, id: &str) -> Option<Arc<Worktree>>`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/repo.rs`:

```rust
mod common;

use std::sync::{mpsc, Arc};
use std::time::Duration;

use common::TestRepo;
use wtboard_core::gh::Gh;
use wtboard_core::git::worktrees::common_dir;
use wtboard_core::git::Git;
use wtboard_core::repo::{Repo, RepoOptions, Sink};

fn open(repo: &TestRepo, base: Option<&str>) -> (Arc<Repo>, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel();
    let sink = Sink {
        changed: Arc::new(|_| {}),
        updated: Arc::new(move || {
            let _ = tx.send(());
        }),
    };
    let opts = RepoOptions { base: base.map(String::from), gh: Gh::disabled(), poll: Duration::from_secs(30) };
    let r = Repo::open(&common_dir(&Git::new(repo.path())).unwrap(), opts, sink).unwrap();
    std::thread::sleep(Duration::from_millis(300)); // FSEvents stream 시작 대기
    (r, rx)
}

fn branches(r: &Repo) -> Vec<Option<String>> {
    r.summary().worktrees.into_iter().map(|w| w.branch).collect()
}

#[test]
fn opens_every_worktree_in_git_order() {
    let repo = TestRepo::new();
    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    std::fs::write(wt.join("new.txt"), "n\n").unwrap();

    let (r, _rx) = open(&repo, Some("main"));
    let s = r.summary();
    assert_eq!(s.name, "repo");
    let got: Vec<_> = s.worktrees.iter().map(|w| (w.branch.clone(), w.changed_files)).collect();
    assert_eq!(got, [(Some("main".to_string()), 0), (Some("feat".to_string()), 1)]);
    assert!(r.worktree(&s.worktrees[1].id).is_some());
}

#[test]
fn worktree_add_and_remove_update_the_list_and_notify() {
    let repo = TestRepo::new();
    let (r, rx) = open(&repo, Some("main"));
    let wt = repo.path().parent().unwrap().join("wt");

    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("add not notified");
    assert_eq!(branches(&r), [Some("main".to_string()), Some("feat".to_string())]);

    repo.git(&["worktree", "remove", wt.to_str().unwrap()]);
    rx.recv_timeout(Duration::from_secs(3)).expect("remove not notified");
    assert_eq!(branches(&r), [Some("main".to_string())]);
}

#[test]
fn rm_rf_of_a_worktree_folder_drops_it() {
    let repo = TestRepo::new();
    let wt = repo.path().parent().unwrap().join("wt");
    repo.git(&["worktree", "add", "-q", "-b", "feat", wt.to_str().unwrap()]);
    let (r, rx) = open(&repo, Some("main"));
    assert_eq!(r.summary().worktrees.len(), 2);

    std::fs::remove_dir_all(&wt).unwrap();
    rx.recv_timeout(Duration::from_secs(3)).expect("removal not notified");
    assert_eq!(branches(&r), [Some("main".to_string())]);
}

#[test]
fn failed_worktree_is_listed_with_error_and_retried() {
    let upstream = TestRepo::new();
    let clone = TestRepo::clone_of(&upstream);
    clone.git(&["remote", "set-head", "origin", "-d"]); // origin/HEAD 없음 → base를 정할 수 없음
    let (r, _rx) = open(&clone, None);
    let err = r.summary().worktrees[0].error.clone().expect("should fail");
    assert!(err.contains("base"), "{err}");

    clone.git(&["remote", "set-head", "origin", "main"]);
    assert!(r.rediscover().unwrap(), "다시 열렸으면 목록이 바뀐 것");
    assert_eq!(r.summary().worktrees[0].error, None);
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test repo`
Expected: 컴파일 실패 (`repo` 모듈 없음)

- [ ] **Step 3: 구현**

`crates/core/src/lib.rs`에 `pub mod repo;` 추가.

`crates/core/src/repo.rs`:

```rust
//! 레포 하나(common dir 하나)와 그 worktree들
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::gh::Gh;
use crate::git::worktrees::{list_worktrees, WorktreeEntry};
use crate::git::{Git, GitError};
use crate::profile::Profile;
use crate::watch::{spawn_repo_watcher, spawn_watcher_with, EmitFn, GoneFn, RepoWatchHandle, WatchHandle};
use crate::worktree::{Worktree, WorktreeSummary};

/// worktree 목록이 바뀌었을 때 (추가, 삭제, 실패했던 worktree가 열림)
pub type UpdatedFn = Arc<dyn Fn() + Send + Sync>;

/// 화면으로 보내는 알림 두 종류
#[derive(Clone)]
pub struct Sink {
    pub changed: EmitFn,
    pub updated: UpdatedFn,
}

#[derive(Debug, Clone)]
pub struct RepoOptions {
    /// config의 `base`. 모든 worktree에 같이 적용한다
    pub base: Option<String>,
    pub gh: Gh,
    pub poll: Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("worktree 목록 감시를 시작할 수 없음: {0}")]
    Watch(#[from] notify::Error),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub id: String,
    pub name: String,
    /// 첫 번째로 열린 worktree(보통 main)의 profile
    pub profile: Profile,
    pub worktrees: Vec<WorktreeSummary>,
}

struct Opened {
    path: PathBuf,
    branch: Option<String>,
    result: Result<Arc<Worktree>, String>,
    _watch: Option<WatchHandle>,
}

pub struct Repo {
    id: String,
    name: String,
    /// common dir에서 실행한다. config에 적힌 worktree가 지워져도 목록을 읽을 수 있다
    git: Git,
    opts: RepoOptions,
    sink: Sink,
    worktrees: Mutex<Vec<Opened>>,
    watch: Mutex<Option<RepoWatchHandle>>,
}

impl Repo {
    pub fn open(common_dir: &Path, opts: RepoOptions, sink: Sink) -> Result<Arc<Self>, RepoError> {
        let repo = Arc::new(Self {
            id: common_dir.to_string_lossy().into_owned(),
            name: repo_name(common_dir),
            git: Git::new(common_dir),
            opts,
            sink,
            worktrees: Mutex::new(Vec::new()),
            watch: Mutex::new(None),
        });
        repo.rediscover()?;
        let weak = Arc::downgrade(&repo);
        let handle = spawn_repo_watcher(common_dir, repo.opts.poll, Arc::new(move || {
            if let Some(r) = weak.upgrade() {
                r.rediscover_and_notify();
            }
        }))?;
        *repo.watch.lock().unwrap() = Some(handle);
        Ok(repo)
    }

    /// worktree 목록을 다시 읽는다. 새 worktree와 전에 열지 못한 worktree는 (다시) 열고, 사라진 worktree는 닫는다.
    /// 화면에 보일 목록(경로와 성공 여부)이 바뀌었으면 true
    pub fn rediscover(self: &Arc<Self>) -> Result<bool, GitError> {
        let entries = list_worktrees(&self.git)?;
        let mut opened = self.worktrees.lock().unwrap();
        let before = signature(&opened);
        opened.retain(|o| o.result.is_ok() && entries.iter().any(|e| e.path == o.path));
        let missing: Vec<&WorktreeEntry> =
            entries.iter().filter(|e| !opened.iter().any(|o| o.path == e.path)).collect();
        let fresh: Vec<Opened> = std::thread::scope(|s| {
            let handles: Vec<_> = missing.iter().map(|e| s.spawn(move || self.open_worktree(e))).collect();
            handles.into_iter().map(|h| h.join().expect("worktree 열기 thread panic")).collect()
        });
        opened.extend(fresh);
        opened.sort_by_key(|o| entries.iter().position(|e| e.path == o.path));
        Ok(signature(&opened) != before)
    }

    pub fn summary(&self) -> RepoSummary {
        let opened = self.worktrees.lock().unwrap();
        let worktrees: Vec<WorktreeSummary> = opened
            .iter()
            .map(|o| match &o.result {
                Ok(wt) => wt.summary(),
                Err(e) => WorktreeSummary::failed(&o.path, o.branch.clone(), e.clone()),
            })
            .collect();
        let profile = worktrees.iter().find(|w| w.error.is_none()).map_or(Profile::Default, |w| w.profile);
        RepoSummary { id: self.id.clone(), name: self.name.clone(), profile, worktrees }
    }

    pub fn worktree(&self, id: &str) -> Option<Arc<Worktree>> {
        self.worktrees
            .lock()
            .unwrap()
            .iter()
            .filter_map(|o| o.result.as_ref().ok())
            .find(|wt| wt.id() == id)
            .cloned()
    }

    fn open_worktree(self: &Arc<Self>, entry: &WorktreeEntry) -> Opened {
        let opened = Worktree::open_with(&entry.path, self.opts.base.clone(), self.opts.gh.clone());
        let (result, watch) = match opened {
            Ok(wt) => match spawn_watcher_with(wt.clone(), self.opts.poll, self.sink.changed.clone(), self.on_gone()) {
                Ok(h) => (Ok(wt), Some(h)),
                Err(e) => (Err(format!("파일 감시를 시작할 수 없음: {e}")), None),
            },
            Err(e) => (Err(e.to_string()), None),
        };
        Opened { path: entry.path.clone(), branch: entry.branch.clone(), result, _watch: watch }
    }

    /// worktree 폴더가 사라지면 목록을 다시 읽는다. watcher worker 안에서 불리므로 새 thread에서 한다
    fn on_gone(self: &Arc<Self>) -> GoneFn {
        let weak = Arc::downgrade(self);
        Arc::new(move || {
            if let Some(repo) = weak.upgrade() {
                std::thread::spawn(move || repo.rediscover_and_notify());
            }
        })
    }

    fn rediscover_and_notify(self: &Arc<Self>) {
        match self.rediscover() {
            Ok(true) => (self.sink.updated)(),
            Ok(false) => {}
            Err(e) => eprintln!("[wtboard] {}: worktree 목록을 읽을 수 없음: {e}", self.name),
        }
    }
}

fn signature(opened: &[Opened]) -> Vec<(PathBuf, bool)> {
    opened.iter().map(|o| (o.path.clone(), o.result.is_ok())).collect()
}

/// `~/dev/my-api/.git` → `my-api`, bare `~/dev/my-api.git` → `my-api`
fn repo_name(common_dir: &Path) -> String {
    let dir = if common_dir.file_name().is_some_and(|n| n == ".git") {
        common_dir.parent().unwrap_or(common_dir)
    } else {
        common_dir
    };
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    name.strip_suffix(".git").map(str::to_owned).unwrap_or(name)
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: 전체 PASS

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/lib.rs crates/core/src/repo.rs crates/core/tests/repo.rs
git commit -m "feat(core): 레포의 모든 worktree를 열고 추가·삭제를 반영한다"
```

그다음 `simplify` skill 실행.

---

### Task 11: `Registry` — config 전체

**Files:**
- Create: `crates/core/src/registry.rs`, `crates/core/tests/registry.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: `Repo`, `RepoOptions`, `Sink`, `RepoSummary` (Task 10), `common_dir` (Task 8), `WatchConfig::poll_interval` (Task 1)
- Produces:
  - `#[derive(Debug, Clone, Serialize)] pub struct EntryError { pub path: String, pub message: String }`
  - `Registry::open(config: &Config, gh: Gh, sink: Sink) -> Registry`
  - `Registry::repos(&self) -> Vec<RepoSummary>`, `Registry::errors(&self) -> &[EntryError]`, `Registry::worktree(&self, id: &str) -> Option<Arc<Worktree>>`

- [ ] **Step 1: 실패하는 테스트**

`crates/core/tests/registry.rs`:

```rust
mod common;

use std::path::Path;
use std::sync::Arc;

use common::TestRepo;
use wtboard_core::gh::Gh;
use wtboard_core::registry::Registry;
use wtboard_core::repo::Sink;
use wtboard_core::store::config::Config;

fn sink() -> Sink {
    Sink { changed: Arc::new(|_| {}), updated: Arc::new(|| {}) }
}

fn registry(entries: &[(&Path, Option<&str>)]) -> Registry {
    let mut toml = String::new();
    for (path, base) in entries {
        toml += &format!("[[repo]]\npath = {:?}\n", path.to_str().unwrap());
        if let Some(b) = base {
            toml += &format!("base = {b:?}\n");
        }
    }
    Registry::open(&Config::parse(&toml).unwrap(), Gh::disabled(), sink())
}

fn linked(repo: &TestRepo, name: &str, branch: &str) -> std::path::PathBuf {
    let p = repo.path().parent().unwrap().join(name);
    repo.git(&["worktree", "add", "-q", "-b", branch, p.to_str().unwrap()]);
    p
}

#[test]
fn entries_of_the_same_repo_merge_into_one() {
    let repo = TestRepo::new();
    let wt = linked(&repo, "wt", "feat");
    let reg = registry(&[(repo.path(), Some("main")), (&wt, Some("main"))]);
    let repos = reg.repos();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0].worktrees.len(), 2);
    assert!(reg.errors().is_empty(), "{:?}", reg.errors());
}

#[test]
fn conflicting_base_keeps_the_first_and_warns() {
    let repo = TestRepo::new();
    let wt = linked(&repo, "wt", "feat");
    let reg = registry(&[(repo.path(), Some("main")), (&wt, Some("feat"))]);
    assert_eq!(reg.repos()[0].worktrees[0].base, "main");
    assert_eq!(reg.errors().len(), 1);
    assert!(reg.errors()[0].message.contains("base"), "{:?}", reg.errors());
}

#[test]
fn missing_path_is_reported_and_others_still_open() {
    let repo = TestRepo::new();
    let reg = registry(&[(Path::new("/nonexistent/wtboard-x"), None), (repo.path(), Some("main"))]);
    assert_eq!(reg.repos().len(), 1);
    assert_eq!(reg.errors()[0].path, "/nonexistent/wtboard-x");
    assert!(reg.errors()[0].message.contains("경로 없음"), "{:?}", reg.errors());
}

/// Review Focus 2: config가 가리키던 worktree가 지워져도 같은 레포의 나머지는 계속 보인다
#[test]
fn removing_the_configured_worktree_keeps_the_rest_of_the_repo() {
    let repo = TestRepo::new();
    let wt = linked(&repo, "wt", "feat");
    let reg = registry(&[(&wt, Some("main"))]);
    assert_eq!(reg.repos()[0].worktrees.len(), 2);

    repo.git(&["worktree", "remove", wt.to_str().unwrap()]);
    std::thread::sleep(std::time::Duration::from_secs(2)); // repo watcher가 반영할 시간
    let worktrees = &reg.repos()[0].worktrees;
    assert_eq!(worktrees.len(), 1);
    assert_eq!(worktrees[0].branch.as_deref(), Some("main"));
}

#[test]
fn worktree_lookup_spans_repos() {
    let (a, b) = (TestRepo::new(), TestRepo::new());
    let reg = registry(&[(a.path(), Some("main")), (b.path(), Some("main"))]);
    for r in reg.repos() {
        assert!(reg.worktree(&r.worktrees[0].id).is_some());
    }
    assert!(reg.worktree("/nope").is_none());
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test registry`
Expected: 컴파일 실패 (`registry` 모듈 없음)

- [ ] **Step 3: 구현**

`crates/core/src/lib.rs`에 `pub mod registry;` 추가.

`crates/core/src/registry.rs`:

```rust
//! config에 등록한 레포 전체
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;

use crate::gh::Gh;
use crate::git::worktrees::common_dir;
use crate::git::Git;
use crate::repo::{Repo, RepoOptions, RepoSummary, Sink};
use crate::store::config::Config;
use crate::worktree::Worktree;

/// 열지 못한 config entry, 또는 entry에 대한 경고
#[derive(Debug, Clone, Serialize)]
pub struct EntryError {
    /// config에 적힌 그대로
    pub path: String,
    pub message: String,
}

pub struct Registry {
    repos: Vec<Arc<Repo>>,
    errors: Vec<EntryError>,
}

struct Group {
    common_dir: PathBuf,
    base: Option<String>,
    first_path: String,
}

impl Registry {
    /// config entry를 common dir로 묶고 레포를 병렬로 연다. 실패한 entry는 `errors`에 남기고 나머지는 계속 연다
    pub fn open(config: &Config, gh: Gh, sink: Sink) -> Self {
        let mut errors = Vec::new();
        let mut groups: Vec<Group> = Vec::new();
        for entry in &config.repos {
            let fail = |message: String| EntryError { path: entry.path.clone(), message };
            let path = entry.resolved_path();
            if !path.is_dir() {
                errors.push(fail(format!("경로 없음: {}", path.display())));
                continue;
            }
            let dir = match common_dir(&Git::new(&path)) {
                Ok(d) => d,
                Err(e) => {
                    errors.push(fail(e.to_string()));
                    continue;
                }
            };
            match groups.iter().position(|g| g.common_dir == dir) {
                None => groups.push(Group { common_dir: dir, base: entry.base.clone(), first_path: entry.path.clone() }),
                Some(i) if groups[i].base != entry.base => errors.push(fail(format!(
                    "{}와 같은 레포인데 base가 다릅니다. 앞 entry의 base({})를 씁니다",
                    groups[i].first_path,
                    groups[i].base.as_deref().unwrap_or("자동")
                ))),
                Some(_) => {}
            }
        }

        let poll = config.watch.poll_interval();
        let opened: Vec<Result<Arc<Repo>, EntryError>> = std::thread::scope(|s| {
            let handles: Vec<_> = groups
                .iter()
                .map(|g| {
                    let opts = RepoOptions { base: g.base.clone(), gh: gh.clone(), poll };
                    let sink = sink.clone();
                    s.spawn(move || {
                        Repo::open(&g.common_dir, opts, sink)
                            .map_err(|e| EntryError { path: g.first_path.clone(), message: e.to_string() })
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("레포 열기 thread panic")).collect()
        });
        let mut repos = Vec::new();
        for r in opened {
            match r {
                Ok(repo) => repos.push(repo),
                Err(e) => errors.push(e),
            }
        }
        Self { repos, errors }
    }

    pub fn repos(&self) -> Vec<RepoSummary> {
        self.repos.iter().map(|r| r.summary()).collect()
    }

    pub fn errors(&self) -> &[EntryError] {
        &self.errors
    }

    pub fn worktree(&self, id: &str) -> Option<Arc<Worktree>> {
        self.repos.iter().find_map(|r| r.worktree(id))
    }
}
```

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: 전체 PASS

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/core/src/lib.rs crates/core/src/registry.rs crates/core/tests/registry.rs
git commit -m "feat(core): config의 레포를 common dir로 묶어 모두 연다"
```

그다음 `simplify` skill 실행.

---

### Task 12: app을 Registry에 연결 + fixture script

**Files:**
- Modify: `crates/app/src/main.rs`, `ui/src/api.ts`, `ui/src/App.tsx`, `ui/src/useWorktreeEvents.ts`, `ui/src/lib/tree.test.ts`
- Create: `scripts/fixtures.sh`

**Interfaces:**
- Consumes: `Registry`, `EntryError` (Task 11), `Sink` (Task 10), `Gh::detect` (Task 4), `Config::default_path` (Task 1)
- Produces:
  - command `list_repos` → `{ ready: bool, repos: RepoSummary[], errors: EntryError[] }`
  - event `worktrees-updated` (payload 없음)
  - UI 타입: `Profile`, `BaseSource`, `ReposResponse`, `EntryError`, `ChangedFile.collapsed`, `WorktreeSummary` 새 필드

- [ ] **Step 1: app 구현**

app은 Tauri 런타임이 필요해 자동 테스트가 없다. 빌드와 실제 화면으로 확인한다.

`crates/app/src/main.rs` 전체:

```rust
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use wtboard_core::domain::{ChangedFile, FilePatch};
use wtboard_core::gh::Gh;
use wtboard_core::registry::{EntryError, Registry};
use wtboard_core::repo::{RepoSummary, Sink};
use wtboard_core::store::config::Config;
use wtboard_core::worktree::{Worktree, WorktreeChanged};

/// Registry는 background thread에서 열고 끝나면 채운다. 그 전에는 비어 있다
struct AppState {
    registry: OnceLock<Result<Registry, String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReposResponse {
    ready: bool,
    repos: Vec<RepoSummary>,
    errors: Vec<EntryError>,
}

#[tauri::command]
async fn list_repos(app: AppHandle) -> Result<ReposResponse, String> {
    blocking(move || match app.state::<AppState>().registry.get() {
        None => Ok(ReposResponse { ready: false, repos: Vec::new(), errors: Vec::new() }),
        Some(Err(e)) => Err(e.clone()),
        Some(Ok(reg)) => Ok(ReposResponse { ready: true, repos: reg.repos(), errors: reg.errors().to_vec() }),
    })
    .await
}

#[tauri::command]
async fn list_files(app: AppHandle, worktree_id: String, view: String) -> Result<Vec<ChangedFile>, String> {
    ensure_branch_view(&view)?;
    blocking(move || Ok(worktree(&app, &worktree_id)?.files())).await
}

#[tauri::command]
async fn get_patch(app: AppHandle, worktree_id: String, view: String, path: String) -> Result<FilePatch, String> {
    ensure_branch_view(&view)?;
    blocking(move || worktree(&app, &worktree_id)?.patch(&path).map_err(|e| e.to_string())).await
}

/// git을 부르는 일은 tokio worker를 막지 않게 blocking pool에서 한다
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn worktree(app: &AppHandle, id: &str) -> Result<Arc<Worktree>, String> {
    match app.state::<AppState>().registry.get() {
        Some(Ok(reg)) => reg.worktree(id).ok_or_else(|| format!("알 수 없는 worktree: {id}")),
        Some(Err(e)) => Err(e.clone()),
        None => Err("아직 불러오는 중입니다".into()),
    }
}

fn ensure_branch_view(view: &str) -> Result<(), String> {
    if view == "branch" {
        Ok(())
    } else {
        Err(format!("아직 지원하지 않는 view: {view}"))
    }
}

fn open_registry(handle: &AppHandle) -> Result<Registry, String> {
    let path = Config::default_path();
    let hint = format!("{} 에 다음을 추가하세요:\n\n[[repo]]\npath = \"~/dev/my-repo\"", path.display());
    let config = Config::load(&path).map_err(|e| format!("{e}\n\n{hint}"))?;
    if config.repos.is_empty() {
        return Err(format!("등록된 레포가 없습니다.\n\n{hint}"));
    }
    let (changed, updated) = (handle.clone(), handle.clone());
    let sink = Sink {
        changed: Arc::new(move |ev: WorktreeChanged| {
            let _ = changed.emit("worktree-changed", ev);
        }),
        updated: Arc::new(move || {
            let _ = updated.emit("worktrees-updated", ());
        }),
    };
    Ok(Registry::open(&config, Gh::detect(), sink))
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            app.manage(AppState { registry: OnceLock::new() });
            let handle = app.handle().clone();
            // 레포가 많거나 크면 수 초 걸린다. 창을 막지 않는다
            std::thread::spawn(move || {
                let result = open_registry(&handle);
                let _ = handle.state::<AppState>().registry.set(result);
                let _ = handle.emit("worktrees-updated", ());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![list_repos, list_files, get_patch])
        .run(tauri::generate_context!())
        .expect("wtboard 실행 실패");
}
```

Run: `source ~/.cargo/env && cargo build -p wtboard-app`
Expected: 빌드 성공

- [ ] **Step 2: UI 타입과 임시 화면**

`ui/src/api.ts` — 타입을 바꾼다:

```ts
export type Profile = 'flutter' | 'node' | 'python' | 'default';
export type BaseSource = 'config' | 'pr' | 'origin';

export interface ChangedFile {
  path: string;
  status: FileStatus;
  /** null이면 바이너리 (또는 1MB 넘는 untracked) */
  additions: number | null;
  deletions: number | null;
  /** 생성·lock 파일. 펼치기 전에는 diff를 계산하지 않는다 */
  collapsed: boolean;
}

export interface WorktreeSummary {
  id: string;
  path: string;
  branch: string | null;
  base: string;
  baseSource: BaseSource;
  changedFiles: number;
  profile: Profile;
  /** Unix ms. 앱이 마지막으로 변경을 알린 시각 */
  lastChangedAt: number | null;
  /** 목록 계산이 느릴 때만 값이 있다 */
  slowListMs: number | null;
  fsmonitor: boolean;
  /** 열지 못한 이유 */
  error: string | null;
}

export interface RepoSummary {
  id: string;
  name: string;
  profile: Profile;
  worktrees: WorktreeSummary[];
}

export interface EntryError {
  path: string;
  message: string;
}

export interface ReposResponse {
  /** false면 아직 레포를 여는 중 */
  ready: boolean;
  repos: RepoSummary[];
  errors: EntryError[];
}

export const listRepos = () => invoke<ReposResponse>('list_repos');
```

`ui/src/lib/tree.test.ts`의 helper에 `collapsed: false`를 넣는다:

```ts
const f = (path: string): ChangedFile => ({ path, status: 'modified', additions: 1, deletions: 0, collapsed: false });
```

`ui/src/useWorktreeEvents.ts`:

```ts
/** worktree-changed event가 오면 관련 query만 무효화한다. worktree 목록이 바뀌면 레포 목록을 다시 읽는다. */
export function useWorktreeEvents() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const unlistenChanged = listen<WorktreeChanged>('worktree-changed', ({ payload }) => {
      for (const queryKey of keysToInvalidate(payload)) queryClient.invalidateQueries({ queryKey });
    });
    const unlistenUpdated = listen('worktrees-updated', () => {
      queryClient.invalidateQueries({ queryKey: ['repos'] });
    });
    return () => {
      unlistenChanged.then((off) => off());
      unlistenUpdated.then((off) => off());
    };
  }, [queryClient]);
}
```

`ui/src/App.tsx`의 `App`만 바꾼다 (sidebar는 Task 13. 지금은 열린 첫 worktree를 보여 준다):

```tsx
export function App() {
  useWorktreeEvents();
  const repos = useQuery({
    queryKey: ['repos'],
    queryFn: listRepos,
    // 시작 직후 event를 놓쳐도 준비되면 화면이 뜨게 한다
    refetchInterval: (q) => (q.state.data?.ready === false ? 500 : false),
  });
  if (repos.error) return <ErrorScreen message={String(repos.error)} />;
  if (!repos.data?.ready) return <p className="p-4 text-gray-500">불러오는 중…</p>;
  const repo = repos.data.repos.find((r) => r.worktrees.some((w) => !w.error));
  const worktree = repo?.worktrees.find((w) => !w.error);
  if (!repo || !worktree) {
    const reasons = repos.data.errors.map((e) => `${e.path}: ${e.message}`).join('\n');
    return <ErrorScreen message={reasons || '열 수 있는 worktree가 없습니다'} />;
  }
  return <WorktreeView repoName={repo.name} worktree={worktree} />;
}
```

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS, 빌드 성공

- [ ] **Step 3: fixture script**

`scripts/fixtures.sh` (실행 권한 `chmod +x`):

```sh
#!/bin/sh
# UI 확인용 fixture 레포 3개(Python / Node / Flutter)와 config.toml을 만든다. 실행할 때마다 새로 만든다.
# 사용: scripts/fixtures.sh <dir>
#       WTBOARD_CONFIG=<dir>/config.toml pnpm tauri dev
set -eu
[ $# -eq 1 ] || { echo "usage: $0 <dir>" >&2; exit 1; }
mkdir -p "$1"
DIR=$(cd "$1" && pwd)
export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
g() { git -c user.email=fixture@example.com -c user.name=fixture -c commit.gpgsign=false "$@"; }

# $1 이름. stdin의 "경로<TAB>내용" 줄로 upstream main을 만들고 clone + feat worktree를 연다
repo() {
  name=$1
  rm -rf "$DIR/upstream-$name" "$DIR/$name" "$DIR/$name-feat"
  mkdir -p "$DIR/upstream-$name"
  while IFS="	" read -r path body; do
    mkdir -p "$DIR/upstream-$name/$(dirname "$path")"
    printf '%b' "$body" > "$DIR/upstream-$name/$path"
  done
  g -C "$DIR/upstream-$name" init -q -b main
  g -C "$DIR/upstream-$name" add -A
  g -C "$DIR/upstream-$name" commit -q -m init
  g clone -q "$DIR/upstream-$name" "$DIR/$name"
  g -C "$DIR/$name" worktree add -q -b feat "$DIR/$name-feat"
}

# $1 worktree, $2 경로, $3 내용
put() { mkdir -p "$(dirname "$1/$2")"; printf '%b' "$3" > "$1/$2"; }

repo py <<'EOF'
requirements.txt	fastapi\n
.gitignore	venv/\n
app/main.py	from fastapi import FastAPI\n\napp = FastAPI()\n
EOF
put "$DIR/py-feat" app/main.py 'from fastapi import FastAPI\n\napp = FastAPI(title="fixture")\n\n\n@app.get("/health")\ndef health():\n    return {"ok": True}\n'
put "$DIR/py-feat" app/users.py 'def list_users():\n    return []\n'
put "$DIR/py-feat" uv.lock 'version = 1\n'
put "$DIR/py-feat" venv/lib/site.py '# 무시됨\n'

repo node <<'EOF'
package.json	{ "dependencies": { "next": "15.0.0" } }\n
.gitattributes	generated/** linguist-generated\n
app/page.tsx	export default function Page() {\n  return <h1>hello</h1>;\n}\n
EOF
put "$DIR/node-feat" app/page.tsx 'export default function Page() {\n  return <h1>hello, fixture</h1>;\n}\n'
put "$DIR/node-feat" package-lock.json '{ "lockfileVersion": 3 }\n'
put "$DIR/node-feat" next-env.d.ts '/// <reference types="next" />\n'
put "$DIR/node-feat" generated/api.ts 'export const api = {};\n'

repo flutter <<'EOF'
pubspec.yaml	name: fixture\n
.gitignore	build/\n
lib/user.dart	class User {}\n
EOF
put "$DIR/flutter-feat" lib/user.dart 'class User {\n  final String name;\n  User(this.name);\n}\n'
put "$DIR/flutter-feat" lib/user.g.dart '// GENERATED CODE\n'
put "$DIR/flutter-feat" pubspec.lock 'packages: {}\n'
put "$DIR/flutter-feat" build/app.txt '무시됨\n'

cat > "$DIR/config.toml" <<EOF
[[repo]]
path = "$DIR/py"

[[repo]]
path = "$DIR/node-feat"

[[repo]]
path = "$DIR/flutter"

[[repo]]
path = "$DIR/없는-레포"
EOF
echo "fixtures: $DIR"
echo "run: WTBOARD_CONFIG=$DIR/config.toml pnpm tauri dev"
```

Run: `scripts/fixtures.sh "$SCRATCH/fixtures" && git -C "$SCRATCH/fixtures/node" worktree list`
Expected: `node`, `node-feat` 두 줄

- [ ] **Step 4: 실제 화면 확인**

"UI 확인 절차"대로 앱을 띄우고 찍는다. 확인할 것:
- "불러오는 중…" 다음에 첫 레포(`py`)의 main worktree 화면이 뜬다 (변경 0개).
- 오류 화면이 아니다 (`없는-레포`는 errors에만 들어간다).

- [ ] **Step 5: Commit + simplify**

```bash
git add crates/app/src/main.rs ui/src scripts/fixtures.sh
git commit -m "feat(app): Registry로 여러 레포를 열고 worktrees-updated를 보낸다"
```

그다음 `simplify` skill 실행.

---

### Task 13: sidebar와 worktree 선택

**Files:**
- Create: `ui/src/lib/selection.ts`, `ui/src/lib/selection.test.ts`, `ui/src/lib/time.ts`, `ui/src/lib/time.test.ts`, `ui/src/lib/useNow.ts`, `ui/src/components/Sidebar.tsx`, `ui/src/components/WorktreeView.tsx`
- Modify: `ui/src/store.ts`, `ui/src/App.tsx`

**Interfaces:**
- Consumes: Task 12의 UI 타입
- Produces:
  - `interface Selection { repoId: string; worktreeId: string }`
  - `resolveSelection(repos: RepoSummary[], selection: Selection | null): { repo: RepoSummary; worktree: WorktreeSummary } | null`
  - `worktreeLabel(w: WorktreeSummary): string`
  - `formatRelative(ms: number, now: number): string`
  - `useNow(intervalMs: number): number`
  - store: `selection`, `select(selection)` (`localStorage`에 저장)
  - `WorktreeView({ repo, worktree })`, `WorktreeError({ worktree })`

- [ ] **Step 1: 실패하는 테스트**

`ui/src/lib/selection.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { RepoSummary, WorktreeSummary } from '../api';
import { resolveSelection, worktreeLabel } from './selection';

const wt = (id: string, branch: string | null = id): WorktreeSummary => ({
  id,
  path: `/dev/${id}`,
  branch,
  base: 'origin/main',
  baseSource: 'origin',
  changedFiles: 0,
  profile: 'default',
  lastChangedAt: null,
  slowListMs: null,
  fsmonitor: false,
  error: null,
});
const repo = (id: string, ...worktrees: WorktreeSummary[]): RepoSummary => ({ id, name: id, profile: 'default', worktrees });
const repos = [repo('a', wt('a1'), wt('a2')), repo('b', wt('b1'))];

describe('resolveSelection', () => {
  it('저장된 선택이 목록에 있으면 그대로 쓴다', () => {
    expect(resolveSelection(repos, { repoId: 'b', worktreeId: 'b1' })?.worktree.id).toBe('b1');
  });

  it('선택한 worktree가 사라지면 같은 레포의 첫 worktree', () => {
    expect(resolveSelection(repos, { repoId: 'a', worktreeId: 'gone' })?.worktree.id).toBe('a1');
  });

  it('레포도 사라지면 첫 레포의 첫 worktree', () => {
    expect(resolveSelection(repos, { repoId: 'z', worktreeId: 'z1' })?.worktree.id).toBe('a1');
  });

  it('worktree가 없는 레포는 건너뛴다', () => {
    expect(resolveSelection([repo('empty'), repo('b', wt('b1'))], null)?.repo.id).toBe('b');
  });

  it('열 수 있는 것이 없으면 null', () => {
    expect(resolveSelection([], null)).toBeNull();
  });
});

describe('worktreeLabel', () => {
  it('브랜치 이름을 쓴다', () => {
    expect(worktreeLabel(wt('a1', 'feat/login'))).toBe('feat/login');
  });

  // Review Focus 5
  it('detached HEAD면 폴더 이름', () => {
    expect(worktreeLabel(wt('my-wt', null))).toBe('my-wt');
  });
});
```

`ui/src/lib/time.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { formatRelative } from './time';

const now = 1_800_000_000_000;

describe('formatRelative', () => {
  it('1분 안은 방금', () => {
    expect(formatRelative(now - 10_000, now)).toBe('방금');
    expect(formatRelative(now + 5_000, now)).toBe('방금'); // 시계 오차
  });

  it('분, 시간, 일 단위', () => {
    expect(formatRelative(now - 3 * 60_000, now)).toBe('3분 전');
    expect(formatRelative(now - 2 * 3_600_000, now)).toBe('2시간 전');
    expect(formatRelative(now - 3 * 86_400_000, now)).toBe('3일 전');
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `pnpm --dir ui test`
Expected: FAIL (모듈 없음)

- [ ] **Step 3: 구현**

`ui/src/lib/selection.ts`:

```ts
import type { RepoSummary, WorktreeSummary } from '../api';

export interface Selection {
  repoId: string;
  worktreeId: string;
}

export interface Current {
  repo: RepoSummary;
  worktree: WorktreeSummary;
}

/** 저장된 선택을 지금 목록에 맞춘다. worktree가 사라졌으면 같은 레포의 첫 worktree, 레포도 없으면 첫 레포의 첫 worktree */
export function resolveSelection(repos: RepoSummary[], selection: Selection | null): Current | null {
  const usable = repos.filter((r) => r.worktrees.length > 0);
  const repo = usable.find((r) => r.id === selection?.repoId);
  if (repo) {
    return { repo, worktree: repo.worktrees.find((w) => w.id === selection?.worktreeId) ?? repo.worktrees[0] };
  }
  const first = usable[0];
  return first ? { repo: first, worktree: first.worktrees[0] } : null;
}

/** sidebar 표시 이름. detached HEAD면 폴더 이름 */
export function worktreeLabel(w: WorktreeSummary): string {
  return w.branch ?? w.path.split('/').filter(Boolean).pop() ?? w.path;
}
```

`ui/src/lib/time.ts`:

```ts
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** "3분 전" 같은 상대 시각 */
export function formatRelative(ms: number, now: number): string {
  const diff = now - ms;
  if (diff < MINUTE) return '방금';
  if (diff < HOUR) return `${Math.floor(diff / MINUTE)}분 전`;
  if (diff < DAY) return `${Math.floor(diff / HOUR)}시간 전`;
  return `${Math.floor(diff / DAY)}일 전`;
}
```

`ui/src/lib/useNow.ts`:

```ts
import { useEffect, useState } from 'react';

/** 상대 시각 표시를 위해 주기적으로 바뀌는 현재 시각 */
export function useNow(intervalMs: number) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), intervalMs);
    return () => clearInterval(id);
  }, [intervalMs]);
  return now;
}
```

`ui/src/store.ts`:

```ts
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import type { Selection } from './lib/selection';

export type ViewType = 'split' | 'unified';

interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
  selection: Selection | null;
  select: (selection: Selection) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      viewType: 'split',
      setViewType: (viewType) => set({ viewType }),
      selection: null,
      select: (selection) => set({ selection }),
    }),
    { name: 'wtboard-ui', storage: createJSONStorage(() => localStorage) },
  ),
);
```

`ui/src/components/Sidebar.tsx`:

```tsx
import type { EntryError, Profile, RepoSummary } from '../api';
import { worktreeLabel, type Selection } from '../lib/selection';
import { formatRelative } from '../lib/time';
import { useNow } from '../lib/useNow';

const PROFILE_BADGE: Record<Profile, string | null> = { python: 'Py', node: 'Node', flutter: 'Flutter', default: null };

interface Props {
  repos: RepoSummary[];
  errors: EntryError[];
  selectedId: string | null;
  onSelect: (selection: Selection) => void;
}

export function Sidebar({ repos, errors, selectedId, onSelect }: Props) {
  const now = useNow(30_000);
  return (
    <nav className="w-56 shrink-0 overflow-y-auto border-r border-gray-200 bg-gray-50 py-2 text-sm">
      {repos.map((repo) => (
        <section key={repo.id} className="mb-2">
          <h2 className="flex items-center gap-2 px-3 py-1 font-semibold text-gray-700" title={repo.id}>
            <span className="flex-1 truncate">{repo.name}</span>
            {PROFILE_BADGE[repo.profile] && (
              <span className="rounded bg-gray-200 px-1 text-[10px] font-medium text-gray-600">
                {PROFILE_BADGE[repo.profile]}
              </span>
            )}
          </h2>
          <ul>
            {repo.worktrees.map((w) => (
              <li key={w.id}>
                <button
                  onClick={() => onSelect({ repoId: repo.id, worktreeId: w.id })}
                  title={w.path}
                  className={`flex w-full flex-col py-1 pr-3 pl-5 text-left ${
                    w.id === selectedId ? 'bg-blue-100' : 'hover:bg-gray-100'
                  }`}
                >
                  <span className="flex w-full items-center gap-2">
                    {w.error && <span className="text-amber-600">⚠</span>}
                    <span className="flex-1 truncate font-mono text-xs">{worktreeLabel(w)}</span>
                    {!w.error && <span className="text-xs text-gray-500 tabular-nums">{w.changedFiles}</span>}
                  </span>
                  {w.lastChangedAt !== null && (
                    <span className="text-[11px] text-gray-400">{formatRelative(w.lastChangedAt, now)}</span>
                  )}
                </button>
              </li>
            ))}
          </ul>
        </section>
      ))}
      {errors.length > 0 && (
        <ul className="mt-2 border-t border-gray-200 px-3 pt-2 text-xs text-amber-700">
          {errors.map((e) => (
            <li key={`${e.path}:${e.message}`} className="mb-1 break-all">
              ⚠ {e.path}: {e.message}
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}
```

`ui/src/components/WorktreeView.tsx` — `App.tsx`의 `WorktreeView`를 옮기고 props를 `repo`로 바꾼다. 바깥 div를 `h-screen` 대신 `min-h-0 flex-1`로:

```tsx
import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { listFiles, type RepoSummary, type WorktreeSummary } from '../api';
import { worktreeLabel } from '../lib/selection';
import { buildTree, flattenFiles } from '../lib/tree';
import { useUiStore } from '../store';
import { FileDiff } from './FileDiff';
import { FileTree } from './FileTree';

export function WorktreeView({ repo, worktree }: { repo: RepoSummary; worktree: WorktreeSummary }) {
  const { viewType, setViewType } = useUiStore();
  const files = useQuery({
    queryKey: ['files', worktree.id, 'branch'],
    queryFn: () => listFiles(worktree.id, 'branch'),
  });
  const tree = useMemo(() => buildTree(files.data ?? []), [files.data]);
  const ordered = useMemo(() => flattenFiles(tree), [tree]); // diff 순서를 트리와 맞춘다

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <header className="flex items-center gap-3 border-b border-gray-200 px-4 py-2 text-sm">
        <strong>{repo.name}</strong>
        <span className="font-mono">{worktreeLabel(worktree)}</span>
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
          <FileTree nodes={tree} />
        </aside>
        {/* 위쪽 padding을 두면 sticky 파일 헤더가 그만큼 아래에 멈추므로 spacer로 띄운다 */}
        <main className="flex-1 overflow-y-auto px-4 pb-4">
          <div className="h-4" />
          {files.error && <p className="text-red-600">{String(files.error)}</p>}
          {files.data?.length === 0 && <p className="text-gray-500">base 대비 변경 사항이 없습니다.</p>}
          {ordered.map((f) => (
            <FileDiff key={f.path} worktreeId={worktree.id} file={f} viewType={viewType} />
          ))}
        </main>
      </div>
    </div>
  );
}

export function WorktreeError({ worktree }: { worktree: WorktreeSummary }) {
  return (
    <div className="p-6">
      <h1 className="mb-1 font-bold">{worktreeLabel(worktree)}를 열 수 없습니다</h1>
      <p className="mb-3 font-mono text-xs text-gray-500">{worktree.path}</p>
      <pre className="text-sm whitespace-pre-wrap text-red-600">{worktree.error}</pre>
    </div>
  );
}
```

`ui/src/App.tsx` 전체:

```tsx
import { useQuery } from '@tanstack/react-query';
import { listRepos } from './api';
import { Sidebar } from './components/Sidebar';
import { WorktreeError, WorktreeView } from './components/WorktreeView';
import { resolveSelection } from './lib/selection';
import { useUiStore } from './store';
import { useWorktreeEvents } from './useWorktreeEvents';

export function App() {
  useWorktreeEvents();
  const selection = useUiStore((s) => s.selection);
  const select = useUiStore((s) => s.select);
  const repos = useQuery({
    queryKey: ['repos'],
    queryFn: listRepos,
    // 시작 직후 event를 놓쳐도 준비되면 화면이 뜨게 한다
    refetchInterval: (q) => (q.state.data?.ready === false ? 500 : false),
  });
  if (repos.error) return <ErrorScreen message={String(repos.error)} />;
  if (!repos.data?.ready) return <p className="p-4 text-gray-500">불러오는 중…</p>;

  const current = resolveSelection(repos.data.repos, selection);
  return (
    <div className="flex h-screen">
      <Sidebar
        repos={repos.data.repos}
        errors={repos.data.errors}
        selectedId={current?.worktree.id ?? null}
        onSelect={select}
      />
      {!current ? (
        <p className="p-4 text-gray-500">열 수 있는 worktree가 없습니다. 왼쪽 아래 오류를 확인하세요.</p>
      ) : current.worktree.error ? (
        <WorktreeError worktree={current.worktree} />
      ) : (
        <WorktreeView key={current.worktree.id} repo={current.repo} worktree={current.worktree} />
      )}
    </div>
  );
}

function ErrorScreen({ message }: { message: string }) {
  return (
    <div className="p-6">
      <h1 className="mb-2 font-bold">wtboard를 시작할 수 없습니다</h1>
      <pre className="text-sm whitespace-pre-wrap text-red-600">{message}</pre>
    </div>
  );
}
```

- [ ] **Step 4: 통과 확인**

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS, 빌드 성공

- [ ] **Step 5: 실제 화면 확인**

"UI 확인 절차"로 찍는다. 확인할 것:
1. sidebar에 `py`(Py), `node`(Node), `flutter`(Flutter) 레포와 각 `main` / `feat` worktree가 보인다. `node`는 `node-feat` 경로로 등록했지만 main까지 두 줄이 보인다.
2. 맨 아래에 `없는-레포: 경로 없음` 오류가 보인다.
3. `flutter`의 `feat`를 누르면(computer-use) 오른쪽이 그 worktree로 바뀐다.
4. 앱을 띄운 채로 `echo x >> "$SCRATCH/fixtures/py-feat/app/users.py"` → 1초 안에 `py/feat`의 변경 수와 "방금"이 보인다.
5. `git -C "$SCRATCH/fixtures/py" worktree add -q -b extra "$SCRATCH/fixtures/py-extra"` → 1초 안에 `extra`가 생긴다. `git -C "$SCRATCH/fixtures/py" worktree remove "$SCRATCH/fixtures/py-extra"` → 사라진다.
6. 앱을 껐다 켜면 마지막에 고른 worktree가 다시 선택돼 있다.

- [ ] **Step 6: Commit + simplify**

```bash
git add ui/src
git commit -m "feat(ui): 레포·worktree sidebar와 선택 유지"
```

그다음 `simplify` skill 실행.

---

### Task 14: 접힌 파일

**Files:**
- Modify: `ui/src/lib/tree.ts`, `ui/src/lib/tree.test.ts`, `ui/src/components/FileTree.tsx`, `ui/src/components/FileDiff.tsx`, `ui/src/components/WorktreeView.tsx`

**Interfaces:**
- Produces: `splitCollapsed(files: ChangedFile[]): { visible: ChangedFile[]; collapsed: ChangedFile[] }`, `CollapsedGroup({ count, nodes })`

- [ ] **Step 1: 실패하는 테스트**

`ui/src/lib/tree.test.ts` 끝에 추가 (import에 `splitCollapsed` 추가):

```ts
describe('splitCollapsed', () => {
  it('접힌 파일을 따로 모으고 나머지 순서는 그대로 둔다', () => {
    const gen = { ...f('lib/user.g.dart'), collapsed: true };
    const lock = { ...f('pubspec.lock'), collapsed: true };
    const { visible, collapsed } = splitCollapsed([f('lib/a.dart'), gen, f('lib/b.dart'), lock]);
    expect(visible.map((x) => x.path)).toEqual(['lib/a.dart', 'lib/b.dart']);
    expect(collapsed.map((x) => x.path)).toEqual(['lib/user.g.dart', 'pubspec.lock']);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `pnpm --dir ui test`
Expected: FAIL (`splitCollapsed` 없음)

- [ ] **Step 3: 구현**

`ui/src/lib/tree.ts` 끝에 추가:

```ts
/** 접힌 파일(생성·lock 파일)과 나머지로 나눈다 */
export function splitCollapsed(files: ChangedFile[]): { visible: ChangedFile[]; collapsed: ChangedFile[] } {
  return { visible: files.filter((f) => !f.collapsed), collapsed: files.filter((f) => f.collapsed) };
}
```

`ui/src/components/FileTree.tsx` 끝에 추가:

```tsx
/** 생성·lock 파일 묶음. 기본으로 닫혀 있다 */
export function CollapsedGroup({ count, nodes }: { count: number; nodes: TreeNode[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="mt-2 border-t border-gray-200 pt-1">
      <button
        className="flex w-full items-center gap-1 px-3 py-0.5 text-left text-sm text-gray-500 hover:bg-gray-100"
        onClick={() => setOpen(!open)}
      >
        <span className="w-3 text-xs">{open ? '▾' : '▸'}</span>
        <span>접힌 파일 ({count})</span>
      </button>
      {open && <FileTree nodes={nodes} />}
    </div>
  );
}
```

`ui/src/components/FileDiff.tsx`:

1. `import { useMemo, useState } from 'react';`
2. `FileDiff` 본문:

```tsx
export function FileDiff({ worktreeId, file, viewType }: Props) {
  const [ref, inView] = useInView<HTMLElement>();
  // 접힌 파일은 펼치기 전까지 patch를 계산하지 않는다
  const [open, setOpen] = useState(!file.collapsed);
  const patch = useQuery({
    queryKey: ['patch', worktreeId, file.path, 'branch'],
    queryFn: () => getPatch(worktreeId, 'branch', file.path),
    enabled: inView && open, // 화면에 보이고 펼쳐져 있을 때만 계산
  });

  return (
    <section ref={ref} id={fileAnchorId(file.path)} className="mb-4 rounded border border-gray-200">
      <header
        className={`sticky top-0 z-10 flex items-center gap-3 border-b border-gray-200 bg-gray-50 px-3 py-2 text-sm ${
          file.collapsed ? 'cursor-pointer' : ''
        }`}
        onClick={file.collapsed ? () => setOpen(!open) : undefined}
      >
        {file.collapsed && <span className="w-3 text-xs text-gray-500">{open ? '▾' : '▸'}</span>}
        <span className="flex-1 truncate font-mono">{file.path}</span>
        {file.collapsed && !open && <span className="text-xs text-gray-400">생성 파일</span>}
        <LineCounts file={file} />
      </header>
      {!open ? null : patch.error ? (
        <p className="p-3 text-sm text-red-600">{String(patch.error)}</p>
      ) : patch.data ? (
        <PatchView body={patch.data.body} viewType={viewType} path={file.path} />
      ) : (
        <div className="h-20" />
      )}
    </section>
  );
}
```

`ui/src/components/WorktreeView.tsx`:

1. import: `import { buildTree, flattenFiles, splitCollapsed } from '../lib/tree';`, `import { CollapsedGroup, FileTree } from './FileTree';`
2. 트리 계산을 바꾼다:

```tsx
  const { visible, collapsed } = useMemo(() => splitCollapsed(files.data ?? []), [files.data]);
  const tree = useMemo(() => buildTree(visible), [visible]);
  const collapsedTree = useMemo(() => buildTree(collapsed), [collapsed]);
  // diff 순서를 트리와 맞춘다. 접힌 파일은 맨 뒤
  const ordered = useMemo(() => [...flattenFiles(tree), ...flattenFiles(collapsedTree)], [tree, collapsedTree]);
```

3. aside 안 `<FileTree nodes={tree} />` 다음에:

```tsx
          {collapsed.length > 0 && <CollapsedGroup count={collapsed.length} nodes={collapsedTree} />}
```

- [ ] **Step 4: 통과 확인**

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS, 빌드 성공

- [ ] **Step 5: 실제 화면 확인**

"UI 확인 절차"로 찍는다. 확인할 것:
1. `flutter/feat`: 트리에 `lib/user.dart`만 보이고 아래에 `▸ 접힌 파일 (2)`. 펼치면 `lib/user.g.dart`, `pubspec.lock`. `build/`는 어디에도 없다.
2. diff 목록 맨 뒤에 `▸ lib/user.g.dart  생성 파일` header만 있다. 누르면 diff가 펼쳐진다.
3. `node/feat`: `package-lock.json`, `next-env.d.ts`, `generated/api.ts`(linguist-generated)가 접혀 있다.
4. `py/feat`: `uv.lock`만 접혀 있고 `venv/`는 목록에 없다.

- [ ] **Step 6: Commit + simplify**

```bash
git add ui/src
git commit -m "feat(ui): 생성·lock 파일을 접어서 보여 준다"
```

그다음 `simplify` skill 실행.

---

### Task 15: header의 base 출처와 fsmonitor banner

**Files:**
- Create: `ui/src/lib/banner.ts`, `ui/src/lib/banner.test.ts`, `ui/src/components/FsmonitorBanner.tsx`
- Modify: `ui/src/store.ts`, `ui/src/components/WorktreeView.tsx`

**Interfaces:**
- Produces: `showFsmonitorBanner(w: WorktreeSummary, repoId: string, dismissed: string[]): boolean`, `FSMONITOR_COMMANDS: string[]`, store `dismissedBanners`, `dismissBanner(repoId)`

- [ ] **Step 1: 실패하는 테스트**

`ui/src/lib/banner.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { WorktreeSummary } from '../api';
import { showFsmonitorBanner } from './banner';

const w = (slowListMs: number | null, fsmonitor: boolean): WorktreeSummary => ({
  id: 'w',
  path: '/w',
  branch: 'main',
  base: 'origin/main',
  baseSource: 'origin',
  changedFiles: 0,
  profile: 'default',
  lastChangedAt: null,
  slowListMs,
  fsmonitor,
  error: null,
});

describe('showFsmonitorBanner', () => {
  it('느리고 fsmonitor가 꺼져 있으면 보인다', () => {
    expect(showFsmonitorBanner(w(820, false), 'r', [])).toBe(true);
  });

  it('빠르거나 fsmonitor가 켜져 있으면 안 보인다', () => {
    expect(showFsmonitorBanner(w(null, false), 'r', [])).toBe(false);
    expect(showFsmonitorBanner(w(820, true), 'r', [])).toBe(false);
  });

  it('그 레포에서 닫았으면 안 보인다', () => {
    expect(showFsmonitorBanner(w(820, false), 'r', ['r'])).toBe(false);
    expect(showFsmonitorBanner(w(820, false), 'other', ['r'])).toBe(true);
  });
});
```

- [ ] **Step 2: 실패 확인**

Run: `pnpm --dir ui test`
Expected: FAIL (`banner` 모듈 없음)

- [ ] **Step 3: 구현**

`ui/src/lib/banner.ts`:

```ts
import type { WorktreeSummary } from '../api';

/** 앱은 레포에 쓰지 않으므로 사용자가 직접 실행할 명령을 보여 준다 */
export const FSMONITOR_COMMANDS = ['git config core.fsmonitor true', 'git config core.untrackedCache true'];

/** 목록 계산이 느리고, fsmonitor가 꺼져 있고, 이 레포의 banner를 닫지 않았을 때 */
export function showFsmonitorBanner(w: WorktreeSummary, repoId: string, dismissed: string[]): boolean {
  return w.slowListMs !== null && !w.fsmonitor && !dismissed.includes(repoId);
}
```

`ui/src/store.ts` — 상태 두 개를 추가하고, 저장은 `viewType`과 `selection`만 한다 (banner 닫기는 앱 실행 중에만 기억):

```ts
interface UiState {
  viewType: ViewType;
  setViewType: (viewType: ViewType) => void;
  selection: Selection | null;
  select: (selection: Selection) => void;
  /** fsmonitor banner를 닫은 레포 id */
  dismissedBanners: string[];
  dismissBanner: (repoId: string) => void;
}

export const useUiStore = create<UiState>()(
  persist(
    (set) => ({
      viewType: 'split',
      setViewType: (viewType) => set({ viewType }),
      selection: null,
      select: (selection) => set({ selection }),
      dismissedBanners: [],
      dismissBanner: (repoId) => set((s) => ({ dismissedBanners: [...s.dismissedBanners, repoId] })),
    }),
    {
      name: 'wtboard-ui',
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({ viewType: s.viewType, selection: s.selection }),
    },
  ),
);
```

`ui/src/components/FsmonitorBanner.tsx`:

```tsx
import type { WorktreeSummary } from '../api';
import { FSMONITOR_COMMANDS, showFsmonitorBanner } from '../lib/banner';
import { useUiStore } from '../store';

export function FsmonitorBanner({ repoId, worktree }: { repoId: string; worktree: WorktreeSummary }) {
  const dismissed = useUiStore((s) => s.dismissedBanners);
  const dismiss = useUiStore((s) => s.dismissBanner);
  if (!showFsmonitorBanner(worktree, repoId, dismissed)) return null;
  return (
    <div className="flex items-start gap-3 border-b border-amber-200 bg-amber-50 px-4 py-2 text-sm">
      <div className="flex-1">
        <p className="mb-1">
          이 레포는 변경 목록 계산에 {worktree.slowListMs}ms 걸립니다. 레포 폴더에서 다음을 실행하면 빨라집니다.
        </p>
        {FSMONITOR_COMMANDS.map((cmd) => (
          <div key={cmd} className="flex items-center gap-2">
            <code className="font-mono text-xs">{cmd}</code>
            <button className="text-xs text-blue-600 hover:underline" onClick={() => navigator.clipboard.writeText(cmd)}>
              복사
            </button>
          </div>
        ))}
      </div>
      <button className="text-xs text-gray-500 hover:underline" onClick={() => dismiss(repoId)}>
        닫기
      </button>
    </div>
  );
}
```

`ui/src/components/WorktreeView.tsx`:

1. `import { FsmonitorBanner } from './FsmonitorBanner';`
2. header의 base 표시:

```tsx
        <span className="text-gray-500">
          ← {worktree.base}
          {worktree.baseSource === 'pr' && ' (PR)'}
        </span>
```

3. `</header>` 바로 다음 줄에 `<FsmonitorBanner repoId={repo.id} worktree={worktree} />`

- [ ] **Step 4: 통과 확인**

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS, 빌드 성공

- [ ] **Step 5: 실제 화면 확인**

`WTBOARD_SLOW_LIST_MS=0`을 붙여 "UI 확인 절차"로 띄운다 (모든 worktree가 느린 것으로 보인다).
1. banner와 명령 두 줄, 복사·닫기 버튼이 보인다.
2. 복사를 누르고 `pbpaste`로 명령이 들어왔는지 확인한다.
3. 닫기를 누르면 그 레포의 다른 worktree로 옮겨도 banner가 없고, 다른 레포에는 있다.
4. env 없이 다시 띄우면 banner가 없다.

PR base `(PR)` 표시는 fixture에 GitHub PR이 없어 Task 16에서 실제 레포로 확인한다.

- [ ] **Step 6: Commit + simplify**

```bash
git add ui/src
git commit -m "feat(ui): PR base 표시와 느린 레포의 fsmonitor 안내"
```

그다음 `simplify` skill 실행.

---

### Task 16: 실제 레포로 확인 + 문서 갱신

**Files:**
- Modify: `docs/progress.md`, `CLAUDE.md`

- [ ] **Step 1: 전체 테스트**

Run: `source ~/.cargo/env && cargo test && pnpm --dir ui test && pnpm --dir ui build`
Expected: 전체 PASS

- [ ] **Step 2: 실제 config로 확인 (읽기만)**

`WTBOARD_CONFIG` 없이 `pnpm tauri dev`로 띄우고 찍는다. 사용자 레포에는 아무것도 쓰지 않는다.
1. sidebar에 `omofictions-worker`(Py), `omofictions-front`(Node), `omofictions-admin`(Node), `Omofictions-App`(Flutter)가 보인다.
2. `omofictions-back-1371`은 맨 아래 `경로 없음` 오류로 보인다 (config 수정은 사용자에게 안내).
3. `omofictions-admin`의 `admin/fix/960-back-1574-ui-ux-review`: PR이 있으면 header에 `← origin/<base> (PR)`이 보인다. 없으면 `gh pr view`를 직접 실행해 결과와 화면이 일치하는지 확인한다.
4. `Omofictions-App`의 `main`이 열린다 (변경 0개면 "base 대비 변경 사항이 없습니다").
5. `git -C <각 레포> status --porcelain` 결과가 앱 실행 전과 같다.

- [ ] **Step 3: 문서 갱신**

`docs/progress.md`:
- 단계 표: 2단계 `완료 (날짜)`, plan 경로. 3단계를 `다음`으로.
- "2단계 결과" 절: fixture와 실제 레포로 확인한 것.
- "2단계에서 정한 것 (spec과 다른 점)": 실행 중에 바뀐 결정이 있으면 적는다.
- "2단계로 넘어간 것" 절을 지우고, 남은 것은 "3단계로 넘어간 것"으로 옮긴다.
- "미룬 Minor"에서 처리한 두 개(`fallback_poll_secs = 0` busy loop, `spawn_blocking`)를 지운다.

`CLAUDE.md` 구조 절:
- `crates/core`: `repo.rs`(레포 하나), `registry.rs`(config 전체), `gh.rs`, `profile.rs` 추가.
- `crates/app`: event에 `worktrees-updated` 추가.
- 명령 절에 `scripts/fixtures.sh <dir>` + `WTBOARD_CONFIG` 사용법 한 줄.

- [ ] **Step 4: Commit + simplify**

```bash
git add docs/progress.md CLAUDE.md
git commit -m "docs: 2단계 완료, 진행 상황과 프로젝트 안내 갱신"
```

그다음 `simplify` skill 실행 (문서만 바뀌었으면 건너뛴다).
