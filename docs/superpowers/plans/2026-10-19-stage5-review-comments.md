# 5단계: 리뷰 코멘트 자동 첨부 + 메뉴바 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** diff 줄에 단 리뷰 코멘트가 그 worktree의 다음 Claude Code prompt에 자동으로 붙고, 메뉴바에 미확인 수와 worktree 바로 가기가 생긴다.

**Architecture:** `state.sqlite`를 한 곳(`core::store::db`)에서 열어 WAL + `user_version` 1로 올리고 `comments`·`settings` table을 더한다. 첨부 텍스트와 "claim → 출력 → 전송됨 commit" transaction은 `core::review`에 두어 hook과 앱("마크다운으로 복사")이 같이 쓴다. hook은 기존 `prompt-submit`에서 stdout JSON(`additionalContext`)을 낸다. 앱은 코멘트 command, Tauri tray, 창 숨기기를 더하고, UI는 react-diff-view의 `renderGutter`·`widgets`로 줄 코멘트를 그린다.

**Tech Stack:** Rust (rusqlite 0.32 bundled), Tauri 2.12 (`tray-icon` feature), React + TanStack Query + zustand + react-diff-view 3.3, vitest.

**Spec:** `docs/superpowers/specs/2026-10-19-stage5-review-comments-design.md`

## Global Constraints

- 앱과 hook은 레포에 아무것도 쓰지 않는다. 쓰기는 data dir(`state.sqlite`, `sessions/`)뿐. git 호출은 `core::git::Git`.
- `state.sqlite`: `journal_mode=WAL`, `busy_timeout` 앱 5초 / hook 1초, `user_version` 1. 0 → 1 migration은 기존 `viewed` row를 유지한다. 1보다 크면 오류.
- worktree key = `WatchLayout::detect(&Git::new(dir))?.root`의 문자열 (앱 `Worktree::id()`와 같다).
- `body` 1~4,000자 (trim 후 비면 오류), `snippet` 500자에서 자른다. 길이는 모두 `char` 수.
- 첨부 cap 9,000자(char). 복사는 cap 없음. JSON escape 뒤 `additionalContext` 길이 10,000자 이하.
- hook은 어떤 경우에도 exit 0, stderr는 많아야 한 줄. stdout은 `prompt-submit`이 첨부할 때만 JSON 한 개, 그 밖에는 비어 있다.
- 사용자 문구 (그대로): `미전송`, `전송됨 · HH:MM`, `복사됨 · HH:MM`, `수정`, `삭제`, `저장`, `취소`, `위치가 바뀐 코멘트 n개`, `마크다운으로 복사`, `전송된 코멘트 지우기`, `자동 첨부 꺼짐: 복사해서 Claude Code에 붙여 넣으세요`, `목록에 없음`, `이미 전송된 코멘트는 고칠 수 없습니다`, `wtboard: 리뷰 코멘트 n개를 첨부했습니다`(남으면 ` (m개 남음)`), `(코멘트 n개가 더 있습니다. 다음 메시지에 붙습니다)`, tray `미확인 n개`·`wtboard 열기`·`항상 위에 띄우기`·`종료`.
- 테스트를 먼저 쓴다. 실제 git, 실제 SQLite 파일, 실제 binary. `source ~/.cargo/env` 먼저.
- task마다 commit 직후 `simplify`를 돌리고, 바뀐 것이 있으면 별도 commit. UI task는 실제 앱 screenshot으로 확인한 뒤 끝낸다.

## Review Focus

1. 앱이 `state.sqlite`에 쓰는 중(봤음 체크)에 hook이 같은 DB를 연다 → hook은 `busy_timeout` 안에서 기다리고, 그래도 막히면 첨부 없이 exit 0, 코멘트는 미전송 유지 (Task 3 test `locked_db_leaves_comments_unsent`).
2. 3단계에서 만든 version 0 `state.sqlite`(봤음 row 있음)를 새 앱이 연다 → 봤음 row가 그대로이고 코멘트 table이 생긴다 (Task 1 test `migrates_v0_keeps_viewed_rows`).
3. 코멘트 body나 snippet에 ``` , `</wtboard-review-comments>`, 개행, 한글·emoji가 섞임 → 첨부 텍스트 구조가 깨지지 않고 cap은 문자 수로 센다 (Task 2 tests `fence_grows_past_backticks`, `cap_counts_chars_not_bytes`).
4. 사용자가 `/compact` 같은 slash command를 보냄 → 첨부하지 않고 미전송 유지 (Task 3 test `slash_command_does_not_attach`).
5. worktree 하위 폴더나 `/var/...`(symlink) 경로에서 Claude Code를 띄움 → 앱이 저장한 worktree key와 맞아 첨부된다 (Task 3 test `cwd_subdir_and_symlink_match_app_worktree_id`).

---

## File Structure

| 파일 | 할 일 |
|---|---|
| `crates/core/src/store/db.rs` (신규) | `state.sqlite` 열기(WAL, busy, migration), hook용 기존 DB 열기 |
| `crates/core/src/store/viewed.rs` | `db::open_state` 사용 |
| `crates/core/src/store/comments.rs` (신규) | `Comment`, `Side`, `NewComment`, `CommentStore` |
| `crates/core/src/store/settings.rs` (신규) | `SettingsStore` (key-value) |
| `crates/core/src/store/config.rs` | `[review] auto_attach_comments` |
| `crates/core/src/review.rs` (신규) | 첨부 텍스트 `render`, `system_message`, `attach` transaction |
| `crates/hook/src/comments.rs` (신규), `crates/hook/src/main.rs` | prompt-submit 첨부, stdout 규칙 변경 |
| `crates/core/src/worktree.rs`, `registry.rs`, `repo.rs` | git 없는 tray 정보 (`unviewed_count`, branch cache) |
| `crates/app/src/main.rs`, `crates/app/src/comments.rs`(신규), `crates/app/src/tray.rs`(신규) | 코멘트 command, tray, 창, settings |
| `crates/app/Cargo.toml`, `crates/app/icons/tray.png`(신규), `capabilities/default.json` | tray feature, icon |
| `ui/src/api.ts`, `ui/src/lib/comments.ts`(신규), `ui/src/lib/events.ts` | 타입, 순수 함수, invalidate |
| `ui/src/components/FileDiff.tsx`, `CommentWidget.tsx`(신규), `CommentsPopover.tsx`(신규), `WorktreeView.tsx`, `FileTree.tsx`, `Sidebar.tsx`, `ui/src/useComments.ts`(신규), `ui/src/useWorktreeEvents.ts`, `ui/src/App.tsx` | 화면 |

---

### Task 1: `state.sqlite` schema 1, `CommentStore`, `SettingsStore`, `[review]` config

**Files:**
- Create: `crates/core/src/store/db.rs`, `crates/core/src/store/comments.rs`, `crates/core/src/store/settings.rs`, `crates/core/tests/state_db.rs`, `crates/core/tests/comment_store.rs`
- Modify: `crates/core/src/store/mod.rs`, `store/viewed.rs`, `store/config.rs`, `crates/core/tests/config.rs`

**Interfaces:**
- Produces:
  - `store::db::SCHEMA_VERSION: i32 = 1`
  - `store::db::open_state(path: &Path) -> Result<Connection, StoreError>` — 부모 폴더 생성, WAL, `busy_timeout(5s)`, migration. `user_version > 1`이면 `StoreError::NewerSchema(i32)`
  - `store::db::open_existing(path: &Path, busy: Duration) -> Result<Option<Connection>, StoreError>` — `OpenFlags::SQLITE_OPEN_READ_WRITE`(CREATE 없음). 파일 없음 또는 `user_version < 1` → `Ok(None)`
  - `store::db::open_memory() -> Connection` (migration까지)
  - `StoreError`에 `NewerSchema(i32)`, `InvalidBody(String)`, `AlreadySent`, `NotFound(i64)` 추가 (`viewed.rs`에서 `store/mod.rs`로 옮겨 공용)
  - `ViewedStore::open`/`in_memory`는 위 함수를 쓴다 (동작 같음)
  - `#[serde(rename_all = "lowercase")] pub enum Side { Old, New }`
  - `#[serde(rename_all = "camelCase")] pub struct Comment { id: i64, worktree: String, path: String, side: Side, line: u32, view: String, snippet: String, body: String, created_at: u64, updated_at: u64, sent_at: Option<u64>, sent_to: Option<String> }` (Serialize)
  - `pub struct NewComment { worktree: String, path: String, side: Side, line: u32, view: String, snippet: String, body: String }`
  - `CommentStore { conn: Mutex<Connection> }`: `open(&Path)`, `in_memory()`, `add(&NewComment) -> Result<Comment>`, `list(worktree) -> Result<Vec<Comment>>` (미전송 먼저, 그 안에서 `path, line, id`), `update_body(id, body) -> Result<Comment>`, `delete(id) -> Result<()>`, `clear_sent(worktree) -> Result<usize>`, `mark_sent(ids: &[i64], sent_to: &str, now: u64) -> Result<usize>` (`WHERE sent_at IS NULL`), `unsent_counts() -> Result<HashMap<String, usize>>`, `purge_sent_before(cutoff_ms: u64) -> Result<usize>`
  - `pub(crate) fn unsent_in(conn: &Connection, worktree: &str) -> rusqlite::Result<Vec<Comment>>` — `review::attach`가 같은 순서로 읽는다
  - `SettingsStore`: `open(&Path)`, `in_memory()`, `get(key) -> Result<Option<String>>`, `set(key, value) -> Result<()>`
  - `Config.review: ReviewConfig { auto_attach_comments: bool }` (`#[serde(default)]`, 기본 `true`)

- [ ] **Step 1: Write the failing tests**

```rust
// tests/state_db.rs
#[test] fn fresh_file_is_version_1_wal()        // open_state → PRAGMA user_version == 1, journal_mode == "wal", comments·settings table 있음
#[test] fn migrates_v0_keeps_viewed_rows()      // 3단계 SCHEMA로 viewed row 1개 만든 파일(version 0) → open_state 후 ViewedStore::load에 그 row, version 1
#[test] fn newer_version_is_error()             // user_version = 2로 만든 파일 → Err(NewerSchema(2))
#[test] fn open_existing_never_creates()        // 없는 경로 → Ok(None), 파일 생기지 않음. version 0 파일 → Ok(None)

// tests/comment_store.rs
#[test] fn add_and_list_order()                 // b.rs:3, a.rs:9, a.rs:2 추가, a.rs:2 mark_sent → list = [a.rs:9, b.rs:3, a.rs:2(sent)]
#[test] fn body_rules()                         // "  " → InvalidBody, 4001자 → InvalidBody, 4000자 OK; snippet 600자 → 500자로 저장
#[test] fn update_only_unsent()                 // update_body 미전송 OK(updated_at 증가) / 전송됨 → AlreadySent / 없는 id → NotFound
#[test] fn mark_sent_skips_already_sent()       // mark_sent([1], "s1") 후 mark_sent([1], "copy") == 0, sent_to == "s1"
#[test] fn clear_sent_and_counts()              // wt A 미전송 2·전송 1, wt B 미전송 1 → unsent_counts {A:2, B:1}; clear_sent(A) == 1
#[test] fn purge_sent_before_cutoff()           // sent_at 100, 200, 미전송 → purge_sent_before(150) == 1, 미전송은 남음
#[test] fn settings_roundtrip()                 // get 없음 → None, set("always_on_top","1") → Some("1"), 다시 set → 덮어씀

// tests/config.rs
#[test] fn review_auto_attach_default_true()    // [review] 없음 → true, auto_attach_comments = false → false
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-core --test state_db --test comment_store --test config` → 컴파일 실패
- [ ] **Step 3: Implement** `store::{db, comments, settings}`, config. migration은 `BEGIN; CREATE TABLE IF NOT EXISTS viewed …; CREATE TABLE comments …; CREATE INDEX …; CREATE TABLE settings …; PRAGMA user_version = 1; COMMIT` (spec 2절 SQL 그대로).
- [ ] **Step 4: Run** — `cargo test -p wtboard-core` 전부 PASS (기존 `viewed_store.rs` 포함)
- [ ] **Step 5: Commit** — `feat(core): state.sqlite schema 1과 코멘트·설정 저장`

---

### Task 2: 첨부 텍스트와 claim transaction (`core::review`)

**Files:**
- Create: `crates/core/src/review.rs`, `crates/core/tests/review.rs`
- Modify: `crates/core/src/lib.rs`

**Interfaces:**
- Consumes: Task 1 `Comment`, `Side`, `unsent_in`, `open_existing`
- Produces:
  - `pub const ATTACH_CAP: usize = 9_000;`
  - `pub struct Rendered { pub text: String, pub included: Vec<i64>, pub remaining: usize }`
  - `pub fn render(worktree_root: &str, comments: &[Comment], cap: Option<usize>) -> Rendered` — 입력 순서대로. 코멘트가 0개면 `text` 빈 문자열
  - `pub fn system_message(included: usize, remaining: usize) -> String`
  - `pub fn attach(conn: &mut Connection, worktree: &str, session_id: &str, now: u64, emit: impl FnOnce(&Rendered) -> std::io::Result<()>) -> Result<Option<Rendered>, StoreError>` — `BEGIN IMMEDIATE` → `unsent_in` → 비면 `Ok(None)`(emit 안 부름) → `render(.., Some(ATTACH_CAP))` → `emit` → 실패면 rollback 후 오류 → `included`만 `sent_at = now, sent_to = session_id` → commit

텍스트 형식은 spec 3절 예시 그대로 (`<wtboard-review-comments worktree="…">`, 안내 문장, 번호 목록, `path:line`, old side는 `path (삭제된 줄, 원래 N번째 줄)`, fence 안 snippet, body). 코멘트 사이는 빈 줄 하나.

- [ ] **Step 1: Write the failing tests** (`tests/review.rs`)

```rust
#[test] fn renders_new_and_old_side()          // 기대 문자열 전체를 assert_eq (spec 예시와 같은 두 코멘트)
#[test] fn fence_grows_past_backticks()        // snippet "let s = \"```\";" → fence가 "````"
#[test] fn closing_tag_in_body_is_escaped()    // body에 "</wtboard-review-comments>" → 출력에 닫는 tag가 정확히 1번 (끝)
#[test] fn cap_stops_and_counts_remaining()    // body 4,000자 코멘트 3개, cap 9,000 → included 2, remaining 1, 끝에 "(코멘트 1개가 더 있습니다. 다음 메시지에 붙습니다)", text.chars().count() <= 9_000
#[test] fn cap_counts_chars_not_bytes()        // 한글 body 2,900자 × 3 → 셋 다 들어감 (byte로 세면 2개)
#[test] fn no_cap_includes_all()               // cap None → included 3
#[test] fn system_message_text()               // (3,0) → "wtboard: 리뷰 코멘트 3개를 첨부했습니다", (2,1) → "… 2개를 첨부했습니다 (1개 남음)"
#[test] fn attach_marks_only_included()        // 4,000자 3개 → attach 후 2개 sent_to == "s1", 1개 미전송
#[test] fn attach_emit_error_keeps_unsent()    // emit이 Err → 모두 미전송, Err 반환
#[test] fn attach_without_comments_skips_emit() // emit 안 불림, Ok(None)
#[test] fn concurrent_attach_claims_once()     // 같은 파일 DB에 연결 2개, thread 2개가 동시에 attach (s1, s2) → 각 코멘트 sent_to가 하나, emit된 id 합집합 = 전체, 교집합 = 없음
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-core --test review`
- [ ] **Step 3: Implement** `review.rs`. 닫는 tag escape는 body·snippet 안의 `</wtboard-review-comments>`를 `<\/wtboard-review-comments>`로 바꾼다. cap 판정은 "다음 코멘트 + 남은 수 안내 줄 + 닫는 tag"까지 넣어도 cap 이하인지로 한다.
- [ ] **Step 4: Run** — PASS
- [ ] **Step 5: Commit** — `feat(core): 리뷰 코멘트 첨부 텍스트와 claim transaction`

---

### Task 3: hook `prompt-submit` 첨부

**Files:**
- Create: `crates/hook/src/comments.rs`, `crates/hook/tests/comments.rs`
- Modify: `crates/hook/src/main.rs`, `crates/hook/tests/common/mod.rs` (코멘트 helper)

**Interfaces:**
- Consumes: Task 1 `open_existing`, `Config::load`, `Config.review`; Task 2 `attach`, `system_message`; `WatchLayout::detect`
- Produces:
  - `HookInput.prompt: Option<String>`
  - `comments::attach(input: &HookInput, out: &mut impl Write) -> HookResult` — spec 3절 순서 1~5. stdout JSON은 `{"systemMessage": …, "hookSpecificOutput": {"hookEventName": "UserPromptSubmit", "additionalContext": …}}` 한 줄 + flush
  - `main::run`: `prompt-submit`이면 `comments::attach(&input, &mut stdout)`와 `turn::prompt_submit`를 둘 다 실행하고, 오류가 둘이면 `; `로 이어 한 줄. 모듈 doc의 stdout 규칙을 "prompt-submit의 첨부 JSON만 쓴다"로 고친다

- [ ] **Step 1: Write the failing tests** (`tests/comments.rs`. `init_repo` 등은 `core/tests/common`, 코멘트는 `CommentStore::open(<data>/state.sqlite)`로 넣는다. worktree key는 `Worktree::open(repo, None)?.id()`)

```rust
#[test] fn attaches_unsent_and_marks_sent()   // 코멘트 2개 → stdout JSON 파싱: additionalContext에 "a.rs:3"과 body, systemMessage "…2개를…"; DB 둘 다 sent_to == sid; 세션 meta.turn == 1 (turn 기록도 됨)
#[test] fn other_worktree_not_attached()      // worktree 2개 (`git worktree add`), B에만 코멘트, A에서 prompt-submit → stdout 비어 있음, B 코멘트 미전송
#[test] fn cwd_subdir_and_symlink_match_app_worktree_id() // cwd = <repo>/src (tempdir는 /var/... 형태 그대로) → 첨부됨
#[test] fn auto_attach_off()                  // config.toml에 [review] auto_attach_comments = false → assert_quiet_exit, 미전송
#[test] fn slash_command_does_not_attach()    // prompt "/compact" → assert_quiet_exit, 미전송; prompt 없음(필드 없음)은 첨부함
#[test] fn missing_or_old_db_is_quiet()       // state.sqlite 없음 → quiet, 파일 안 생김; version 0 DB → quiet; 깨진 파일(쓰레기 byte) → exit 0, stdout 빈, stderr ≤ 1줄
#[test] fn locked_db_leaves_comments_unsent() // 다른 연결이 BEGIN EXCLUSIVE를 쥔 채 hook 실행 → 2초 안에 exit 0, stdout 빈, lock 푼 뒤 미전송
#[test] fn overflow_attaches_rest_next_time() // 4,000자 3개 → 1회차 2개 + "1개 남음", 2회차 1개
#[test] fn attach_keeps_index_untouched()     // read_only.rs처럼 hook 전후 .git/index mtime·내용 같음
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-hook --test comments`
- [ ] **Step 3: Implement** `comments.rs`, `main.rs` 변경. busy timeout 1초. config 읽기 실패 → `true`. `WatchLayout::detect` 실패(레포 아님) → 아무것도 안 함.
- [ ] **Step 4: Run** — `cargo test -p wtboard-hook` 전부 PASS (기존 `assert_quiet_exit`를 쓰는 prompt-submit test는 코멘트가 없으므로 그대로 통과해야 한다). release 빌드로 첨부 경로 시간 측정: `hyperfine` 없으면 10회 `time` → 50ms 이하 기록
- [ ] **Step 5: Commit** — `feat(hook): prompt-submit이 미전송 리뷰 코멘트를 첨부한다`

---

### Task 4: 앱 코멘트 command와 summary

**Files:**
- Create: `crates/app/src/comments.rs`
- Modify: `crates/app/src/main.rs`, `crates/core/src/worktree.rs` (`WorktreeSummary.unsent_comments`)

**Interfaces:**
- Consumes: Task 1 `CommentStore`, `NewComment`, `Side`, `Config.review`; Task 2 `render`
- Produces (Tauri command, 인자는 camelCase로 UI에서 넘어온다):
  - `list_comments(worktree_id) -> Vec<Comment>`
  - `add_comment(worktree_id, path, side: Side, line: u32, view: String, snippet, body) -> Comment` — `view`는 `branch|session|turn`만
  - `update_comment(id: i64, body) -> Comment`, `delete_comment(id: i64)`, `clear_sent_comments(worktree_id) -> usize`
  - `copy_comments(worktree_id) -> CopyPayload { markdown: String, ids: Vec<i64> }` (`render(.., None)`, 상태 안 바꿈)
  - `mark_comments_sent(ids: Vec<i64>) -> usize` (`sent_to = "copy"`)
  - `WorktreeSummary.unsent_comments: usize` (core, 기본 0) — `list_repos`가 `unsent_counts()`로 채운다
  - `ReposResponse.auto_attach_comments: bool` — `list_repos`마다 `Config::load(Config::default_path())`, 실패면 `true`
  - `AppState.comments: OnceLock<Arc<CommentStore>>`, `AppState.settings: OnceLock<SettingsStore>` — `open_registry`에서 `state.sqlite`로 연다. 열기 실패면 `in_memory()` + 기존 경고 방식(`reg.warn`, 문구 `코멘트를 저장할 수 없음: …`)
  - `open_registry`에서 세션 정리와 같은 때 `purge_sent_before(now - keep_days일)`

- [ ] **Step 1: Write the failing tests** (`crates/app/src/comments.rs`의 `#[cfg(test)]`. command 본문은 `CommentStore`를 받는 순수 함수로 떼어 test한다)

```rust
#[test] fn add_rejects_unknown_view()        // view "commit" → Err, "branch"|"session"|"turn" → Ok
#[test] fn copy_payload_lists_unsent_only()  // 미전송 2 + 전송 1 → ids 2개, markdown에 cap 없음 (4,000자 3개 전부)
#[test] fn update_sent_error_text()          // 전송된 코멘트 update → Err("이미 전송된 코멘트는 고칠 수 없습니다")
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-app`
- [ ] **Step 3: Implement** command와 `invoke_handler` 등록, summary 채우기.
- [ ] **Step 4: Run** — `cargo test` 전부 PASS
- [ ] **Step 5: Commit** — `feat(app): 리뷰 코멘트 command와 worktree별 미전송 수`

---

### Task 5: 메뉴바 tray, 창 숨기기, 항상 위에

**Files:**
- Create: `crates/app/src/tray.rs`, `crates/app/icons/tray.png`
- Modify: `crates/app/Cargo.toml` (`tauri = { version = "2", features = ["tray-icon"] }`), `crates/app/src/main.rs`, `crates/core/src/worktree.rs`, `crates/core/src/repo.rs`, `crates/core/src/registry.rs`

**Interfaces:**
- Consumes: Task 1 `SettingsStore`; 기존 `active_roots`, `Sink`
- Produces:
  - core: `Worktree::tray_info(&self) -> (String, usize)` — (`branch_key`, 미확인 수). git을 부르지 않는다 (`State`만 읽음. 미확인 = `summary()`의 `unviewed_files`와 같은 계산)
  - core: `pub struct TrayWorktree { pub id: String, pub repo: String, pub branch: String, pub unviewed: usize }`, `Registry::tray_worktrees(&self) -> Vec<TrayWorktree>` (sidebar 순서, 열린 worktree만, git 호출 없음). branch가 `''`(detached)면 worktree 폴더 이름
  - app `tray.rs`:
    - `pub struct TrayItem { pub id: String, pub label: String, pub count: usize, pub active: bool }`
    - `pub struct TrayModel { pub title: Option<String>, pub header: String, pub items: Vec<TrayItem> }`
    - `pub fn model(wts: &[TrayWorktree], active: &HashSet<PathBuf>) -> TrayModel` — title = 합계(0이면 None), header = `미확인 n개`, items = 미확인 > 0 또는 active인 것, label `● repo · branch` / `  repo · branch` (active 표시는 label 앞), count는 오른쪽 정렬 대신 label 끝에 붙여도 된다
    - `TrayUpdater` — `request()`를 여러 번 불러도 1초에 한 번 `model` 계산 → 메뉴 다시 만들기 + `set_title`. 60초 timer도 `request()`
  - menu id: `wt:<worktree id>`, `open`, `always_on_top`(CheckMenuItem), `quit`
  - event `select-worktree` payload `{ worktreeId: String }`
  - 창: `on_window_event`의 `CloseRequested` → `prevent_close()` + `hide()`. `RunEvent::Reopen` → `show()` + `set_focus()`. 시작 때 `settings.get("always_on_top") == Some("1")`이면 `set_always_on_top(true)`
  - `request()` 호출 지점: `Sink.changed`, `Sink.updated`, `set_viewed` 성공, `session-updated` 송신, timer

- [ ] **Step 1: Write the failing tests** (`tray.rs`의 `#[cfg(test)]`, core test 하나)

```rust
#[test] fn title_is_total_or_none()           // unviewed [3,0,4] → title Some("7"), header "미확인 7개"; 전부 0 → None, "미확인 0개"
#[test] fn items_unviewed_or_active()         // [a:3, b:0 active, c:0] → items [a, b], b.label이 "●"로 시작
#[test] fn detached_uses_folder_name()        // core tests/registry.rs: detached worktree의 TrayWorktree.branch == 폴더 이름
#[test] fn tray_info_matches_summary()        // core tests/viewed.rs: 체크 1개 후 tray_info().1 == summary().unviewed_files
```

- [ ] **Step 2: Run to verify it fails** — `cargo test -p wtboard-app`, `cargo test -p wtboard-core --test registry --test viewed`
- [ ] **Step 3: Implement.** tray icon: 단색 검정 + 투명 배경 PNG, 36×36 (@2x 18pt). 만드는 방법은 자유 (Python, `sips` 등). `TrayIconBuilder::with_id("main").icon(..).icon_as_template(true).menu(..).show_menu_on_left_click(true)`.
- [ ] **Step 4: Run** — `cargo test` PASS, `pnpm tauri dev`로 띄워 메뉴바 숫자가 보이는지 screenshot (실제 앱 확인 규칙)
- [ ] **Step 5: Commit** — `feat(app): 메뉴바 미확인 수와 worktree 바로 가기, 창 숨기기, 항상 위에`

---

### Task 6: UI 타입·순수 함수·event

**Files:**
- Create: `ui/src/lib/comments.ts`, `ui/src/lib/comments.test.ts`, `ui/src/useComments.ts`
- Modify: `ui/src/api.ts`, `ui/src/lib/events.ts`, `ui/src/lib/events.test.ts`, `ui/src/useWorktreeEvents.ts`, `ui/src/App.tsx`, `ui/src/lib/selection.ts`(+test)

**Interfaces:**
- Consumes: Task 4 command 이름·타입, Task 5 `select-worktree`
- Produces:
  - `api.ts`: `type Side = 'old' | 'new'`, `interface Comment` (Task 1 필드 camelCase), `WorktreeSummary.unsentComments: number`, `ReposResponse.autoAttachComments: boolean`, 함수 `listComments`, `addComment(worktreeId, {path, side, line, view, snippet, body})`, `updateComment(id, body)`, `deleteComment(id)`, `clearSentComments(worktreeId)`, `copyComments(worktreeId) → {markdown, ids}`, `markCommentsSent(ids)`
  - `lib/comments.ts`:
    - `anchorOf(change: Change, side: 'old' | 'new' (react-diff-view Side)) → { side: Side; line: number; snippet: string }` — delete → old·`lineNumber`; insert → new; normal → 언제나 new·`newLineNumber` (spec 5절). snippet은 `+`/`-`/공백 접두 없이 (parseDiff 결과의 `content`에 접두가 있는지 확인하고 맞춘다)
    - `placeComments(comments: Comment[], changes: Change[], view: View) → { byChangeKey: Map<string, Comment[]>; outdated: Comment[] }` — (side, line)과 snippet이 같은 change에 둔다. `side === 'old'`는 `comment.view === view`일 때만. key는 react-diff-view `getChangeKey`
    - `sortForList(comments) → Comment[]` (미전송 먼저 → path → line → id. 서버 순서와 같아야 한다)
    - `statusLabel(c: Comment, fmtTime) → '미전송' | '전송됨 · 14:05' | '복사됨 · 14:05'`
    - `unsentByPath(comments) → Map<string, number>`
    - `commentsKey(worktreeId) = ['comments', worktreeId] as const`
  - `events.ts`: `filtersForSessionUpdated`가 worktree마다 `{queryKey: ['comments', id]}`도 넣는다
  - `useComments(worktreeId)` — `useQuery(commentsKey, listComments, { refetchInterval: (q) => q.state.data?.some(c => c.sentAt === null) ? 5000 : false })`. data가 바뀌면 (`sentAt` 집합 비교) `['repos']` invalidate. mutation hook들(add/update/delete/clear/copy)은 성공 시 `commentsKey`와 `['repos']` invalidate
  - `selection.ts`: `selectionForWorktree(repos, worktreeId) → Selection | null`; `useWorktreeEvents`가 `select-worktree`를 listen해 `select`

- [ ] **Step 1: Write the failing tests**

```ts
// comments.test.ts
test('anchorOf: delete→old, insert→new, normal→new', ...)       // 각 Change 변종 → {side, line, snippet}
test('placeComments: 같은 줄·같은 내용이면 그 change에', ...)
test('placeComments: 내용이 다르면 outdated', ...)
test('placeComments: old side는 단 view와 같을 때만', ...)       // view 'branch'에서 단 old 코멘트를 'session'에서 → outdated
test('sortForList: 미전송 → path → line', ...)
test('statusLabel', ...)                                         // sentTo 'copy' → '복사됨 · 14:05'
// events.test.ts
test('session-updated는 comments도 무효화', ...)
// selection.test.ts
test('selectionForWorktree: 있는 id → {repoId, worktreeId}, 없는 id → null', ...)
```

- [ ] **Step 2: Run to verify it fails** — `pnpm --dir ui test`
- [ ] **Step 3: Implement**
- [ ] **Step 4: Run** — `pnpm --dir ui test`, `pnpm --dir ui build` PASS
- [ ] **Step 5: Commit** — `feat(ui): 리뷰 코멘트 타입·배치 계산·event`

---

### Task 7: UI 화면 — 줄 코멘트, 목록 popover, sidebar·파일 목록 표시

**Files:**
- Create: `ui/src/components/CommentWidget.tsx` (저장된 코멘트 + 입력 widget), `ui/src/components/CommentsPopover.tsx`
- Modify: `ui/src/components/FileDiff.tsx`, `WorktreeView.tsx`, `FileTree.tsx`, `Sidebar.tsx`

**Interfaces:**
- Consumes: Task 6 전부
- Produces: 화면 (spec 5절 그림과 문구)
  - `FileDiff`: props에 `comments: Comment[]`(이 파일 것)과 `onAdd`/`onUpdate`/`onDelete` (참조 안정. memo 유지). `TextDiff`에 `renderGutter`(hover 때 `+` 버튼), `widgets`(`placeComments`의 `byChangeKey` + 열린 입력 widget 하나), diff 위 `위치가 바뀐 코멘트 n개`(`<details>`). header에 `💬 n`(미전송). 입력 widget: textarea autofocus, Cmd+Enter 저장, Esc 취소, 저장 실패 시 widget 유지 + 아래 오류
  - `WorktreeView` header: `💬 미전송 n` 버튼 → `CommentsPopover` (목록 클릭 → `document.getElementById(fileAnchorId(path))?.scrollIntoView()`, 없으면 `목록에 없음`), `마크다운으로 복사`(`copyComments` → `navigator.clipboard.writeText` → `markCommentsSent`), `전송된 코멘트 지우기`, `autoAttachComments === false`면 안내 문구
  - `FileTree` 파일 이름 옆 💬 (미전송 있을 때), `Sidebar` worktree 줄에 `💬n` (`unsentComments > 0`)

- [ ] **Step 1: Implement** (이 task는 component다. 순수 로직은 Task 6에서 test했다)
- [ ] **Step 2: Run** — `pnpm --dir ui test`, `pnpm --dir ui build` PASS
- [ ] **Step 3: 실제 앱 확인** — `scripts/fixtures.sh <tmp>` → `WTBOARD_CONFIG=<tmp>/config.toml pnpm tauri dev`. 클릭은 3단계 안전 규칙(클릭 직전 wtboard frontmost, 좌표가 창 rect 안, **키 입력은 보내지 않는다**)을 따른다. 그래서 글자 입력은 화면으로 하지 않는다:
  1. 클릭으로: gutter `+` → 입력 widget이 열림, `취소`로 닫힘 (추가 줄·삭제 줄·split 왼쪽 문맥 줄 각각) screenshot
  2. 코멘트 내용은 `sqlite3 <tmp>/state.sqlite`로 `comments`에 row를 직접 넣는다 (미전송 3개: 추가 줄, 삭제 줄, 문맥 줄; 전송됨 1개). 그 뒤 widget 표시·`💬` 수·popover·`삭제` 클릭·`마크다운으로 복사` 클릭 후 `복사됨`(`pbpaste`로 내용 확인)을 screenshot
  3. fixture 파일을 고쳐 코멘트 줄 내용을 바꾸면 `위치가 바뀐 코멘트 1개`로 옮겨 가는 것 screenshot
  4. `저장`의 실제 입력 경로는 Task 8에서 사용자에게 한 번 직접 입력해 달라고 요청한다
- [ ] **Step 4: Commit** — `feat(ui): 줄 리뷰 코멘트와 코멘트 목록`

---

### Task 8: 실제 Claude Code 확인과 문서

**Files:**
- Modify: `docs/progress.md`, `CLAUDE.md` (command 목록), `docs/draft.md`는 건드리지 않는다

- [ ] **Step 1: 실제 첨부 확인** (spec 7절 "실제 확인" 1~5). fixture에서 코멘트 2개(하나는 삭제 줄) → `cargo build -p wtboard-hook` → `WTBOARD_CONFIG=<tmp>/config.toml WTBOARD_HOOK_BIN=$PWD/target/debug/wtboard-hook claude --plugin-dir plugin/wtboard -p "리뷰 코멘트 반영해 줘"` → 파일이 코멘트대로 바뀌었는지 diff, 앱에서 `전송됨` screenshot. `systemMessage`가 출력에 보였는지 기록
- [ ] **Step 2:** `auto_attach_comments = false`로 같은 순서 → 파일 변화 없음, 코멘트 미전송
- [ ] **Step 3:** 메뉴바 숫자, 메뉴에서 worktree 선택, 창 닫기 → Dock 클릭으로 다시 열기, 항상 위에 토글 → 앱 재시작 후 유지 screenshot. fixture `.git/index` mtime 전후 비교
- [ ] **Step 4:** `docs/progress.md`에 5단계 결과·spec과 다른 점·알려진 한계, 표의 5단계를 완료로, "6단계 후보"(spec 범위 밖 4개 + 옛 hook binary 감지). `CLAUDE.md` 구조 줄에 `store/comments`, `review`, tray, 새 command
- [ ] **Step 5: Commit** — `docs: 5단계 완료, 진행 상황 갱신`
