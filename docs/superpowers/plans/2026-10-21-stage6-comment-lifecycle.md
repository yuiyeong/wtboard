# 6단계: 전송 뒤 코멘트의 생애 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 전송된 리뷰 코멘트마다 Claude 응답(turn)이 그 파일을 고쳤는지 보이고(`응답 #n 보기`), `해결됨`으로 치우거나 `다시 보내기`로 미전송에 되돌릴 수 있다. schema 변경으로 멈추는 옛 hook binary는 앱이 감지해 경고한다.

**Architecture:** `state.sqlite` schema 2가 `comments`에 `sent_turn`(첨부한 prompt가 연 turn)과 `resolved_at`을 더한다. hook은 첨부할 때 `sent_turn`을 쓰고, 앱은 `list_comments`마다 그 세션의 event(`touched_turns`)에서 코멘트별 `replyTurn`을 계산해 붙인다. UI는 전송됨 widget과 목록 popover에 버튼 세 개와 `응답 #n` 표시를 더하고, 다른 view로 옮긴 뒤 파일 목록이 오면 스크롤하는 `pendingScroll`을 둔다.

**Tech Stack:** Rust (rusqlite 0.32), Tauri 2, React + TanStack Query + zustand + react-diff-view, vitest.

**Spec:** `docs/superpowers/specs/2026-10-21-stage6-comment-lifecycle-design.md`

## Global Constraints

- 앱과 hook은 레포에 아무것도 쓰지 않는다. hook `version`은 stdin을 읽지 않고 stdout 한 줄·exit 0. 다른 subcommand의 stdout 규칙은 5단계 그대로.
- `state.sqlite` `user_version` 2. migration은 0 → 1 → 2 차례로. hook `open_existing`은 version이 2가 아니면 열지 않는다 (작으면 `None`, 크면 `NewerSchema`).
- 코멘트 상태: 미전송 `sent_at IS NULL` / 전송됨 `sent_at IS NOT NULL AND resolved_at IS NULL` / 해결됨 `resolved_at IS NOT NULL`. 목록 순서 `ORDER BY resolved_at IS NOT NULL, sent_at IS NOT NULL, path, line, id`.
- `replyTurn` = `sent_to` 세션의 Edit·Bash event 중 `turn >= sent_turn`이고 이 worktree의 그 path를 건드린 가장 큰 turn. turn 0은 1. `copy`·`sent_turn` NULL·세션 폴더 없음 → 없음.
- 사용자 문구 (그대로): `다시 보내기`, `해결됨`, `되돌리기`, `응답 #n 보기`, `응답 #n 진행 중 보기`, `응답 #n`, `해결됨 · HH:MM`, `해결된 코멘트 n개`, `💬 미전송 a · 응답 b`, `아직 전송되지 않은 코멘트는 해결됨으로 바꿀 수 없습니다`, `hook binary가 옛 version입니다 (schema 2 필요). scripts/install-hook.sh를 다시 실행하세요. 그때까지 리뷰 코멘트가 첨부되지 않습니다`.
- 테스트를 먼저 쓴다 (실패 확인 → 구현 → 통과). 실제 SQLite 파일, 실제 세션 폴더, 실제 binary. `source ~/.cargo/env` 먼저.
- branch `stage6-comment-lifecycle`. task마다 commit 직후 `simplify`를 돌리고 바뀐 것이 있으면 별도 commit. UI task는 실제 앱 screenshot으로 확인한 뒤 끝낸다 (`scripts/fixtures.sh`, `WTBOARD_CONFIG`).

## Review Focus

1. 옛 hook binary(schema 1)가 새 DB(2)를 만남 → 첨부하지 않고(Task 1 `open_existing_rejects_other_versions`) 앱 sidebar에 경고(Task 4 `check_warns_on_binary_without_schema`).
2. `다시 보내기` 직후 prompt → 바로 붙고 `sent_turn`·`sent_to`가 새 값, `resolved_at` NULL (Task 2 `resend_then_attach_gets_new_turn`).
3. 세션 기록이 `keep_days`로 지워진 뒤 목록 → `replyTurn` 없음, 오류 없음 (Task 2 `reply_turns_missing_or_invalid_session_is_none`).
4. `sent_to`에 폴더 이름으로 쓸 수 없는 값(`../x`) → 세션 폴더 밖을 읽지 않고 없음 (Task 2 같은 test).
5. `응답 #n 보기`를 눌렀는데 그 세션이 목록에 없음(귀속 안 됨, 목록 stale) → 버튼 자체가 없다 (Task 5 `replyTarget` test). 스크롤 요청한 파일이 목록에 없음 → 요청이 남아 있다가 목록에 들어오면 스크롤 (Task 5 `takeScroll` test).

---

## File Structure

| 파일 | 할 일 |
|---|---|
| `crates/core/src/store/db.rs` | `SCHEMA_VERSION = 2`, `MIGRATIONS` 배열, 순서대로 migration |
| `crates/core/src/store/comments.rs` | `sent_turn`·`resolved_at` 필드, `resolve`·`unresolve`·`resend`, 목록 3단 |
| `crates/core/src/store/mod.rs` | `StoreError::NotSent` |
| `crates/core/src/session/list.rs` | `touched_turns` |
| `crates/core/src/review.rs` | `attach`에 `turn`, `reply_turns` |
| `crates/hook/src/main.rs`, `crates/hook/src/comments.rs` | `version` subcommand, 첨부에 turn 전달 |
| `crates/app/src/comments.rs`, `crates/app/src/hook.rs`(신규), `crates/app/src/main.rs` | `CommentItem`, command 3개, hook version 확인 |
| `ui/src/api.ts`, `ui/src/lib/comments.ts`, `ui/src/nav.ts`(신규), `ui/src/lib/fixtures.ts`, `ui/src/useComments.ts` | 타입, 순수 함수, 스크롤 예약 store, mutation |
| `ui/src/components/CommentWidget.tsx`, `CommentsPopover.tsx`, `FileDiff.tsx`, `WorktreeView.tsx` | 화면 |
| `docs/progress.md`, `CLAUDE.md` | 6단계 기록, command·subcommand 목록 |

---

### Task 1: schema 2와 코멘트 상태 (`resolve`·`unresolve`·`resend`)

**Files:**
- Modify: `crates/core/src/store/db.rs`, `crates/core/src/store/comments.rs`, `crates/core/src/store/mod.rs`
- Modify (컴파일): `crates/core/tests/review.rs` `comment()` helper의 `Comment` literal에 `sent_turn: None, resolved_at: None`
- Test: `crates/core/tests/state_db.rs`, `crates/core/tests/comment_store.rs`

**Interfaces:**
- Produces: `db::SCHEMA_VERSION: i32 = 2`. `Comment { …기존…, sent_turn: Option<u32>, resolved_at: Option<u64> }` (serde camelCase: `sentTurn`, `resolvedAt`). `CommentStore::resolve(&self, id: i64, now: u64) -> Result<Comment, StoreError>`, `unresolve(&self, id: i64) -> Result<Comment, StoreError>`, `resend(&self, id: i64, now: u64) -> Result<Comment, StoreError>`. `StoreError::NotSent` (`#[error("아직 전송되지 않은 코멘트는 해결됨으로 바꿀 수 없음")]`).

- [ ] **Step 1: state_db.rs test를 version 2로 바꾸고 v1 migration test를 더한다**

`fresh_file_is_version_1_wal` → `fresh_file_is_version_2_wal`: `SCHEMA_VERSION == 2`, `user_version == 2`, `PRAGMA table_info(comments)`의 이름에 `sent_turn`과 `resolved_at`이 있다. `newer_version_is_error`는 3으로. `open_existing_never_creates`의 `v0`는 그대로 `None`, `ready`는 `user_version == 2`.

```rust
/// 5단계 앱이 만든 version 1 파일. comments·settings는 있고 새 column은 없다
const V1: &str = "CREATE TABLE viewed (worktree TEXT NOT NULL, branch TEXT NOT NULL, path TEXT NOT NULL, hash TEXT NOT NULL, viewed_at INTEGER NOT NULL, PRIMARY KEY (worktree, branch, path));
CREATE TABLE comments (id INTEGER PRIMARY KEY, worktree TEXT NOT NULL, path TEXT NOT NULL, side TEXT NOT NULL, line INTEGER NOT NULL, view TEXT NOT NULL, snippet TEXT NOT NULL, body TEXT NOT NULL, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, sent_at INTEGER, sent_to TEXT);
CREATE INDEX comments_unsent ON comments (worktree, sent_at);
CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
INSERT INTO comments (worktree, path, side, line, view, snippet, body, created_at, updated_at, sent_at, sent_to) VALUES ('w', 'a.py', 'new', 1, 'branch', 'x', 'b', 1, 1, 100, 's1');
PRAGMA user_version = 1;";

#[test]
fn migrates_v1_keeps_comment_rows() {
    // Connection::open → execute_batch(V1) → drop
    let conn = open_state(&path).unwrap();
    assert_eq!(user_version(&conn), 2);
    drop(conn);
    let list = CommentStore::open(&path).unwrap().list("w").unwrap();
    assert_eq!((list[0].sent_at, list[0].sent_to.as_deref(), list[0].sent_turn, list[0].resolved_at), (Some(100), Some("s1"), None, None));
}

#[test]
fn open_existing_rejects_other_versions() {
    // V1 파일: open_existing → Ok(None). version 3 파일: Err(NewerSchema(3)). open_state 뒤의 파일: Some, user_version 2
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test state_db`
Expected: FAIL (`SCHEMA_VERSION` 1, column 없음, `sent_turn` 필드 없음)

- [ ] **Step 3: db.rs 구현**

`SCHEMA_VERSION = 2`. `const MIGRATIONS: [&str; 2] = [MIGRATE_V1, MIGRATE_V2]` (`MIGRATE_V2` = spec 2절 SQL을 `BEGIN; … PRAGMA user_version = 2; COMMIT;`로). `migrate`: `for sql in &MIGRATIONS[version as usize..] { conn.execute_batch(sql)?; }`. `comments.rs`: `COLUMNS`에 `sent_turn, resolved_at` 추가, `Comment` 필드와 `from_row`(index 12, 13) 추가. `tests/review.rs`의 `comment()` literal 보정.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test state_db`
Expected: PASS

- [ ] **Step 5: comment_store.rs test 추가**

```rust
#[test]
fn resolve_only_sent() {
    let s = CommentStore::in_memory();
    let c = s.add(&new("w", "a.rs", 1)).unwrap();
    assert!(matches!(s.resolve(c.id, 300), Err(StoreError::NotSent)));
    s.mark_sent(&[c.id], "s1", 100).unwrap();
    assert_eq!(s.resolve(c.id, 300).unwrap().resolved_at, Some(300));
    assert_eq!(s.resolve(c.id, 400).unwrap().resolved_at, Some(300)); // 이미 해결됨이면 그대로
    assert!(matches!(s.resolve(999, 1), Err(StoreError::NotFound(999))));
    assert_eq!(s.unresolve(c.id).unwrap().resolved_at, None);
    assert_eq!(s.unresolve(c.id).unwrap().resolved_at, None);
    assert!(matches!(s.unresolve(999), Err(StoreError::NotFound(999))));
}

#[test]
fn resend_resets_send_columns() {
    // add → mark_sent("s1", 100) → resolve(300) → resend(400):
    // sent_at None, sent_to None, sent_turn None, resolved_at None, updated_at 400
    // unsent_counts()["w"] == 1. resend 다시 → 같은 값. resend(999) → NotFound
}

#[test]
fn list_order_three_tiers() {
    // 미전송 a.rs:9, 전송됨 b.rs:1, 해결됨 a.rs:2, 미전송 b.rs:3 →
    // [(a.rs,9), (b.rs,3), (b.rs,1), (a.rs,2)]
}

#[test]
fn clear_and_purge_include_resolved() {
    // A: 전송됨 1개 + 해결됨 1개 + 미전송 1개 → clear_sent("A") == 2, 남은 1
    // w: mark_sent(100) 뒤 resolve → purge_sent_before(150) == 1
}
```

- [ ] **Step 6: 실패 확인**

Run: `cargo test -p wtboard-core --test comment_store`
Expected: FAIL (`resolve` 등 없음)

- [ ] **Step 7: comments.rs·mod.rs 구현**

`StoreError::NotSent`. `resolve`: `UPDATE comments SET resolved_at = ?2 WHERE id = ?1 AND sent_at IS NOT NULL AND resolved_at IS NULL`; 0 row면 `get` 결과로 없음 → `NotFound`, 미전송 → `NotSent`, 그 밖에 → 그대로. `unresolve`: `SET resolved_at = NULL WHERE id = ?1`; 없으면 `NotFound`. `resend`: `SET sent_at = NULL, sent_to = NULL, sent_turn = NULL, resolved_at = NULL, updated_at = ?2 WHERE id = ?1 AND sent_at IS NOT NULL`; 없으면 `NotFound`, 미전송이면 그대로. `list` ORDER BY를 Global Constraints대로.

- [ ] **Step 8: 전체 core test 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add crates/core
git commit -m "feat(core): state.sqlite schema 2 (sent_turn, resolved_at), 코멘트 resolve·unresolve·resend"
```

---

### Task 2: 첨부가 `sent_turn`을 쓰고, 코멘트별 응답 turn을 계산한다

**Files:**
- Modify: `crates/core/src/review.rs`, `crates/core/src/session/list.rs`
- Test: `crates/core/tests/review.rs`, Create `crates/core/tests/replies.rs`

**Interfaces:**
- Consumes: Task 1의 `Comment.sent_turn`, `CommentStore::resend`. 기존 `session::list::touched`, `with_root`, `SessionDir`.
- Produces: `review::attach(conn: &mut Connection, worktree: &str, session_id: &str, turn: u32, now: u64, emit) -> Result<Option<Rendered>, StoreError>`. `session::list::touched_turns(dir: &SessionDir, worktree_root: &Path, all_roots: &[PathBuf]) -> HashMap<String, BTreeSet<u32>>`. `review::reply_turns(data_dir: &Path, worktree_root: &Path, all_roots: &[PathBuf], comments: &[Comment]) -> HashMap<i64, u32>`.

- [ ] **Step 1: review.rs test — 기존 `attach(…, "s1", 500, …)` 호출 4곳을 `attach(…, "s1", 3, 500, …)`로 바꾸고 두 test를 더한다**

```rust
fn sent_turns_of(path: &Path) -> Vec<Option<u32>>  // id 순 sent_turn

#[test]
fn attach_records_sent_turn_and_copy_does_not() {
    // three_long → attach(turn 3) → included 2개 sent_turn Some(3), 나머지 None
    // CommentStore::open(..).mark_sent(&[남은 id], "copy", 600) → 그 id sent_turn None
}

#[test]
fn resend_then_attach_gets_new_turn() {
    // three_long → attach("s1", 1, 100) → store.resolve(ids[0], 200) → store.resend(ids[0], 300)
    // → attach("s2", 7, 400): included == [ids[0], ids[2]] (ids[2]는 첫 cap에서 남은 것)
    // → ids[0]: sent_to s2, sent_turn 7, sent_at 400, resolved_at None
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-core --test review`
Expected: FAIL (인자 수)

- [ ] **Step 3: `attach`에 `turn: u32` 추가, UPDATE에 `sent_turn = ?4`**

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-core --test review`
Expected: PASS

- [ ] **Step 5: replies.rs test (실제 세션 폴더)**

`common::feat_repo()`의 `Worktree::open` root를 worktree로, `tempdir`를 data dir로. session `s1`에 `dir.append_event`로: turn 0 Edit `a.py`, turn 2 Bash `b.py`, turn 3 Edit `a.py`, turn 3 Edit `<다른 폴더>/c.py`(worktree 밖). session `s2`: turn 5 Edit `a.py`. `Comment` literal helper `sent(id, path, sent_to: Option<&str>, sent_turn: Option<u32>)`.

```rust
#[test]
fn touched_turns_groups_by_path_and_maps_turn_zero_to_one() {
    // s1: {"a.py": {1, 3}, "b.py": {2}}. c.py 없음
}

#[test]
fn reply_turn_is_latest_turn_at_or_after_sent_turn() {
    // (a.py, s1, 1) → 3; (a.py, s1, 3) → 3; (a.py, s1, 4) → 없음; (b.py, s1, 1) → 2; (b.py, s1, 3) → 없음
    // (a.py, s2, 1) → 5  (세션마다 자기 event)
}

#[test]
fn reply_turns_missing_or_invalid_session_is_none() {
    // sent_to "copy" / None / "s-none"(폴더 없음) / "../x" / sent_turn None → map에 없음. panic·오류 없음
}

#[test]
fn reply_turns_only_this_worktree() {
    // 안쪽 worktree root를 all_roots에 더하고 그 아래 경로 event → 바깥 worktree 코멘트에 안 잡힘 (기존 nested 규칙)
}
```

- [ ] **Step 6: 실패 확인**

Run: `cargo test -p wtboard-core --test replies`
Expected: FAIL (함수 없음)

- [ ] **Step 7: `touched_turns`와 `reply_turns` 구현**

`touched_turns`: `touched(&dir.events(), root, &with_root(root, all_roots))`를 `rel`로 묶어 `turn`을 모은다. `reply_turns`: 코멘트를 `sent_to`(`Some`이고 `"copy"`가 아니고 `sent_turn`이 `Some`)별로 묶고, 세션마다 `SessionDir::open(data_dir, id)`가 `Some`일 때만 `touched_turns` 한 번 → 코멘트마다 `turns.get(path).and_then(|t| t.range(sent_turn..).next_back())`.

- [ ] **Step 8: 통과 확인**

Run: `cargo test -p wtboard-core`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add crates/core
git commit -m "feat(core): 첨부가 sent_turn을 기록하고 코멘트별 응답 turn을 계산한다"
```

---

### Task 3: hook — `sent_turn` 전달과 `version` subcommand

**Files:**
- Modify: `crates/hook/src/main.rs`, `crates/hook/src/comments.rs`
- Test: `crates/hook/tests/comments.rs`, `crates/hook/tests/hook.rs`, `crates/hook/tests/common/mod.rs`

**Interfaces:**
- Consumes: Task 2 `review::attach(…, turn, now, …)`, `db::SCHEMA_VERSION`.
- Produces: `comments::attach(input: &HookInput, dir: &SessionDir, out: &mut impl Write) -> HookResult`. subcommand `version` → stdout `wtboard-hook <CARGO_PKG_VERSION> schema <SCHEMA_VERSION>\n`.

- [ ] **Step 1: test**

`common/mod.rs`에 `pub fn sent_turns(data_dir, worktree) -> Vec<(i64, Option<u32>)>`.

```rust
// tests/comments.rs
#[test]
fn sent_turn_follows_prompt_turn() {
    // seed → submit("go") → sent_turns == [(id1, Some(1))], meta.turn == 1
    // seed 둘째 → submit("again") → 둘째 (id2, Some(2)), 첫째 그대로 Some(1), meta.turn == 2
}

// tests/hook.rs
#[test]
fn version_prints_schema_without_reading_stdin() {
    let out = run_with_stdin(hook_command(data.path(), &["version"]), b"");
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("wtboard-hook {} schema 2\n", env!("CARGO_PKG_VERSION")));
    assert!(out.stderr.is_empty());
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-hook`
Expected: FAIL (`sent_turns` 없음 → 추가 후 `sent_turn` None; `version` stdout 비어 있음)

- [ ] **Step 3: 구현**

`main.rs` `run()`: handler match 앞에 `if sub == "version" { writeln!(io::stdout(), "wtboard-hook {} schema {}", env!("CARGO_PKG_VERSION"), SCHEMA_VERSION)?; return Ok(()); }`. `comments::attach(&input, &dir, &mut io::stdout().lock())`로 부르고, 안에서 `let turn = dir.read_meta().map_or(0, |m| m.turn) + 1;`을 `review::attach`에 넘긴다. 파일 머리 주석의 stdout 규칙에 `version`을 더한다.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-hook`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/hook
git commit -m "feat(hook): 첨부에 sent_turn을 쓰고 version subcommand를 더한다"
```

---

### Task 4: 앱 — command 3개, `replyTurn`, 옛 hook binary 경고

**Files:**
- Create: `crates/app/src/hook.rs`
- Modify: `crates/app/src/comments.rs`, `crates/app/src/main.rs`
- Test: 각 파일의 `#[cfg(test)]`

**Interfaces:**
- Consumes: Task 1 `resolve`·`unresolve`·`resend`, `StoreError::NotSent`; Task 2 `reply_turns`; `db::SCHEMA_VERSION`; 기존 `registry(&app)?.roots()`, `data_dir()`, `Registry::warn`.
- Produces: command `list_comments(worktree_id) -> Vec<CommentItem>` (`#[derive(Serialize)] #[serde(rename_all = "camelCase")] struct CommentItem { #[serde(flatten)] comment: Comment, reply_turn: Option<u32> }`), `resolve_comment(id: i64) -> Comment`, `unresolve_comment(id: i64) -> Comment`, `resend_comment(id: i64) -> Comment`. `hook::hook_binary() -> PathBuf` (`WTBOARD_HOOK_BIN` 비어 있지 않으면 그것, 아니면 `$HOME/.wtboard/bin/wtboard-hook`), `hook::parse_schema(stdout: &str) -> Option<i32>`, `hook::check(bin: &Path) -> Option<String>` (없는 파일 `None`; 실행해서 schema == `SCHEMA_VERSION`이면 `None`; 아니면 Global Constraints의 경고 문구, 실행 오류면 ` (<오류>)`를 뒤에 붙인다).

- [ ] **Step 1: test**

```rust
// comments.rs tests
#[test]
fn comment_item_serializes_flat_with_reply_turn() {
    // CommentItem { comment: add(...), reply_turn: Some(3) } → serde_json에 "replyTurn": 3, "sentAt": null, "sentTurn": null, "resolvedAt": null
}
#[test]
fn resolve_unsent_error_text() {
    // resolve(&store, id) == Err("아직 전송되지 않은 코멘트는 해결됨으로 바꿀 수 없습니다")
}

// hook.rs tests
#[test]
fn parse_schema_reads_number_after_schema() {
    // "wtboard-hook 0.1.0 schema 2\n" → Some(2); "" → None; "schema x" → None; "wtboard-hook prompt-submit: 모르는 subcommand" → None
}
#[test]
fn check_is_silent_without_binary_and_warns_on_binary_without_schema() {
    // tempdir/none → None; Path::new("/usr/bin/true") → Some(msg) && msg.contains("schema 2 필요")
}
```

- [ ] **Step 2: 실패 확인**

Run: `cargo test -p wtboard-app`
Expected: FAIL (컴파일)

- [ ] **Step 3: 구현**

`comments.rs`: `fn items(store, worktree_id: &str, data: &Path, roots: &[PathBuf]) -> Result<Vec<CommentItem>, String>` (store.list → `reply_turns(data, Path::new(worktree_id), roots, &list)` → 붙이기). `list_comments`는 `with_store` 안에서 `registry(&app)?.roots()`와 `data_dir()`를 구해 `items`를 부른다 (closure가 `app`을 clone해 잡는다). `msg()`에 `NotSent` 문구. command 3개는 `with_store(app, move |s| s.resolve(id, now_ms()).map_err(msg))` 꼴. `copy_payload`는 `store.list`를 직접 쓴다.
`hook.rs`: `check`는 `Command::new(bin).arg("version").stdin(Stdio::null()).output()`. `main.rs`: `mod hook;`, `open_registry`에서 `Registry::open` 뒤 `if let Some(message) = hook::check(&bin) { reg.warn(EntryError { path: bin.display().to_string(), message }) }`, `generate_handler!`에 command 3개.

- [ ] **Step 4: 통과 확인**

Run: `cargo test -p wtboard-app && cargo build`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/app
git commit -m "feat(app): resolve·unresolve·resend command, 코멘트 목록에 replyTurn, 옛 hook binary 경고"
```

---

### Task 5: UI 순수 함수 — 타입, 상태 글, 목록 순서, header 글, 응답 대상, 스크롤 예약

**Files:**
- Modify: `ui/src/api.ts`, `ui/src/lib/comments.ts`, `ui/src/lib/fixtures.ts`, `ui/src/useComments.ts`
- Create: `ui/src/nav.ts`
- Test: `ui/src/lib/comments.test.ts`, Create `ui/src/nav.test.ts`

**Interfaces:**
- Consumes: Task 4 command 이름과 `CommentItem` key.
- Produces (`api.ts`): `Comment`에 `sentTurn: number | null; resolvedAt: number | null`, `interface CommentItem extends Comment { replyTurn: number | null }`, `listComments → Promise<CommentItem[]>`, `resolveComment(id)`, `unresolveComment(id)`, `resendComment(id)` (각 `invoke<Comment>`). (`lib/comments.ts`): `statusLabel` (해결됨 → `해결됨 · HH:MM`, `resolvedAt`), `sortForList` 3단, `openComments(list: CommentItem[]): CommentItem[]` (`resolvedAt === null`), `replyCount(list): number` (전송됨·미해결·`replyTurn !== null`), `headerLabel(list): string`, `interface ReplyTarget { sessionId: string; view: 'session' | 'turn'; label: string }`, `replyTarget(c: CommentItem, sessions: SessionSummary[]): ReplyTarget | null`, `replyLabel(c): string | null` (`응답 #n`, 없으면 null). (`nav.ts`): `useNavStore` (`{ pendingScroll: { worktreeId: string; path: string } | null }`, persist 없음), `requestScroll(worktreeId, path)`, `takeScroll(worktreeId, listed: Set<string>): string | null`. (`useComments.ts`): `useCommentActions`에 `resolve`, `unresolve`, `resend` mutation (`id: number`), `useComments`가 `CommentItem[]`. (`fixtures.ts`): `commentFixture(): CommentItem` 기본 `sentTurn: null, resolvedAt: null, replyTurn: null`.

- [ ] **Step 1: test**

```ts
// comments.test.ts에 추가
describe('statusLabel', () => {
  it('해결됨 · 시각', () => {
    expect(statusLabel(commentFixture({ sentAt: 7, sentTo: 's1', resolvedAt: 9 }), fmt)).toBe('해결됨 · t9');
  });
});
describe('sortForList', () => {
  it('미전송 → 전송됨 → 해결됨, 그 안에서 path → line → id', () => {
    // resolved a.ts:1 (id 1), sent b.ts:1 (id 2), unsent a.ts:9 (id 3), unsent b.ts:3 (id 4), sent a.ts:2 (id 5)
    // → [3, 4, 5, 2, 1]
  });
});
describe('openComments / replyCount / headerLabel', () => {
  // openComments: resolvedAt 있는 것만 뺀다
  // replyCount: sentAt·replyTurn 있고 resolvedAt 없는 것만 (미전송에 replyTurn이 있어도 안 셈)
  // headerLabel: [] → '💬'; 미전송 2 → '💬 미전송 2'; 응답 3 → '💬 응답 3'; 둘 다 → '💬 미전송 2 · 응답 3'
});
describe('replyTarget', () => {
  const s = (id: string, turn: number, turnOpen = false): SessionSummary => ({ id, startedAt: 0, lastEventAt: 0, active: false, turn, turnOpen, touchedFiles: 0 });
  // replyTurn null → null; resolvedAt 있음 → null; 세션 목록에 없음 → null
  // replyTurn 3, 세션 turn 3 닫힘 → { sessionId, view: 'turn', label: '응답 #3 보기' }
  // replyTurn 3, 세션 turn 3 진행 중 → label '응답 #3 진행 중 보기', view 'turn'
  // replyTurn 2, 세션 turn 3 → view 'session', label '응답 #2 보기'
  // replyLabel: '응답 #3' / null
});

// nav.test.ts
describe('nav store', () => {
  // beforeEach: useNavStore.setState({ pendingScroll: null })
  // requestScroll('w', 'a.ts') → takeScroll('w', new Set(['a.ts'])) === 'a.ts', 그 뒤 pendingScroll null
  // 목록에 없으면 null이고 요청이 남는다 → 목록에 들어오면 돌려준다
  // 다른 worktree의 takeScroll은 null이고 요청을 지우지 않는다
  // 새 requestScroll이 이전 요청을 바꾼다 (slot 하나)
});
```

- [ ] **Step 2: 실패 확인**

Run: `pnpm --dir ui test`
Expected: FAIL (함수·필드 없음)

- [ ] **Step 3: 구현** (Interfaces대로. `replyTarget`: `sentTo` 세션을 찾아 `s.turn === c.replyTurn`이면 `turn` view, 그때 `s.turnOpen`이면 `진행 중` 문구)

- [ ] **Step 4: 통과·타입 확인**

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS (build는 component가 아직 옛 타입을 써서 실패할 수 있다. 실패하면 Task 6에서 잡는다고 적고 test만 PASS 확인)

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): 코멘트 해결됨·응답 turn 타입과 순수 함수, 스크롤 예약 store"
```

---

### Task 6: UI 화면 — widget 버튼, popover 묶음, header, 응답 보기 이동

**Files:**
- Modify: `ui/src/components/CommentWidget.tsx`, `ui/src/components/CommentsPopover.tsx`, `ui/src/components/FileDiff.tsx`, `ui/src/components/WorktreeView.tsx`

**Interfaces:**
- Consumes: Task 5 전부, 기존 `scrollToFile`, `useUiStore.setView`·`selectSession`.
- Produces: `SavedComment` props에 `onResolve: (id: number) => Promise<unknown>`, `onResend: (id: number) => Promise<unknown>`, `replyTargetOf: (c: CommentItem) => ReplyTarget | null`, `onOpenReply: (c: CommentItem, target: ReplyTarget) => void` (모두 참조 안정). `FileDiff`의 `CommentProps`도 같은 네 개를 더해 `CommentThread`·`OutdatedComments`로 내린다. `CommentsPopover` props에 변화 없음 (`comments: CommentItem[]`).

- [ ] **Step 1: 구현**

- `CommentWidget.tsx` `SavedComment`: 전송됨이면 header에 `statusLabel`, `replyTargetOf(comment)`가 있으면 그 `label` 버튼(`onOpenReply`), 버튼 `다시 보내기`(`onResend`) `해결됨`(`onResolve`) `삭제`. 해결됨 코멘트는 `openComments`로 걸러져 widget에 오지 않으므로 미전송·전송됨만 다룬다. 실패는 기존 `error` 줄.
- `CommentsPopover.tsx`: `useCommentActions`의 `resolve`·`unresolve`·`resend`. 목록을 `sortForList` 뒤 해결됨 앞까지와 해결됨으로 나눠, 해결됨은 `<details>` `해결된 코멘트 n개` 안에. 전송됨 row 오른쪽: `replyLabel` 글 + 버튼 `해결됨`·`다시 보내기`; 해결됨 row: `되돌리기`. 버튼은 `onClick={(e) => { e.stopPropagation(); … }}`. header 버튼 글은 `headerLabel(comments)`, 색은 미전송 > 0 파랑, 아니면 응답 > 0 호박(`text-amber-600`), 아니면 회색.
- `FileDiff.tsx`: `CommentProps` 확장, `CommentThread`·`OutdatedComments`에 전달.
- `WorktreeView.tsx`: `const open = useMemo(() => openComments(commentList), [commentList])` → `byPath(open)`; `replyTargetOf = useCallback((c) => replyTarget(c, sessions.data ?? []), [sessions.data])`; `openReply = useCallback((c, t) => { selectSession(worktree.id, t.sessionId); setView(worktree.id, t.view); requestScroll(worktree.id, c.path); }, …)`; `const pending = useNavStore((s) => s.pendingScroll); useEffect(() => { const p = takeScroll(worktree.id, listedPaths); if (p) scrollToFile(p); }, [pending, listedPaths, worktree.id]);`. `FileDiff`에 `onResolve={resolve.mutateAsync}` 등 전달.

- [ ] **Step 2: 타입·test 확인**

Run: `pnpm --dir ui build && pnpm --dir ui test`
Expected: PASS

- [ ] **Step 3: 실제 앱 확인 (screenshot)**

`scripts/fixtures.sh <dir>` → `WTBOARD_CONFIG=<dir>/config.toml pnpm tauri dev`. sqlite3로 fixture worktree에 코멘트 3개: 미전송 1, 전송됨 1(`sent_at`, `sent_to='s1'`), 해결됨 1(`resolved_at`). 확인: header `💬 미전송 1`, 전송됨 widget에 `다시 보내기`·`해결됨`·`삭제`, 해결됨은 diff에 없고 popover `해결된 코멘트 1개` 안에 `해결됨 · HH:MM`과 `되돌리기`. `해결됨` 클릭 → 사라지고 묶음 2개. `되돌리기` → 돌아옴. `다시 보내기` → 미전송(파란), header `💬 미전송 2`. 각 단계 screenshot (`~/.claude/.../memory/tauri-ui-automation.md` 방식).

- [ ] **Step 4: Commit**

```bash
git add ui/src
git commit -m "feat(ui): 전송됨 코멘트의 다시 보내기·해결됨·응답 보기, 목록의 해결됨 묶음"
```

---

### Task 7: 실제 확인(`claude -p`)과 문서, main에 합치기

**Files:**
- Modify: `docs/progress.md`, `CLAUDE.md`

- [ ] **Step 1: 실제 첨부 흐름 확인** (spec 8절 "실제 확인" 1~6)

fixture worktree에서 debug hook으로 (`CLAUDE.md`의 `W=$PWD; cd <fixture worktree> && WTBOARD_CONFIG=… WTBOARD_HOOK_BIN=$W/target/debug/wtboard-hook claude --plugin-dir $W/plugin/wtboard -p "…" --permission-mode acceptEdits`). 앱을 같은 `WTBOARD_CONFIG`로 띄운 채:
1. 코멘트 2개(파일 둘) → `-p "리뷰 코멘트 반영해 줘. 1번 코멘트만 반영하고 2번은 건너뛰어"` → sqlite3로 `sent_turn = 1` 둘 다 → 앱: 1번 `응답 #1 보기`, 2번 없음, header `💬 응답 1`. screenshot.
2. `응답 #1 보기` → 마지막 응답 탭 + 그 파일 위치. screenshot.
3. `해결됨` → 사라짐, popover 묶음. `되돌리기`.
4. 2번 `다시 보내기` → `claude --resume <session id> -p "이제 2번 반영해 줘"` → `sent_turn = 2`, 응답 뒤 `응답 #2 보기`.
5. `WTBOARD_HOOK_BIN=/usr/bin/true pnpm tauri dev` → sidebar 경고 문구 screenshot. debug hook으로 띄우면 경고 없음.
6. fixture `.git/index` mtime 전후 비교.

막히는 항목은 progress에 "확인하지 못한 것"으로 적고 넘어간다.

- [ ] **Step 2: 문서**

`docs/progress.md`: 단계 표에 6행(완료, plan·spec 경로), `## 6단계 결과`(실제 확인 결과·수치), `## 6단계에서 정한 것`(spec의 "내 판단" 항목), `## 6단계 알려진 한계`(파일 단위 판정, `copy`는 응답 없음, 되돌려진 변경도 건드린 것으로 셈, 세션 정리 뒤 `replyTurn` 사라짐, 해결됨 widget 없음, 옛 hook 경고는 시작 때 한 번), "6단계 후보"에서 끝난 항목 제거, **사용자가 할 일: `scripts/install-hook.sh` 다시 실행**. `CLAUDE.md`: `crates/app` command 목록에 `resolve_comment`·`unresolve_comment`·`resend_comment`, `crates/hook`에 `version`, schema `user_version` = 2.

- [ ] **Step 3: 전체 test와 commit**

Run: `cargo test && pnpm --dir ui test && pnpm --dir ui build`
Expected: PASS

```bash
git add docs/progress.md CLAUDE.md
git commit -m "docs: 6단계 결과·정한 것·알려진 한계"
```

- [ ] **Step 4: main에 fast-forward**

```bash
git checkout main && git merge --ff-only stage6-comment-lifecycle && git branch -d stage6-comment-lifecycle
```
