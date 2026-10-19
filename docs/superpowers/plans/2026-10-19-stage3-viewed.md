# 3단계: 봤음 체크 + 다시 바뀜 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 파일마다 "봤음"을 체크해 그 시점 diff hash를 SQLite에 저장하고, diff가 바뀌면 1초 안에 "다시 바뀜"으로 보인다.

**Architecture:** `core::store::viewed::ViewedStore`(SQLite)를 `Registry → Repo → Worktree`로 내려보낸다. `Worktree`는 현재 브랜치의 row(path → 저장 hash)를 들고, row가 있는 파일의 현재 hash를 항상 `State.hashes`에 유지한다. 그러면 1단계의 hash 비교가 체크한 파일에도 걸려 `worktree-changed`가 나가고, UI는 기존 invalidate로 상태를 다시 읽는다.

**Tech Stack:** Rust (`rusqlite` bundled), Tauri 2, React + TanStack Query, vitest.

**Spec:** `docs/superpowers/specs/2026-10-19-stage3-viewed-design.md`

## Global Constraints

- 앱은 레포에 아무것도 쓰지 않는다. DB는 config 파일과 같은 폴더의 `state.sqlite` (기본 `~/.wtboard/state.sqlite`).
- 봤음 key는 `(worktree id, branch, path)`. detached HEAD의 branch는 `''`.
- 상태 값은 serde/TS 모두 `'none' | 'viewed' | 'changed'`.
- `unviewedFiles` = 변경 파일 중 `viewed`가 아닌 수 (접힌 파일 포함).
- 계산을 모두 끝낸 뒤 state를 바꾼다 (git 실패 시 state와 화면이 어긋나지 않게).
- lock 순서: state → meta.
- 테스트를 먼저 쓴다 (실패 확인 → 구현 → 통과). 실제 git, 실제 FSEvents.
- 사용자에게 보이는 문구: `봤음`, `다시 바뀜`, `확인 n/m`, 저장 실패 경고 `봤음 상태를 저장할 수 없음: <오류>`.
- 명령 전에 `source ~/.cargo/env`.

## Review Focus

1. 같은 merge-base를 가진 다른 브랜치로 전환 → `refresh_base`가 "base 그대로"로 빠지지 않고 새 브랜치 row를 읽어야 한다 (Task 3 test `branch_switch_with_same_merge_base_reloads_rows`).
2. 체크한 파일을 revert(목록에서 빠짐) 후 같은 변경을 다시 하면 `viewed`로 돌아온다 (Task 3 test `revert_then_redo_restores_viewed`).
3. 체크하려는 사이 파일이 목록에서 빠짐 → `UnknownFile` 오류, UI는 optimistic update를 되돌린다 (core는 Task 2 test `set_viewed_unknown_file_errors`. UI 되돌리기는 `useSetViewed`의 `onError`이고 hook test 환경이 없으므로 Task 6 Step 3에서 확인).
4. 삭제된 파일(status D)을 체크 → 관련 없는 event 뒤에도 `viewed` 유지 (Task 3 test `viewed_deleted_file_stays_viewed`).
5. `state.sqlite`를 만들 수 없는 경로 → `ViewedStore::open`이 `Err`, 앱은 in-memory로 계속하고 경고를 보인다 (Task 1 test `open_fails_under_a_file`).

---

## File Structure

| 파일 | 할 일 |
|---|---|
| `crates/core/src/store/viewed.rs` (신규) | `ViewedStore`: SQLite 열기, load / set / remove |
| `crates/core/src/worktree.rs` | `ViewedState`, `FileEntry`, row·hash 유지, `entries()`, `set_viewed()`, `unviewed_files` |
| `crates/core/src/repo.rs`, `registry.rs` | `RepoOptions.viewed`, `Registry::open`의 store 인자, `Registry::warn` |
| `crates/app/src/main.rs` | store 열기와 fallback, `list_files`가 `entries()`, `set_viewed` command |
| `ui/src/api.ts`, `ui/src/lib/viewed.ts` (신규), `ui/src/useSetViewed.ts` (신규) | 타입, 순수 함수, mutation hook |
| `ui/src/components/*.tsx` | checkbox, badge, 확인 수, sidebar 숫자 |

---

### Task 1: `ViewedStore`와 연결 배선

동작 변화 없이 store를 만들어 `Worktree`까지 내려보낸다.

**Files:**
- Create: `crates/core/src/store/viewed.rs`, `crates/core/tests/viewed_store.rs`
- Modify: `crates/core/Cargo.toml`, `crates/core/src/store/mod.rs`, `crates/core/src/worktree.rs` (`open`, `open_with`), `crates/core/src/repo.rs` (`RepoOptions`, `open_worktree`), `crates/core/src/registry.rs`, `crates/app/src/main.rs` (`open_registry`)
- Test call sites to update: `tests/repo.rs:20`, `tests/repo_lock.rs:19`, `tests/registry.rs:24`, `tests/watcher.rs:67`, `tests/worktree.rs:230,248,258,268,277`

**Interfaces:**
- Produces:
  - `pub struct ViewedStore` (`#[derive(Debug)]`, 안에 `Mutex<rusqlite::Connection>`)
  - `ViewedStore::open(path: &Path) -> Result<ViewedStore, StoreError>` — 부모 폴더가 없으면 만든다. spec 1절의 `CREATE TABLE IF NOT EXISTS`
  - `ViewedStore::in_memory() -> ViewedStore`
  - `load(&self, worktree: &str, branch: &str) -> Result<HashMap<String, String>, StoreError>` (path → hash)
  - `set(&self, worktree: &str, branch: &str, path: &str, hash: &str) -> Result<(), StoreError>` (upsert, `viewed_at` = 지금 Unix ms)
  - `remove(&self, worktree: &str, branch: &str, path: &str) -> Result<(), StoreError>`
  - `pub enum StoreError { Io(std::io::Error), Sqlite(rusqlite::Error) }` (thiserror, 메시지는 한국어)
  - `Worktree::open_with(path, configured_base, gh, viewed: Arc<ViewedStore>)`. `Worktree::open(path, base)`는 `Gh::disabled()` + `Arc::new(ViewedStore::in_memory())`
  - `RepoOptions { base, gh, poll, viewed: Arc<ViewedStore> }`
  - `Registry::open(config: &Config, gh: Gh, viewed: Arc<ViewedStore>, sink: Sink) -> Self`
  - `Registry::warn(&mut self, e: EntryError)` — 오류 목록 끝에 붙인다

- [ ] **Step 1: Write the failing tests** (`tests/viewed_store.rs`, tempdir 사용)

```rust
#[test] fn set_then_load() // set("w","feat","a.py","h1") → load("w","feat") == {"a.py": "h1"}
#[test] fn set_overwrites_hash() // set h1, set h2 → {"a.py": "h2"}
#[test] fn remove_deletes_row() // set, remove → load가 비어 있음
#[test] fn rows_are_split_by_worktree_and_branch() // ("w","feat"), ("w","main"), ("v","feat")에 각각 set → load마다 자기 row만
#[test] fn survives_reopen() // open(dir/state.sqlite)에 set → drop → 다시 open → load에 남아 있음
#[test] fn open_creates_parent_dir() // open(dir/nested/state.sqlite) 성공
#[test] fn open_fails_under_a_file() // dir/file을 일반 파일로 만들고 open(dir/file/state.sqlite) → Err
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p wtboard-core --test viewed_store`
Expected: 컴파일 실패 (`store::viewed` 없음)

- [ ] **Step 3: Implement `ViewedStore`**

`Cargo.toml`에 `rusqlite = { version = "0.32", features = ["bundled"] }`. `store/mod.rs`에 `pub mod viewed;`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p wtboard-core --test viewed_store`
Expected: 7 passed

- [ ] **Step 5: 배선**

Interfaces의 signature대로 `Worktree::open_with`, `RepoOptions`, `Registry::open`, `Registry::warn`을 바꾸고 test call site에 `Arc::new(ViewedStore::in_memory())`를 넘긴다. `Worktree`는 store를 field로 들고만 있다 (Task 2에서 쓴다).

`main.rs`의 `open_registry`:
- `let state_path = path.with_file_name("state.sqlite");` (`path` = config 경로)
- `ViewedStore::open(&state_path)`가 실패하면 `ViewedStore::in_memory()`를 쓰고, `Registry::open` 뒤에 `reg.warn(EntryError { path: state_path.display().to_string(), message: format!("봤음 상태를 저장할 수 없음: {e}") })`.

- [ ] **Step 6: Run all Rust tests**

Run: `cargo test`
Expected: 전부 PASS (동작 변화 없음)

- [ ] **Step 7: Commit**

```bash
git add -A crates
git commit -m "feat(core): 봤음 상태 저장소(SQLite)를 worktree까지 연결"
```

---

### Task 2: `Worktree` 봤음 상태 — 체크, 해제, 다시 열기

**Files:**
- Modify: `crates/core/src/worktree.rs`
- Create: `crates/core/tests/viewed.rs`

**Interfaces:**
- Consumes: Task 1의 `ViewedStore`, `Worktree::open_with(.., viewed)`
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] pub enum ViewedState { #[serde(rename = "none")] Unviewed, #[serde(rename = "viewed")] Viewed, #[serde(rename = "changed")] Changed }`
  - `#[derive(Debug, Clone, Serialize)] pub struct FileEntry { #[serde(flatten)] pub file: ChangedFile, pub viewed: ViewedState }`
  - `Worktree::entries(&self) -> Vec<FileEntry>` (`files()`와 같은 순서). `files()`는 그대로 둔다
  - `Worktree::set_viewed(&self, path: &str, viewed: bool, hash: Option<String>) -> Result<(), WorktreeError>`
  - `WorktreeSummary.unviewed_files: usize` (`failed()`에서는 0)
  - `WorktreeError::Store(#[from] StoreError)`

상태 규칙: `State`에 `branch_key: String`(현재 branch, detached면 `""`)과 `viewed: HashMap<String, String>`(row)을 둔다. path의 상태 = row 없음 → `Unviewed`, `row == hashes[path]` → `Viewed`, 그 외 → `Changed`.

- [ ] **Step 1: Write the failing tests** (`tests/viewed.rs`)

`feat_repo()`(src/a.py, src/b.py를 main에 커밋 후 feat)를 `tests/worktree.rs`에서 `tests/common/mod.rs`로 옮겨 같이 쓴다. helper `fn state(wt: &Worktree, p: &str) -> ViewedState`.

```rust
#[test] fn unchecked_file_is_none()
  // a.py 수정 → state == Unviewed, summary().unviewed_files == 1
#[test] fn check_with_screen_hash_is_viewed()
  // h = wt.patch("src/a.py").hash; set_viewed("src/a.py", true, Some(h)) → Viewed, unviewed_files == 0
#[test] fn check_without_hash_uses_current_diff()
  // patch를 부르지 않고 set_viewed(.., true, None) → Viewed
#[test] fn check_with_stale_hash_is_changed()
  // set_viewed(.., true, Some("stale".into())) → Changed
#[test] fn uncheck_returns_to_none()
  // 체크 후 set_viewed(.., false, None) → Unviewed
#[test] fn set_viewed_unknown_file_errors()
  // set_viewed("nope.py", true, None) → Err(WorktreeError::UnknownFile)
#[test] fn edit_after_check_is_changed_and_emits()
  // 체크 → write "a = 3\n" (+1/-1 그대로) → handle([File a.py]) == Some(scope File, [a.py]) → Changed
  // → 다시 set_viewed(true, None) → Viewed
#[test] fn checks_survive_reopen()
  // tempdir store: open_with(.., store) → 체크 → drop → 같은 store로 다시 open → Viewed
#[test] fn change_while_closed_is_changed_on_open()
  // 체크 → drop → 파일 수정 → 다시 open → Changed
```

`read_only.rs`에 test 하나 추가: `set_viewed` 후에도 `.git/index` 불변 (기존 test의 index 비교 helper 사용).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p wtboard-core --test viewed`
Expected: 컴파일 실패 (`ViewedState`, `entries`, `set_viewed` 없음)

- [ ] **Step 3: Implement**

- `open_with`: 목록 계산 뒤 `store.load(id, branch_key)`. 읽기에 실패하면 `eprintln!("[wtboard] ...")` 후 빈 map (worktree는 연다). 목록에 있는 row 파일의 hash를 계산해 `hashes`에 넣는다.
- row 파일 hash를 채우는 private helper `fn viewed_hashes(&self, st: &State) -> Result<Vec<(String, String)>, WorktreeError>`: row가 있고 `files`에 있고 `hashes`에 없는 파일만 `file_patch`로 계산해 돌려준다. 호출한 쪽이 결과를 `hashes`에 넣는다. Task 3이 같은 helper를 쓴다.
- `set_viewed(true, hash)`: 목록에 없으면 `UnknownFile`. 현재 hash를 계산해 `hashes`에 넣고, 저장할 hash = `hash.unwrap_or(현재 hash)`. store에 쓴 뒤 `State.viewed`를 바꾼다 (store 실패 시 state 불변).
- `set_viewed(false, _)`: store에서 지우고 `State.viewed`에서 뺀다.
- `summary()`: `unviewed_files`.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p wtboard-core --test viewed --test read_only`
Expected: 전부 PASS

- [ ] **Step 5: Run all Rust tests and commit**

Run: `cargo test` → 전부 PASS

```bash
git add -A crates
git commit -m "feat(core): 파일 봤음 체크와 다시 바뀜 판정"
```

---

### Task 3: 재계산 경로에서 봤음 hash 유지

spec 2절의 불변식을 `refresh_list`, `refresh_base`, `too_many`, 브랜치 전환에서 지킨다.

**Files:**
- Modify: `crates/core/src/worktree.rs` (`refresh_list`, `refresh_base`)
- Test: `crates/core/tests/viewed.rs`

**Interfaces:**
- Consumes: Task 2의 `viewed_hashes`, `ViewedState`, `set_viewed`

- [ ] **Step 1: Write the failing tests**

```rust
#[test] fn list_refresh_keeps_checked_file_tracked()
  // a.py 체크 → .gitignore 수정(목록 전체 재계산 유도) + a.py "a = 3\n" → handle([File .gitignore, File a.py]) → a.py Changed
#[test] fn too_many_event_paths_still_catch_checked_file()
  // a.py 체크 → a.py "a = 3\n" → handle(a.py + "x/0.py".."x/200.py" 201개 File) → 결과 event에 a.py, a.py Changed
#[test] fn base_refresh_recomputes_checked_hashes()
  // a.py 체크 → main에 무관한 커밋(c.py)을 추가하고 feat에 merge → handle([Base]) 뒤 a.py Viewed (diff 그대로)
#[test] fn branch_switch_restores_each_branch_checks()
  // feat에서 a.py 수정·커밋·체크 → main에서 feat2 만들어 b.py 수정·커밋 → handle([Base]) → a.py는 목록에 없음, b.py Unviewed
  // b.py 체크 → checkout feat → handle([Base]) → a.py Viewed
#[test] fn branch_switch_with_same_merge_base_reloads_rows()
  // feat에서 a.py 수정(커밋 안 함) → 체크 → `git checkout -q -b feat2` (작업 트리 그대로, merge-base 같음)
  // → handle([Base]) → 반환 event scope == Base, a.py Unviewed
#[test] fn revert_then_redo_restores_viewed()
  // 체크 → a.py 원래대로 → handle → 목록에 없음 → 같은 수정 다시 → handle → Viewed
#[test] fn viewed_deleted_file_stays_viewed()
  // b.py 삭제 → 체크 → a.py 수정 → handle([File a.py]) 및 handle([List]) 뒤에도 b.py Viewed
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p wtboard-core --test viewed`
Expected: 새 test 중 `list_refresh_keeps_checked_file_tracked`, `base_refresh_recomputes_checked_hashes`, `branch_switch_*`가 FAIL (hash가 지워진 채로 남거나 이전 브랜치 row를 씀). 이미 통과하는 test는 그대로 둔다.

- [ ] **Step 3: Implement**

- `refresh_list`: 바뀐 경로의 hash를 지운 뒤 `viewed_hashes`로 다시 채운다.
- `refresh_base`: 맨 앞에서 현재 branch key를 구한다. 이전 key와 다르면 새 key의 row를 읽고, base·merge-base가 같아도 "unchanged"로 빠지지 않고 전체를 바꾼 뒤 Base event를 낸다. 새 `State`를 만들 때 row 파일 hash를 채운 뒤 교체한다.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p wtboard-core --test viewed --test worktree`
Expected: 전부 PASS

- [ ] **Step 5: Run all Rust tests and commit**

Run: `cargo test` → 전부 PASS

```bash
git add -A crates
git commit -m "feat(core): 목록·base 재계산과 브랜치 전환에서 봤음 상태 유지"
```

---

### Task 4: Tauri command와 watcher 통합

**Files:**
- Modify: `crates/app/src/main.rs`
- Test: `crates/core/tests/watcher.rs`

**Interfaces:**
- Consumes: `Worktree::entries`, `Worktree::set_viewed`
- Produces (UI가 쓰는 IPC):
  - `list_files(worktreeId, view) -> FileEntry[]` (camelCase, `viewed` 필드 추가)
  - `set_viewed(worktreeId: String, path: String, viewed: bool, hash: Option<String>) -> Result<(), String>` (spec 3절 그대로. 봤음은 view와 무관하다)

- [ ] **Step 1: Write the failing watcher test**

```rust
#[test]
fn edit_to_checked_file_flips_to_changed_within_one_second()
  // README.md 수정 → Worktree::open → set_viewed("README.md", true, None) → spawn_watcher
  // → 300ms 대기 → README.md 다시 수정 (+/- 수 같게) → 1초 안에 event(File, [README.md])
  // → wt.entries()에서 README.md의 viewed == ViewedState::Changed
```

`start()`는 `Worktree`를 감추므로 이 test는 `Worktree`를 직접 열고 `spawn_watcher(wt.clone(), ..)`를 부른다.

- [ ] **Step 2: Run it**

Run: `cargo test -p wtboard-core --test watcher edit_to_checked_file`
Expected: PASS (Task 2·3으로 이미 동작해야 한다. FAIL이면 Task 3 불변식이 watcher 경로에서 깨진 것이므로 거기서 고친다)

- [ ] **Step 3: command 추가**

`list_files`가 `entries()`를 돌려주고, `set_viewed` command를 `generate_handler!`에 등록한다.

- [ ] **Step 4: Build and commit**

Run: `cargo build -p wtboard-app && cargo test`
Expected: 성공, 전부 PASS

```bash
git add -A crates
git commit -m "feat(app): set_viewed command, list_files에 봤음 상태"
```

---

### Task 5: UI 타입, 순수 함수, mutation hook

**Files:**
- Modify: `ui/src/api.ts`, `ui/src/lib/fixtures.ts`
- Create: `ui/src/lib/viewed.ts`, `ui/src/lib/viewed.test.ts`, `ui/src/useSetViewed.ts`

**Interfaces:**
- Consumes: Task 4 IPC
- Produces:
  - `api.ts`: `export type ViewedState = 'none' | 'viewed' | 'changed'`; `ChangedFile.viewed: ViewedState`; `WorktreeSummary.unviewedFiles: number`; `setViewed(worktreeId: string, path: string, viewed: boolean, hash: string | null) => Promise<void>`
  - `lib/viewed.ts`:
    - `viewedProgress(files: ChangedFile[]): { viewed: number; total: number }`
    - `initiallyOpen(file: ChangedFile): boolean` — `!file.collapsed && file.viewed !== 'viewed'`
    - `nextOpen(prev: ViewedState, next: ViewedState, open: boolean): boolean` — 상태가 바뀌었을 때 펼침 여부
    - `withViewed(files: ChangedFile[], path: string, viewed: boolean): ChangedFile[]` — optimistic update용. 해당 파일만 `'viewed'` 또는 `'none'`
    - `countLabel(w: WorktreeSummary): string` — `unviewedFiles > 0 ? \`${unviewedFiles}/${changedFiles}\` : \`${changedFiles}\``
  - `useSetViewed(worktreeId: string): (file: ChangedFile, viewed: boolean, hash: string | null) => void` — `useMutation`. `onMutate`: `['files', worktreeId, 'branch']` query를 cancel하고 `withViewed`로 바꾼 뒤 이전 값 보관. `onError`: 이전 값으로 되돌리고 `console.error`. `onSettled`: `['files', worktreeId]`, `['repos']` invalidate

- [ ] **Step 1: Write the failing tests** (`lib/viewed.test.ts`)

```ts
viewedProgress: [none, viewed, changed] → { viewed: 1, total: 3 }; [] → { viewed: 0, total: 0 }
initiallyOpen: none → true, viewed → false, changed → true, collapsed+none → false
nextOpen:
  ('none','viewed', true) → false   // 체크하면 접힌다
  ('viewed','none', false) → true   // 해제하면 펼친다
  ('viewed','changed', false) → true // 다시 바뀌면 펼친다
  ('changed','viewed', true) → false
  ('none','none', false) → false    // 그대로면 현재 값 유지
  ('changed','none', true) → true   // 해제는 펼침 유지
withViewed: 대상 파일만 바뀌고 나머지 객체는 같은 참조 (memo 유지), true → 'viewed', false → 'none'
countLabel: unviewed 3, changed 14 → '3/14'; unviewed 0, changed 14 → '14'
```

`fixtures.ts`의 `worktreeFixture`에 `unviewedFiles: 0` 추가, test용 `fileFixture(overrides: Partial<ChangedFile>): ChangedFile` 추가 (`viewed: 'none'`, `collapsed: false`).

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm --dir ui test`
Expected: `viewed.test.ts` FAIL (모듈 없음)

- [ ] **Step 3: Implement `lib/viewed.ts`, `api.ts` 변경, `useSetViewed.ts`**

- [ ] **Step 4: Run tests and type check**

Run: `pnpm --dir ui test && pnpm --dir ui build`
Expected: 전부 PASS, build 성공

- [ ] **Step 5: Commit**

```bash
git add ui/src
git commit -m "feat(ui): 봤음 상태 타입과 optimistic update hook"
```

---

### Task 6: UI 화면 — checkbox, badge, 확인 수, sidebar

**Files:**
- Modify: `ui/src/components/FileDiff.tsx`, `FileTree.tsx`, `WorktreeView.tsx`, `Sidebar.tsx`

**Interfaces:**
- Consumes: Task 5의 `initiallyOpen`, `nextOpen`, `viewedProgress`, `countLabel`, `useSetViewed`

spec 4절의 모양대로:
- **FileDiff**: `useState(initiallyOpen(file))`. 이전 `file.viewed`를 `useRef`로 들고, 바뀌면 `setOpen(nextOpen(prev, next, open))`. header 오른쪽에 `<label>` + checkbox `봤음` (클릭은 `stopPropagation`). 체크 시 `patch.data?.hash ?? null`을 넘긴다. `changed`면 header에 amber `다시 바뀜` badge. `FileDiff`는 `onSetViewed`를 prop으로 받고 `WorktreeView`가 `useSetViewed(worktree.id)`를 한 번 만들어 넘긴다 (memo가 깨지지 않게 hook 반환값은 안정적인 함수여야 한다: `useCallback`으로 `mutate`를 감싼다).
- **FileTree**: status 글자 앞에 폭 고정 칸 하나: `viewed` → 회색 `☑`, `changed` → amber `●`, `none` → 빈칸. `viewed`면 이름을 `text-gray-400`.
- **WorktreeView header**: worktree 라벨 뒤, split/unified 앞에 `확인 {viewed}/{total}`. `total > 0 && viewed === total`이면 `text-green-600`, 아니면 `text-gray-500`.
- **Sidebar**: 숫자 자리에 `countLabel(w)`. `unviewedFiles === 0`이면 지금처럼 회색.

- [ ] **Step 1: Implement the four components**

- [ ] **Step 2: Type check and tests**

Run: `pnpm --dir ui build && pnpm --dir ui test`
Expected: 성공, 전부 PASS

- [ ] **Step 3: 실제 앱 확인 (memory: verify-real-ui, tauri-ui-automation)**

```bash
scripts/fixtures.sh "$SCRATCH/fx"
WTBOARD_CONFIG="$SCRATCH/fx/config.toml" pnpm tauri dev
```

순서대로 screenshot으로 확인:
1. 변경 파일 하나 체크 → 그 diff가 접히고, tree에 ☑, header `확인 1/n`, sidebar `n-1/n`.
2. 터미널에서 그 파일 수정 → 1초 안에 diff가 펼쳐지고 `다시 바뀜` badge, tree에 ●, header `확인 0/n`.
3. 다시 체크 → 1번 상태.
4. 접힌 생성 파일을 펼치지 않고 체크 → `확인` 수 증가.
5. 앱 종료 후 재실행 → 체크 유지. `$SCRATCH/fx/state.sqlite`가 생기고 `~/.wtboard/state.sqlite`는 건드리지 않음.
6. 체크할 때 core가 실패하게 만든 뒤(체크 직전 그 파일을 `git checkout`으로 되돌려 목록에서 빠지게) 체크 → 화면이 되돌아가고 console에 오류.
7. fixture 레포의 `git status`, `.git/index` mtime이 확인 전후 같음.

- [ ] **Step 4: Commit**

```bash
git add ui/src
git commit -m "feat(ui): 봤음 checkbox, 다시 바뀜 badge, 확인 수"
```

---

### Task 7: 마무리

**Files:**
- Modify: `docs/progress.md`, `CLAUDE.md` (구조 줄에 `store/`의 `viewed`, command `set_viewed` 추가)

- [ ] **Step 1: 전체 검증**

Run: `cargo test && pnpm --dir ui test && pnpm --dir ui build`
Expected: 전부 PASS

- [ ] **Step 2: 실제 `~/.wtboard/config.toml`로 읽기만 하며 확인**

`pnpm tauri dev`로 띄워 체크 하나, 해제 하나. 실제 레포의 `git status`와 `.git/index`가 실행 전후로 같은지 확인한다.

- [ ] **Step 3: `progress.md` 갱신**

단계 표의 3단계를 `완료 (날짜)` + plan·spec 경로로, 4단계를 `다음`으로. "3단계 결과", "3단계에서 정한 것 (spec과 다른 점)" 절을 2단계 형식으로 추가. "3단계로 넘어간 것"에서 hash 범위 항목을 지우고, 남은 두 항목(`upstream` PR base, summary cache)은 "4단계로 넘어간 것"으로 옮긴다.

- [ ] **Step 4: Commit**

```bash
git add docs/progress.md CLAUDE.md
git commit -m "docs: 3단계 완료, 진행 상황 갱신"
```
