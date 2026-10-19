# 4단계: plugin hooks + 이번 세션 / 마지막 응답 탭 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Claude Code hook이 세션·turn별로 건드린 파일과 원본을 기록하고, 앱이 "이번 세션"과 "마지막 응답" 탭에서 그 변경만 diff로 보여 준다.

**Architecture:** 새 binary crate `crates/hook`(`wtboard-hook`)이 hook stdin JSON을 받아 `<data dir>/sessions/<id>/`에 기록만 한다. 형식은 `core::session::format`이 쓰기·읽기 모두 맡는다. 앱은 `core::session::{list, view}`로 세션 목록과 변경 목록·patch를 계산하고, `sessions/` 감시로 `session-updated` event를 보낸다. UI는 view 탭 3개와 세션 선택을 더한다.

**Tech Stack:** Rust (serde_json, notify), Tauri 2, React + TanStack Query + zustand, vitest. Claude Code plugin (local marketplace).

**Spec:** `docs/superpowers/specs/2026-10-19-stage4-sessions-design.md`

## Global Constraints

- 앱과 hook은 레포에 아무것도 쓰지 않는다. 쓰기는 `<data dir>/sessions/` (그리고 3단계 `state.sqlite`)뿐이다. 모든 git 호출은 `core::git::Git`(`GIT_OPTIONAL_LOCKS=0`).
- data dir: `WTBOARD_CONFIG`가 있으면 그 파일의 폴더, 없으면 `~/.wtboard`. 앱과 hook이 같은 함수(`core::store::data_dir()`)를 쓴다.
- hook은 어떤 입력·오류에도 exit 0. 오류는 stderr 한 줄. stdout에는 아무것도 쓰지 않는다 (Claude Code가 JSON으로 해석한다).
- `session_id`는 `[A-Za-z0-9_-]+`만, 경로는 절대 경로이고 `..` component 없음. 아니면 기록하지 않는다.
- 경로는 NFC 절대 경로로 저장한다 (`core::paths::nfc`). 화면에는 worktree 상대 경로.
- 1MB(`MAX_DIFF_BYTES`) 넘는 파일은 내용 복사 대신 `too_large`.
- `meta.json`은 turn 경계 hook(session-start, prompt-submit, turn-end, session-end)만 쓰고 tmp+rename. 마지막 활동 시각 = `events.jsonl` mtime.
- active = `ended_at` 없음 && 마지막 활동 30분 이내.
- view 이름: `branch` | `session` | `turn`. 사용자 문구: `브랜치 전체`, `이번 세션`, `마지막 응답`, `진행 중`, `원본 불확실`, `이 worktree의 Claude Code 세션 기록이 없습니다`.
- 테스트를 먼저 쓴다. 실제 git, 실제 파일, 실제 FSEvents. `source ~/.cargo/env` 먼저.

## Review Focus

1. stdin의 `session_id`나 `file_path`에 `../`가 섞여 세션 폴더 밖에 쓰는 경우 → 아무것도 쓰지 않고 exit 0 (Task 2 test `rejects_traversal`).
2. data dir에 쓸 수 없거나 `HOME`이 없는 환경 → 빠르게 exit 0, stdout 비어 있음 (Task 2 test `unwritable_data_dir_exits_zero`).
3. 병렬 tool 호출로 `post-edit` 50개가 동시에 append → `events.jsonl`의 모든 줄이 JSON으로 읽힌다 (Task 2 test `parallel_appends_stay_line_atomic`).
4. Esc로 중단돼 Stop 없이 다음 prompt가 옴 → 이전 turn이 닫히고 스냅샷이 남는다 (Task 2 test `prompt_without_stop_closes_previous_turn`).
5. 한글(NFD로 들어온) 파일 이름 → baseline과 화면 경로가 worktree 상대 NFC 경로로 맞는다 (Task 4 test `nfd_path_matches_worktree_path`).

---

## File Structure

| 파일 | 할 일 |
|---|---|
| `crates/core/src/store/mod.rs`, `store/config.rs` | `data_dir()`, `[sessions] keep_days` |
| `crates/core/src/session/mod.rs` (신규) | module 묶음, `is_valid_session_id`, `valid_abs_path` |
| `crates/core/src/session/format.rs` (신규) | 세션 폴더 형식: `SessionDir`, `Meta`, `Event`, `Baseline`, 스냅샷 |
| `crates/core/src/session/list.rs` (신규) | 세션 목록, worktree 귀속, active, 정리 |
| `crates/core/src/session/view.rs` (신규) | session/turn 변경 목록과 patch |
| `crates/core/src/git/patch.rs` | `no_index_patch` (두 파일 비교) |
| `crates/core/src/watch/sessions_watcher.rs` (신규) | `sessions/` 감시 |
| `crates/hook/` (신규) | `wtboard-hook` binary: `main.rs`(입력·dispatch·exit 0), `edit.rs`, `bash.rs`, `turn.rs` |
| `crates/app/src/main.rs` | `list_sessions`, `list_files`/`get_patch`의 `session_id`, `session-updated`, 정리, `activeSession` |
| `plugin/`, `scripts/install-hook.sh` (신규) | marketplace, plugin, 설치 script |
| `ui/src/api.ts`, `ui/src/lib/sessions.ts` (신규), `ui/src/store.ts`, `ui/src/useWorktreeEvents.ts`, `ui/src/lib/events.ts` | 타입, 순수 함수, 탭·세션 기억, invalidate |
| `ui/src/components/*.tsx` | 탭, 세션 선택, 원본 불확실, sidebar ● |

---

### Task 1: data dir과 세션 폴더 형식 (`core::session::format`)

**Files:**
- Modify: `crates/core/src/store/mod.rs`, `crates/core/src/store/config.rs`, `crates/core/src/lib.rs`, `crates/app/src/main.rs` (`state.sqlite` 경로를 `data_dir()`로)
- Create: `crates/core/src/session/mod.rs`, `crates/core/src/session/format.rs`, `crates/core/tests/session_format.rs`

**Interfaces:**
- Produces:
  - `core::store::data_dir() -> PathBuf` — `Config::default_path()`의 부모 폴더
  - `Config.sessions: SessionsConfig { keep_days: u64 }` (`#[serde(default)]`, 기본 7)
  - `core::session::is_valid_session_id(&str) -> bool`, `core::session::valid_abs_path(&Path) -> Option<PathBuf>` (절대 경로, `..` 없음, NFC로 바꿔 돌려줌)
  - `pub struct Meta { started_at: u64, project_dir: PathBuf, start_head: Option<String>, turn: u32, turn_open: bool, ended_at: Option<u64> }` (serde, 시각은 Unix ms)
  - `pub enum EventKind { Edit, Bash, TurnStart, TurnEnd }` (serde snake_case)
  - `pub struct Event { t: u64, turn: u32, kind: EventKind, tool: Option<String>, paths: Vec<PathBuf>, agent_id: Option<String> }`
  - `pub enum BaselineKind { File, New, TooLarge, Uncertain }` (serde snake_case)
  - `pub struct Baseline { path: PathBuf, kind: BaselineKind, head: Option<String> }`
  - `pub enum BaselineContent<'a> { Bytes(&'a [u8]), CopyOf(&'a Path), New, Uncertain { head: Option<String>, bytes: Option<&'a [u8]> } }` — `Uncertain`의 `bytes`는 세션 시작 HEAD의 내용 (hook이 `git show`로 읽어 넘긴다. 없으면 빈 파일로 본다)
  - `pub enum Snapshot { Content(PathBuf), Deleted, TooLarge }` — `Content`는 세션 폴더 안 파일 경로
  - `pub struct SessionDir` with:
    - `open(data_dir: &Path, session_id: &str) -> Option<SessionDir>` (id 검증 실패면 `None`, 폴더는 만들지 않음)
    - `id(&self) -> &str`, `root(&self) -> &Path`
    - `read_meta(&self) -> Option<Meta>`, `write_meta(&self, &Meta) -> io::Result<()>` (폴더 생성 + tmp + rename)
    - `append_event(&self, &Event) -> io::Result<()>` (줄 하나를 `O_APPEND` write 한 번)
    - `events(&self) -> Vec<Event>` (못 읽는 줄은 건너뜀)
    - `last_activity(&self) -> Option<u64>` (`events.jsonl` mtime, 없으면 `meta.json` mtime)
    - `ensure_baseline(&self, path: &Path, content: BaselineContent) -> io::Result<bool>` — 이미 있으면 `false`. 내용 파일은 `create_new`로 만들어 경쟁에서 하나만 이긴다. 이긴 쪽만 `baseline.jsonl`에 한 줄 append. `CopyOf`/`Bytes`가 1MB를 넘으면 `TooLarge`로 기록
    - `baselines(&self) -> HashMap<PathBuf, Baseline>`, `baseline_file(&self, path: &Path) -> PathBuf`
    - `snapshot_turn(&self, turn: u32, paths: &[PathBuf]) -> io::Result<()>` — 현재 내용을 `turns/<n>/<abs>`에 복사, 없는 파일은 `turns/<n>/deleted.txt`에, 1MB 초과는 `turns/<n>/too_large.txt`에
    - `turn_snapshot(&self, turn: u32, path: &Path) -> Option<Snapshot>`

- [ ] **Step 1: Write the failing tests** (`tests/session_format.rs`, tempdir를 data dir로)

```rust
#[test] fn rejects_bad_session_ids()       // "../x", "a/b", "", "a b" → open() == None; "abc-123_X" → Some
#[test] fn valid_abs_path_rules()          // "rel/x" → None, "/a/../b" → None, "/a/b" → Some, NFD "/a/한" → NFC
#[test] fn meta_roundtrip_and_atomic()     // write_meta → read_meta 같음, 폴더 안에 tmp 파일이 남지 않음
#[test] fn events_skip_broken_lines()      // append 2개 + 깨진 줄 직접 추가 → events().len() == 2
#[test] fn baseline_first_writer_wins()    // ensure_baseline(CopyOf v1) == true, (Bytes v2) == false, 내용은 v1
#[test] fn baseline_race_single_winner()   // thread 10개가 같은 path로 ensure_baseline → true 1개, baselines() 1개
#[test] fn baseline_kinds()                // New, Uncertain{head}, 1MB+1 Bytes → TooLarge
#[test] fn turn_snapshot_content_deleted_too_large() // 있는 파일, 없는 파일, 1MB 초과 각각 Snapshot 변종
#[test] fn last_activity_is_events_mtime()
#[test] fn config_sessions_keep_days_default() // [sessions] 없으면 7, keep_days = 3이면 3
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-core --test session_format` → 컴파일 실패
- [ ] **Step 3: Implement** — `store::data_dir`, config, `session::{mod, format}`. app의 `state.sqlite` 경로를 `data_dir().join("state.sqlite")`로 바꾼다 (동작 같음).
- [ ] **Step 4: Run** — `cargo test` 전부 PASS
- [ ] **Step 5: Commit** — `feat(core): 세션 기록 폴더 형식과 data dir`

---

### Task 2: `wtboard-hook` — 입력, edit, turn

**Files:**
- Modify: `Cargo.toml` (workspace members에 `crates/hook`)
- Create: `crates/hook/Cargo.toml` (bin `wtboard-hook`, deps: `wtboard-core`, `serde`, `serde_json`), `crates/hook/src/main.rs`, `crates/hook/src/edit.rs`, `crates/hook/src/turn.rs`, `crates/hook/tests/hook.rs`, `crates/hook/tests/common/mod.rs`

**Interfaces:**
- Consumes: Task 1 전부
- Produces:
  - CLI: `wtboard-hook <subcommand>`, subcommand = `session-start | prompt-submit | pre-edit | post-edit | pre-bash | post-bash | turn-end | session-end`. 모르는 subcommand도 exit 0. 이 task는 `pre-bash`/`post-bash`를 빈 동작으로 둔다 (Task 3)
  - 입력 struct `HookInput { session_id: String, cwd: PathBuf, tool_name: Option<String>, tool_input: Option<serde_json::Value>, tool_use_id: Option<String>, agent_id: Option<String>, source: Option<String> }` (모르는 필드 무시)
  - `main`: stdin 전체 읽기 → 파싱 → `SessionDir::open(data_dir(), id)` → dispatch. 각 subcommand 함수는 `Result`를 돌려주고, `main`이 `Err`를 stderr 한 줄로 찍고 exit 0. panic도 `std::panic::catch_unwind`로 잡아 exit 0
  - 동작은 spec 3절 표 그대로:
    - `session-start`: meta 없으면 생성 (`project_dir` = env `CLAUDE_PROJECT_DIR` 또는 `cwd`, `start_head` = `git rev-parse HEAD` in project_dir, 실패하면 `None`, `turn: 0`). 있으면 `ended_at = None`
    - `prompt-submit`: 열린 turn 닫기(= turn-end 동작) → `turn += 1`, `turn_open = true`, `TurnStart` event
    - `pre-edit`: `tool_input.file_path` 또는 `notebook_path` → `valid_abs_path` → 파일 있으면 `CopyOf`, 없으면 `New`
    - `post-edit`: `Edit` event (`turn` = meta.turn, 없으면 0)
    - `turn-end` / `session-end`: 열린 turn이면 그 turn event 경로로 `snapshot_turn`, `TurnEnd` event, `turn_open = false`. `session-end`는 `ended_at` 기록
  - test helper `run_hook(data_dir: &Path, sub: &str, input: &serde_json::Value) -> std::process::Output` (`env!("CARGO_BIN_EXE_wtboard-hook")`, `WTBOARD_CONFIG=<data_dir>/config.toml`)

- [ ] **Step 1: Write the failing tests** (`crates/hook/tests/hook.rs`)

```rust
#[test] fn any_input_exits_zero_with_empty_stdout() // 빈 stdin, "not json", 모르는 subcommand, session_id 없음 → status 0, stdout 비어 있음
#[test] fn unwritable_data_dir_exits_zero()        // data dir를 일반 파일로 → status 0, 1초 안에 끝남
#[test] fn rejects_traversal()                     // session_id "../evil", file_path "/tmp/../etc/x" → data dir 밖에 새 파일 없음, sessions/ 아래에도 기록 없음
#[test] fn session_start_creates_meta_and_resume_keeps_it() // start → meta.turn 0, start_head == 레포 HEAD; ended 후 source "resume" → ended_at None, turn 그대로
#[test] fn pre_edit_baselines_existing_and_new()   // 있는 파일 → File 내용 같음, 없는 파일 → New, 두 번째 pre-edit은 baseline 그대로
#[test] fn parallel_pre_edit_one_baseline()        // 같은 파일 pre-edit 프로세스 10개 동시 → baselines() 1개
#[test] fn parallel_appends_stay_line_atomic()     // post-edit 프로세스 50개 동시 → events() 50개
#[test] fn turn_lifecycle_snapshots()              // prompt-submit → pre/post-edit a.py(v2로 수정) → turn-end → turns/1/a.py == v2, TurnEnd event, meta.turn_open false
#[test] fn prompt_without_stop_closes_previous_turn() // prompt → edit → prompt → turn 1 snapshot 있음, meta.turn 2, turn_open true
#[test] fn events_before_first_prompt_are_turn_zero()
#[test] fn notebook_path_is_accepted()             // tool_input {notebook_path} → baseline 생김
#[test] fn hook_never_touches_git_index()          // TestRepo에서 모든 subcommand 실행 전후 .git/index 바이트·mtime 같음
```

(`TestRepo`는 `crates/core/tests/common/mod.rs`와 같은 것을 `crates/hook/tests/common/mod.rs`로 복사하지 말고 `#[path = "../../core/tests/common/mod.rs"] mod common;`으로 공유한다.)

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-hook` → 컴파일 실패
- [ ] **Step 3: Implement** — `main.rs`, `edit.rs`, `turn.rs`
- [ ] **Step 4: Run** — `cargo test -p wtboard-hook` PASS, 이어서 `cargo test` 전부 PASS
- [ ] **Step 5: 속도 확인** — `cargo build --release -p wtboard-hook` 후 post-edit 100회 평균 시간을 report에 적는다 (목표 50ms 이하)
- [ ] **Step 6: Commit** — `feat(hook): wtboard-hook 입력 처리와 edit·turn 기록`

---

### Task 3: `wtboard-hook` — Bash 전후 비교

**Files:**
- Create: `crates/hook/src/bash.rs`
- Modify: `crates/hook/src/main.rs` (dispatch), `crates/hook/tests/hook.rs`

**Interfaces:**
- Consumes: Task 1·2
- Produces:
  - `pre-bash`: `cwd`에서 `git rev-parse --show-toplevel HEAD`. 레포가 아니면 아무것도 안 함. `git status --porcelain=v2 -z --untracked-files=all` → 경로마다 (status 문자열, mtime ns, size). `bash/<tool_use_id>.json`에 `{root, head, entries}` 저장 (`tool_use_id` 없으면 기록 안 함)
  - `post-bash`: 같은 파일을 읽고 지운다. 지금 status와 비교해 spec 3절 "post-bash" 표대로 바뀐 경로를 찾는다. 각 경로 `ensure_baseline`: 깨끗했다가 바뀐 tracked → `Bytes(git show <pre head>:<rel>)`, 새 untracked → `New`, 그 외 → `Uncertain { head: meta.start_head, bytes: git show <start_head>:<rel> (실패하면 None) }`. 바뀐 경로가 있으면 `Bash` event
  - status 파싱은 `--porcelain=v2 -z`의 `1`/`2`/`u`/`?` 줄. `2`(rename)는 `--no-renames`가 없으므로 새 경로만 쓴다

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn bash_edit_of_clean_tracked_file()      // pre-bash → (sed 대신 fs::write) a.py 수정 → post-bash → baseline File == HEAD 내용, Bash event paths [a.py]
#[test] fn bash_new_untracked_file()              // 새 파일 생성 → New
#[test] fn bash_reedit_of_dirty_file_is_uncertain() // pre 전에 이미 수정된 b.py를 다시 수정 (mtime 바뀜) → Uncertain{head: start_head}, baseline 내용 == start_head의 b.py
#[test] fn bash_revert_is_uncertain()             // 수정된 b.py를 git checkout -- b.py → Uncertain, event에 b.py
#[test] fn bash_commit_uses_pre_head()            // 깨끗한 c.py 수정 + git commit (HEAD 바뀜) → baseline == pre HEAD의 c.py
#[test] fn bash_already_baselined_keeps_baseline() // pre-edit으로 baseline 있는 파일을 Bash가 고침 → baseline 그대로
#[test] fn bash_outside_repo_does_nothing()       // cwd가 레포 아님 → bash/ 폴더도 event도 없음, exit 0
#[test] fn bash_without_tool_use_id_does_nothing()
#[test] fn bash_snapshot_file_removed_after_post()
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-hook bash_` → FAIL
- [ ] **Step 3: Implement `bash.rs`**
- [ ] **Step 4: Run** — `cargo test -p wtboard-hook` PASS. 실제 `omofictions-back` 크기 레포가 아니라도 report에 fixture에서 pre-bash 시간 기록
- [ ] **Step 5: Commit** — `feat(hook): Bash 전후 git status 비교로 바뀐 파일 기록`

---

### Task 4: 세션 변경 계산 (`core::session::{list, view}`)

**Files:**
- Create: `crates/core/src/session/list.rs`, `crates/core/src/session/view.rs`, `crates/core/tests/session_view.rs`
- Modify: `crates/core/src/git/patch.rs` (`no_index_patch`), `crates/core/src/worktree.rs` (`FileEntry.uncertain`, `Worktree::git()`), `crates/core/src/session/mod.rs`

**Interfaces:**
- Consumes: Task 1 format (테스트는 hook binary가 아니라 `SessionDir` 쓰기 API로 세션을 만든다)
- Produces:
  - `FileEntry`에 `pub uncertain: bool` 추가 (branch view는 항상 false). `Worktree::git(&self) -> &Git`
  - `git::patch::no_index_patch(git: &Git, rel: &str, before: Option<&Path>, after: Option<&Path>) -> Result<FilePatch, GitError>` — `None`은 `/dev/null`. `DIFF_FLAGS` + `--no-index`, exit 0/1 허용. 결과 diff text의 header 경로를 `a/<rel>`, `b/<rel>`로 바꾼다. 1MB 초과·바이너리 판정과 hash는 `file_patch`와 같은 규칙(`PatchBody`)
  - `list.rs`:
    - `pub struct SessionSummary { id, started_at: u64, last_event_at: u64, active: bool, turn: u32, turn_open: bool, touched_files: usize }` (serde camelCase)
    - `sessions_for(data_dir: &Path, worktree_root: &Path, now: u64) -> Vec<SessionSummary>` — 최근 활동 순. 귀속: event 경로가 root 아래이거나 `project_dir`가 root 아래
    - `worktrees_of(dir: &SessionDir, roots: &[PathBuf]) -> Vec<PathBuf>`
    - `active_roots(data_dir: &Path, roots: &[PathBuf], now: u64) -> HashSet<PathBuf>` — meta와 mtime만 먼저 보고 active인 세션만 events를 읽는다
    - `cleanup(data_dir: &Path, keep_days: u64, now: u64) -> usize` — 마지막 활동이 오래된 세션 폴더 삭제
    - `const ACTIVE_WINDOW_MS: u64 = 30 * 60 * 1000`
  - `view.rs`:
    - `pub enum SessionView { Session, Turn }`
    - `pub struct Side { content: Option<PathBuf> /* None = 없음 */, too_large: bool }` — before/after 한쪽
    - `changes(dir: &SessionDir, view: SessionView, wt: &Worktree) -> Result<Vec<FileEntry>, WorktreeError>` — spec 4절 규칙. status/additions/deletions는 `no_index_patch`의 numstat가 아니라 `git diff --no-index --numstat`로. 접기는 `profile.collapses || linguist_generated`. 순서는 경로순
    - `patch(dir: &SessionDir, view: SessionView, wt: &Worktree, rel: &str) -> Result<FilePatch, WorktreeError>` — 목록에 없는 경로면 `UnknownFile`
    - `Uncertain` baseline은 hook이 저장한 내용 파일(세션 시작 HEAD 내용)을 before로 쓴다. 내용 파일이 없으면 before 없음(빈 파일). 앱은 세션 폴더 밖에 임시 파일을 만들지 않는다

- [ ] **Step 1: Write the failing tests** (`tests/session_view.rs`, `TestRepo` + tempdir data dir)

```rust
#[test] fn session_view_modified_added_deleted()   // baseline File(a v1)→현재 v2: modified; New→현재 있음: added; File→현재 없음: deleted
#[test] fn session_view_drops_reverted_files()     // baseline == 현재 → 목록에 없음
#[test] fn session_view_survives_commit()          // 수정 후 git commit → 세션 목록·patch 그대로
#[test] fn session_view_uncertain_uses_start_head() // Uncertain{bytes: start_head 내용} → before = 그 내용, FileEntry.uncertain true
#[test] fn session_view_only_this_worktree()       // 다른 폴더 경로 event는 빠짐
#[test] fn turn_view_closed_turn()                  // turn1 a v1→v2, turn2 a v2→v3 + b 수정 → turn 2 목록 [a, b], a before == turns/1 내용
#[test] fn turn_view_open_turn_compares_current()   // turn 3 열림 + event 있음 → after = 현재 파일
#[test] fn turn_view_open_turn_without_events_shows_last_closed()
#[test] fn turn_view_before_falls_back_to_baseline() // turn 2에 처음 건드린 파일 → before = baseline
#[test] fn turn_zero_merges_into_turn_one()
#[test] fn patch_headers_use_worktree_relative_path() // diff text에 "a/src/a.py", "b/src/a.py", 세션 폴더 경로 없음
#[test] fn patch_unknown_file_errors()
#[test] fn too_large_side_is_too_large_body()
#[test] fn nfd_path_matches_worktree_path()          // event 경로를 NFD 한글로 기록 → 목록 경로 NFC 상대 경로
#[test] fn sessions_for_orders_and_attributes()      // 두 세션, 귀속·순서·touched_files
#[test] fn active_rules()                            // ended → false, 31분 전 활동 → false, 방금 → true
#[test] fn cleanup_removes_old_sessions_only()
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-core --test session_view` → 컴파일 실패
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run** — `cargo test` 전부 PASS (`read_only.rs` 포함)
- [ ] **Step 5: Commit** — `feat(core): 이번 세션·마지막 응답 변경 계산`

---

### Task 5: 감시, Tauri command, event

**Files:**
- Create: `crates/core/src/watch/sessions_watcher.rs`, test in `crates/core/tests/watcher.rs`
- Modify: `crates/core/src/watch/mod.rs`, `crates/core/src/worktree.rs` (`WorktreeSummary.active_session: bool`, 기본 false), `crates/app/src/main.rs`

**Interfaces:**
- Consumes: Task 4 `list`, `view`
- Produces:
  - `spawn_sessions_watcher(sessions_dir: &Path, on_change: Arc<dyn Fn(Vec<String>) + Send + Sync>) -> notify::Result<SessionsWatchHandle>` — 폴더가 없으면 만든다. 300ms debounce, 바뀐 경로의 첫 component(session id)를 중복 없이 모아 부른다
  - Tauri:
    - `list_sessions(worktreeId) -> SessionSummary[]`
    - `list_files(worktreeId, view, sessionId: Option<String>) -> FileEntry[]` — `branch`는 기존 그대로, `session`/`turn`은 `sessionId` 필수(없으면 오류)
    - `get_patch(worktreeId, view, path, sessionId: Option<String>) -> FilePatch`
    - `set_viewed`는 그대로
    - `list_repos`: 각 `WorktreeSummary.activeSession`을 `active_roots`로 채운다
    - event `session-updated` `{ sessionIds: string[], worktreeIds: string[] }` — `worktrees_of`로 귀속 worktree를 구한다
  - 시작 시(registry를 연 background thread에서) `cleanup(data_dir, config.sessions.keep_days, now)`

- [ ] **Step 1: Write the failing watcher test**

```rust
#[test] fn session_write_reaches_callback_within_one_second() // tempdir sessions/, 감시 시작 300ms 대기 → SessionDir::append_event → 1초 안에 vec!["s1"]
```

- [ ] **Step 2: Run to verify it fails**, **Step 3: Implement watcher**, **Step 4: Run** — PASS
- [ ] **Step 5: Tauri 연결** — 위 command/event. `cargo build -p wtboard-app && cargo test` 성공
- [ ] **Step 6: Commit** — `feat(app): 세션 목록·세션 view command와 session-updated event`

---

### Task 6: plugin과 설치 script

**Files:**
- Create: `plugin/.claude-plugin/marketplace.json`, `plugin/wtboard/.claude-plugin/plugin.json`, `plugin/wtboard/hooks/hooks.json`, `plugin/wtboard/hooks/run.sh` (실행 권한), `scripts/install-hook.sh`, `crates/hook/tests/plugin.rs`

**Interfaces:**
- `marketplace.json`: `{"name":"wtboard-local","owner":{"name":"wtboard"},"plugins":[{"name":"wtboard","source":"./wtboard","description":"wtboard 세션 기록 hook"}]}`
- `plugin.json`: `{"name":"wtboard","version":"0.1.0","description":"Claude Code 세션이 바꾼 파일을 wtboard에 기록한다"}`
- `hooks.json` (top-level `"hooks"` key):
  - `SessionStart` → `session-start`; `UserPromptSubmit` → `prompt-submit`
  - `PreToolUse` matcher `Edit|Write|NotebookEdit` → `pre-edit`; matcher `Bash` → `pre-bash`
  - `PostToolUse`와 `PostToolUseFailure` 각각 matcher `Edit|Write|NotebookEdit` → `post-edit`; matcher `Bash` → `post-bash`
  - `Stop`, `StopFailure` → `turn-end`; `SessionEnd` → `session-end`
  - command 형식: `"\"${CLAUDE_PLUGIN_ROOT}\"/hooks/run.sh <subcommand>"`
- `run.sh`: `BIN="${WTBOARD_HOOK_BIN:-$HOME/.wtboard/bin/wtboard-hook}"; [ -x "$BIN" ] || exit 0; exec "$BIN" "$@"`
- `install-hook.sh`: `cargo build --release -p wtboard-hook` → `~/.wtboard/bin/wtboard-hook`로 복사(폴더 생성) → 설치 명령 두 줄(`claude plugin marketplace add <repo>/plugin`, `claude plugin install wtboard@wtboard-local`)을 출력만 한다. Claude 설정은 건드리지 않는다

- [ ] **Step 1: Write the failing test** (`crates/hook/tests/plugin.rs`)

```rust
#[test] fn hooks_json_covers_every_subcommand()   // hooks.json 파싱 → spec 3절의 event·matcher·subcommand 조합이 모두 있음
#[test] fn run_sh_exits_zero_without_binary()     // WTBOARD_HOOK_BIN=/nonexistent → status 0
#[test] fn run_sh_forwards_to_binary()            // WTBOARD_HOOK_BIN=<CARGO_BIN_EXE> + session-start 입력 → meta 생김
```

- [ ] **Step 2: Run to verify it fails**, **Step 3: Create files**, **Step 4: Run** — PASS
- [ ] **Step 5: Validate** — `claude plugin validate plugin` 통과 (출력 report에 기록). `claude`가 없으면 그 사실을 report에 적는다
- [ ] **Step 6: Commit** — `feat(plugin): Claude Code plugin과 hook 설치 script`

---

### Task 7: UI 타입, 순수 함수, 상태

**Files:**
- Modify: `ui/src/api.ts`, `ui/src/store.ts`, `ui/src/lib/events.ts`, `ui/src/lib/events.test.ts`, `ui/src/useWorktreeEvents.ts`, `ui/src/lib/fixtures.ts`
- Create: `ui/src/lib/sessions.ts`, `ui/src/lib/sessions.test.ts`

**Interfaces:**
- Consumes: Task 5 IPC
- Produces:
  - `api.ts`: `View = 'branch' | 'session' | 'turn'`; `SessionSummary { id; startedAt; lastEventAt; active; turn; turnOpen; touchedFiles }`; `ChangedFile.uncertain: boolean`; `WorktreeSummary.activeSession: boolean`; `listSessions(worktreeId)`; `listFiles(worktreeId, view, sessionId: string | null)`; `getPatch(worktreeId, view, path, sessionId: string | null)`; `SessionUpdated { sessionIds: string[]; worktreeIds: string[] }`
  - `store.ts`: `views: Record<worktreeId, View>`, `setView(worktreeId, view)`, `sessions: Record<worktreeId, sessionId>`, `selectSession(worktreeId, id)` (persist)
  - `lib/sessions.ts`:
    - `pickSession(sessions: SessionSummary[], remembered: string | undefined): SessionSummary | null` — remembered가 목록에 있으면 그것, 아니면 첫 번째(최근 활동), 없으면 null
    - `effectiveView(view: View, session: SessionSummary | null): View` — 세션이 없으면 `branch`
    - `filesKey(worktreeId, view, sessionId)` = `['files', worktreeId, view, sessionId ?? null]`, `patchKey(worktreeId, path, view, sessionId)` = `['patch', worktreeId, path, view, sessionId ?? null]`
    - `turnLabel(s: SessionSummary): string` — `#${turn}` + (`turnOpen`이면 ` 진행 중`)
  - `lib/events.ts`: `keysForSessionUpdated(ev: SessionUpdated): QueryKey[]` — `['repos']` 한 번, 그리고 worktree마다 `['sessions', id]`, `['files', id, 'session']`, `['files', id, 'turn']`, `['patch', id]`. patch를 worktree 전체로 무효화하는 이유: 새 prompt로 마지막 응답의 대상 turn이 바뀌면 같은 path·view·session key의 내용이 달라진다 (화면에 보이는 patch만 다시 가져온다). 기존 `keysToInvalidate`는 그대로 (prefix `['files', id]`, `['patch', id, path]`가 새 key에도 걸린다)
  - `useWorktreeEvents`: `session-updated` listen 추가

- [ ] **Step 1: Write the failing tests** — `pickSession`(기억한 것 우선, 없어진 경우, 빈 목록), `effectiveView`, key 함수 모양, `turnLabel`, `keysForSessionUpdated`(worktree 2개 → key 목록 정확히)
- [ ] **Step 2: Run to verify it fails**, **Step 3: Implement**, 기존 `WorktreeView`/`FileDiff`의 query key를 새 key 함수로 바꿔 build가 깨지지 않게 한다 (branch 동작 그대로)
- [ ] **Step 4: Run** — `pnpm --dir ui test && pnpm --dir ui build`
- [ ] **Step 5: Commit** — `feat(ui): 세션 view 타입과 탭·세션 선택 상태`

---

### Task 8: UI 화면과 실제 세션 확인

**Files:**
- Modify: `ui/src/components/WorktreeView.tsx`, `ui/src/components/FileDiff.tsx`, `ui/src/components/Sidebar.tsx`
- Create: `ui/src/components/ViewTabs.tsx`

**Interfaces:**
- Consumes: Task 7 전부
- spec 6절 모양대로:
  - `ViewTabs`: `브랜치 전체` / `이번 세션` (+ 세션이 2개 이상이면 select: `HH:MM 시작`, 활성이면 `●`) / `마지막 응답 {turnLabel}`. 세션이 없으면 두 탭 비활성 + `title="이 worktree의 Claude Code 세션 기록이 없습니다"`
  - `WorktreeView`: `useQuery(['sessions', id])`, `pickSession`, `effectiveView`로 files query. `branch`가 아닐 때 `확인 n/m` 숨김. 빈 목록 문구: session → `이 세션이 바꾼 파일이 없습니다.`, turn → `이 응답이 바꾼 파일이 없습니다.`
  - `FileDiff`: `view`, `sessionId` prop. `branch`가 아니면 `봤음` checkbox 숨김. `file.uncertain`이면 header에 회색 `원본 불확실` (`title="Bash가 바꾼 파일이라 세션 전 내용을 모릅니다. 세션 시작 commit과 비교합니다"`)
  - `Sidebar`: `activeSession`이면 worktree 이름 앞에 초록 `●`
- [ ] **Step 1: Implement**, **Step 2:** `pnpm --dir ui build && pnpm --dir ui test`
- [ ] **Step 3: 실제 세션 확인** (memory `tauri-ui-automation`의 안전 규칙: 클릭 직전 frontmost·창 안 확인, 키 입력 없음, `screencapture -R`)
  1. `scripts/fixtures.sh "$SCRATCH/fx4"`, `cargo build -p wtboard-hook`
  2. Node fixture worktree에서: `WTBOARD_CONFIG=$SCRATCH/fx4/config.toml WTBOARD_HOOK_BIN=$PWD/target/debug/wtboard-hook claude --plugin-dir plugin/wtboard -p "<src의 파일 하나에 함수 하나 추가하고, Bash로 sed를 써서 다른 파일 한 줄 바꾸고, Agent tool로 subagent에게 세 번째 파일 수정을 맡겨라>" --permission-mode acceptEdits`(Bash 허용이 필요하면 `--allowedTools`로 그 명령만 허용) → `$SCRATCH/fx4/sessions/<id>/` 내용 확인. subagent edit이 같은 `session_id`로 들어왔는지 report에 기록
  3. 같은 세션으로 `claude --resume <id> -p "<첫 파일 한 줄 더 수정>"` 한 번 더 (turn 2)
  4. `WTBOARD_CONFIG=... pnpm tauri dev` → screenshot: sidebar(세션이 끝났으면 ● 없음), 이번 세션 탭(파일 3개, sed 파일), 마지막 응답 탭(turn 2의 파일 하나), 브랜치 전체 탭은 그대로
  5. fixture 레포 `.git/index` mtime이 확인 전후 같음 (`GIT_OPTIONAL_LOCKS=0`으로만 git 조회)
- [ ] **Step 4: Commit** — `feat(ui): 이번 세션·마지막 응답 탭`

---

### Task 9: 마무리

**Files:** `docs/progress.md`, `CLAUDE.md`, `docs/draft.md`(12절 interface 표에 `session_id` 인자와 `session-updated` 모양만 반영)

- [ ] **Step 1:** `cargo test && pnpm --dir ui test && pnpm --dir ui build` 전부 PASS
- [ ] **Step 2:** `progress.md` — 4단계 완료·5단계 다음, "4단계 결과", "4단계에서 정한 것 (spec과 다른 점)", 알려진 한계(SessionEnd 1.5초, 큰 레포 pre-bash 시간, ignored 파일 미추적, 앱 번들 미포함), "5단계로 넘어간 것"(`upstream` PR base, summary cache, hook binary 번들). CLAUDE.md 명령 절에 `scripts/install-hook.sh`, 구조 절에 `crates/hook`, `plugin/`, `core::session`
- [ ] **Step 3: Commit** — `docs: 4단계 완료, 진행 상황 갱신`
