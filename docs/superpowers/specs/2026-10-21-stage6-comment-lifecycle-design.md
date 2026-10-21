# 6단계 설계: 전송 뒤 코멘트의 생애 (다시 보내기 · 응답 연결 · 해결됨)

상위 설계: `docs/draft.md` 11절, 5단계 spec `2026-10-19-stage5-review-comments-design.md`. 이 문서는 5단계 위에 더하는 것만 적는다.

사용자는 질문 없이 바로 진행하기로 했다. "(내 판단)"으로 표시한 것은 설계자가 정한 것이다.

## 왜

5단계의 리뷰 loop는 "코멘트 → 다음 prompt에 첨부 → 전송됨"에서 끝난다. Claude가 답한 뒤 사용자가 알고 싶은 것은 코멘트마다 "Claude의 응답이 이 코드를 건드렸나, 그 응답 diff는 어디서 보나, 확인했으면 치우고, 무시됐으면 다시 보낸다"인데 지금은 전송됨 코멘트가 회색으로 남아 있을 뿐이고, 응답이 그 줄을 고치면 "위치가 바뀐 코멘트" 묶음으로 밀려 들어간다. 다시 보내는 길은 같은 글을 새로 다는 것뿐이다.

## 목표와 완료 기준

- 전송된 코멘트마다 그 코멘트를 받은 세션의 응답(turn)이 그 파일을 고쳤는지 보인다 (`응답 #n`). 누르면 그 응답의 diff(마지막 응답 또는 이번 세션 탭)로 가서 그 파일로 스크롤한다.
- 전송된 코멘트를 `해결됨`으로 바꾸면 diff에서 사라지고 코멘트 목록의 "해결된 코멘트" 묶음에만 남는다. `되돌리기`로 돌아온다.
- 전송된 코멘트를 `다시 보내기`하면 미전송이 되어 다음 prompt에 다시 붙는다.
- 이번 schema 변경(user_version 2)으로 옛 hook binary가 첨부를 멈추므로, 앱이 옛 binary를 감지해 sidebar 경고로 알린다.

완료 기준 (실제 확인): fixture에서 코멘트 2개 → `claude -p`로 첨부 → Claude가 한 줄만 고침 → 앱에서 그 코멘트에 `응답 #1`이 보이고 다른 코멘트에는 안 보인다 → `응답 보기`가 마지막 응답 탭의 그 파일로 간다 → `해결됨` → diff에서 사라지고 목록 묶음에 보인다 → 안 고친 코멘트 `다시 보내기` → `claude --resume`에 다시 붙고 `sent_turn = 2`.

범위 밖 (내 판단):
- 줄 단위 "응답이 이 줄을 고쳤다" 판정. 파일 단위(그 turn의 Edit/Bash event에 그 path가 있다)만 한다. 줄이 바뀌었는지는 기존 위치 맞추기(snippet 비교)가 이미 보여 준다.
- "해결됨" 자동 판정, Claude 응답 텍스트 연결, 답글(thread), 여러 줄 범위·파일 단위 코멘트.
- hook binary를 앱 번들에 넣기. 감지와 안내까지만 한다.
- 일괄 해결(`모두 해결됨`). `전송된 코멘트 지우기`가 일괄 정리를 맡는다.

## 1. 구성

| 단위 | 하는 일 |
|---|---|
| `core::store::db` (수정) | schema 2: `comments`에 `sent_turn`, `resolved_at`. migration을 version 순서대로 돌린다 |
| `core::store::comments` (수정) | `Comment.sent_turn`, `resolved_at`; `resolve`, `unresolve`, `resend`; 목록 순서 3단 |
| `core::review` (수정) | `attach`가 `sent_turn`을 기록한다. 새 `reply_turns`: 코멘트별 "응답 turn" 계산 |
| `core::session::list` (수정) | `touched_turns`: worktree 안 path별로 건드린 turn 집합 (기존 `touched` 위에) |
| `crates/hook` (수정) | `prompt-submit`이 여는 turn 번호를 첨부에 넘긴다. 새 subcommand `version` |
| `crates/app` (수정) | command `resolve_comment`, `unresolve_comment`, `resend_comment`; `list_comments`가 `replyTurn`을 붙인다; 시작 때 hook binary version 확인 → 경고 |
| `ui/` (수정) | 전송됨 widget의 `응답 #n 보기`·`다시 보내기`·`해결됨`, 목록 popover의 해결됨 묶음과 row 버튼, header `💬 미전송 a · 응답 b`, 다른 view로 이동 후 파일 스크롤 |

## 2. 저장

### `state.sqlite` schema 2

```sql
ALTER TABLE comments ADD COLUMN sent_turn   INTEGER;  -- 첨부한 prompt가 연 turn. 복사(copy)면 NULL
ALTER TABLE comments ADD COLUMN resolved_at INTEGER;  -- NULL = 해결되지 않음 (Unix ms)
PRAGMA user_version = 2;
```

- `migrate`는 `version < SCHEMA_VERSION`인 동안 `MIGRATIONS[version]`을 차례로 돌린다 (0 → 1 → 2). 빈 파일은 1, 2를 이어서 받는다.
- hook의 `open_existing`은 지금처럼 version이 `SCHEMA_VERSION`과 다르면 열지 않는다 (작으면 `None`, 크면 `NewerSchema`). 옛 hook(1)이 새 DB(2)를 만나면 `NewerSchema(2)`로 첨부하지 않는다. 그래서 앱이 옛 binary를 감지한다 (4절).

### 코멘트 상태

| 상태 | 조건 | 할 수 있는 것 |
|---|---|---|
| 미전송 | `sent_at IS NULL` | 수정, 삭제. 다음 prompt에 첨부 |
| 전송됨 | `sent_at IS NOT NULL AND resolved_at IS NULL` | 다시 보내기, 해결됨, 삭제, 응답 보기(응답 turn이 있을 때) |
| 해결됨 | `resolved_at IS NOT NULL` (항상 `sent_at`도 있음) | 되돌리기, 삭제 |

`CommentStore`:

| 함수 | 동작 |
|---|---|
| `resolve(id, now)` | 전송됨만. `WHERE id AND sent_at IS NOT NULL AND resolved_at IS NULL`. 미전송이면 `StoreError::NotSent`, 이미 해결됨이면 그대로 성공, 없으면 `NotFound` |
| `unresolve(id)` | `resolved_at = NULL`. 없으면 `NotFound`. 해결 안 된 것이면 그대로 성공 |
| `resend(id, now)` | `sent_at = NULL, sent_to = NULL, sent_turn = NULL, resolved_at = NULL, updated_at = now`. 이미 미전송이면 그대로 성공. 없으면 `NotFound` |
| `list(worktree)` | 미전송 → 전송됨 → 해결됨, 그 안에서 `path, line, id` (`ORDER BY resolved_at IS NOT NULL, sent_at IS NOT NULL, path, line, id`) |
| `mark_sent(ids, "copy", now)` | 그대로 (`sent_turn`은 NULL) |
| `clear_sent`, `purge_sent_before`, `unsent_counts`, `unsent_in` | 그대로. 해결됨은 전송됨의 부분집합이라 지우기·정리에 같이 들어간다 |

`review::attach(conn, worktree, session_id, turn, now, emit)`: `sent_turn = turn`을 같이 쓴다.

## 3. "응답 turn" 계산

코멘트 하나의 `reply_turn` = `sent_to` 세션의 event 중 `turn >= sent_turn`이고 이 worktree의 `path`를 건드린 (`Edit`·`Bash`) event가 있는 가장 큰 turn. 없으면 `None`.

- `sent_to`가 `copy`이거나 `sent_turn`이 NULL이거나 세션 폴더가 없으면(정리됨) `None`.
- turn 0 event는 기존 `touched`처럼 turn 1로 본다. 진행 중인 turn도 센다 (화면은 `진행 중`을 세션 목록에서 안다).
- 되돌려진 변경(before == after)도 건드린 것으로 센다 (알려진 한계).

`core::session::list::touched_turns(dir, worktree_root, all_roots) -> HashMap<String, BTreeSet<u32>>` (rel path → turn 집합). `core::review::reply_turns(data_dir, worktree_root, all_roots, comments) -> HashMap<i64, u32>`는 세션별로 event를 한 번만 읽는다.

앱의 `list_comments`가 호출마다 계산한다. 세션 하나의 `events.jsonl` 읽기라 싸다. turn이 끝나면 `session-updated` → comments invalidate → 다시 계산되어 `응답 #n`이 저절로 나타난다 (5단계 event 흐름 그대로).

## 4. hook

- `prompt-submit`: 첨부가 turn 기록보다 먼저 도는데(`main.rs`), 이 prompt가 여는 turn은 `read_meta().turn + 1` (meta가 없으면 1)이다. `comments::attach`에 `SessionDir`를 넘겨 그 값을 `review::attach`의 `turn`으로 쓴다. `turn::prompt_submit`이 같은 계산으로 `meta.turn`을 올리므로 둘은 같다.
- 새 subcommand `version`: stdin을 읽지 않고 stdout에 `wtboard-hook <CARGO_PKG_VERSION> schema <SCHEMA_VERSION>` 한 줄, exit 0. 옛 binary는 모르는 subcommand라 stdout이 비고 stderr 한 줄이다 (그래서 앱은 "stdout에 `schema N`이 없음 = 옛 binary"로 본다).

### 앱의 옛 binary 감지 (내 판단)

- binary 경로는 run.sh와 같다: `WTBOARD_HOOK_BIN`, 없으면 `~/.wtboard/bin/wtboard-hook`.
- `open_registry`(background thread)에서 한 번: 파일이 없으면 아무 말 없음 (hook을 안 쓰는 사용자). 있으면 `<bin> version`을 stdin `null`로 실행해 stdout에서 `schema N`을 찾는다. `N != SCHEMA_VERSION`이거나 못 찾으면 `reg.warn(EntryError { path: <bin>, message })`:
  `hook binary가 옛 version입니다 (schema 2 필요). scripts/install-hook.sh를 다시 실행하세요. 그때까지 리뷰 코멘트가 첨부되지 않습니다`
- 실행 실패(권한 등)도 같은 경고에 오류를 덧붙인다. 앱은 이 확인 말고는 hook을 실행하지 않는다.

## 5. 앱 command

| command | 동작 |
|---|---|
| `list_comments(worktree_id)` | `CommentItem[]` = `Comment` 필드 전부(flatten) + `replyTurn: number \| null` |
| `resolve_comment(id)` | `resolve`. 미전송이면 오류 "아직 전송되지 않은 코멘트는 해결됨으로 바꿀 수 없습니다" |
| `unresolve_comment(id)` | `unresolve` |
| `resend_comment(id)` | `resend` |

기존 command는 그대로. `WorktreeSummary.unsentComments`도 그대로 (sidebar는 미전송만 보인다. 응답 수는 worktree마다 세션을 읽어야 해서 넣지 않는다, 내 판단).

## 6. UI

### 전송됨 widget (줄 아래, "위치가 바뀐 코멘트" 묶음 공통)

```
┌─ 전송됨 · 14:05   응답 #3 보기        다시 보내기  해결됨  삭제 ┐
│ 페이지네이션이 필요합니다.                                      │
└────────────────────────────────────────────────────────────────┘
```

- `응답 #n 보기`는 `replyTurn`이 있고 그 세션(`sentTo`)이 이 worktree의 세션 목록에 있을 때만 보인다 (목록에 없는 세션은 `pickSession`이 첫 세션으로 바꿔 버려서 이동이 되지 않는다). 세션 목록의 그 세션이 `turnOpen`이고 `turn === replyTurn`이면 `응답 #n 진행 중 보기`.
- 누르면: `selectSession(worktreeId, sentTo)`, view는 `replyTurn === session.turn`이면 `turn`(마지막 응답), 아니면 `session`(이번 세션; 그 turn만 보는 화면이 없다), 그리고 그 파일로 스크롤 예약.
- `다시 보내기`·`해결됨`·`삭제`는 확인 없이 바로 (삭제와 같다). 실패는 widget 아래 빨간 글.
- 해결됨 코멘트는 diff에 그리지 않는다 (`WorktreeView`가 `byPath`에 넘기기 전에 거른다). `isWriting`은 영향 없다 (수정은 미전송만).

### 코멘트 목록 popover

- 순서: 미전송 → 전송됨 → `<details>` "해결된 코멘트 n개" (처음엔 닫힘).
- 전송됨 row 오른쪽에 `응답 #n`(있으면)과 버튼 `해결됨` `다시 보내기`. 해결됨 row에 `되돌리기`. row 자체는 지금처럼 파일로 스크롤 (버튼은 `stopPropagation`).
- 상태 글: `해결됨 · HH:MM` (`resolvedAt`).
- `전송된 코멘트 지우기`는 그대로 (해결됨 포함).

### header 버튼

`💬 미전송 a · 응답 b` — `a` = 미전송 수(파란), `b` = 전송됨(해결 안 됨) 중 `replyTurn`이 있는 수(호박색). 0인 쪽은 뺀다. 둘 다 0이면 `💬` 회색. `title`에 풀어서 쓴다.

### 다른 view로 이동한 뒤 스크롤

`select-worktree`처럼 즉시 스크롤할 수 없다 (파일 목록이 query로 늦게 온다). 저장하지 않는 store(`ui/src/nav.ts`)에 `pendingScroll: { worktreeId, path } | null`을 두고, `WorktreeView`가 `listedPaths`에 그 path가 들어오면 `scrollToFile` 후 지운다. 다른 worktree로 바뀌면 그 worktree의 요청만 남는다. (파일이 접혀 있으면 header까지만 스크롤한다. 펼치지 않는다, 내 판단.)

## 7. 오류 처리와 경계 사례

- `resolve`·`resend`가 hook의 첨부 transaction과 겹침: hook은 `sent_at IS NULL`만 고르므로 `resend` 직후 prompt가 오면 바로 붙는다 (의도). `resolve`는 전송됨 row만 바꿔 hook과 부딪히지 않는다.
- 다시 보낸 코멘트는 `sent_turn`·`sent_to`가 새로 찍히고 `replyTurn`은 새 세션·turn으로 다시 계산된다.
- 세션 기록이 `keep_days`로 지워지면 `replyTurn`이 사라진다. 코멘트는 남는다.
- 같은 worktree 세션이 둘이면 `sent_to`가 받은 세션이고 `replyTurn`도 그 세션에서만 본다.
- 코멘트가 `copy`로 전송됐으면 `replyTurn`은 늘 없다 (어느 세션에 붙였는지 모른다).
- 앱이 메모리 DB를 쓰는 동안(열기 실패) 해결·재전송도 메모리에만 남는다 (5단계와 같은 경고).
- schema 2 DB를 1단계 앱(이 commit 이전)이 열면 `NewerSchema(2)`로 메모리 DB + 경고 (기존 동작).

## 8. test

test를 먼저 쓴다. 실제 SQLite 파일, 실제 세션 폴더, 실제 binary.

**core**
1. db: 빈 파일 → version 2에 두 column. version 1 파일(코멘트 row 있음) → 2로 올라가고 row와 `sent_at`·`sent_to` 유지, 새 column NULL. version 0 → 2. 3 → `NewerSchema`. hook `open_existing`: 1이면 `None`, 2면 열림, 3이면 오류.
2. `CommentStore`: `resolve` 규칙(미전송 `NotSent`, 해결됨 idempotent, 없음 `NotFound`), `unresolve`, `resend`(네 column 초기화·`updated_at` 갱신·미전송에 idempotent), 목록 3단 순서, `clear_sent`·`purge_sent_before`가 해결됨도 지움, `unsent_in`이 재전송한 것을 다시 고름.
3. `review::attach`가 `sent_turn`을 쓴다. `mark_sent("copy")`는 NULL.
4. `session::list::touched_turns`: Edit·Bash event, turn 0 → 1, 다른 worktree 경로 제외, 안쪽 worktree 경로 제외.
5. `review::reply_turns`: sent_turn 이후 turn만, 가장 큰 turn, `copy`·세션 없음·`sent_turn` NULL → 없음, 세션 둘(코멘트마다 자기 세션), 세션별 event 읽기 1회(계산 결과로만 검증).

**hook**
1. 첫 prompt → `sent_turn = 1`, `--resume` 두 번째 prompt(meta turn 1) → 새 코멘트 `sent_turn = 2`. 둘 다 `meta.turn`과 같다.
2. `version` → stdout `wtboard-hook <ver> schema 2` 한 줄, exit 0, stdin을 안 읽어도 끝난다(stdin null).
3. 기존 test 그대로 통과 (schema 2 DB를 `seed_comment`가 만든다).

**app**: `hook::parse_schema`(옛 binary의 빈 stdout, 정상 출력, 쓰레기), `hook::check`가 없는 파일에 `None`, `/usr/bin/true`에 경고, 실제 `wtboard-hook`(test bin)에 `None`. `reply_turn`을 붙인 `CommentItem` serialize key(`replyTurn`).

**UI (`pnpm --dir ui test`)**: `statusLabel`(해결됨), `sortForList`(3단), `openComments`(해결됨 제외), `headerLabel`(a·b 조합 4가지), `replyTarget`(세션 목록에 없음 → null, `turn === replyTurn` → `turn`, 아니면 `session`, 진행 중 문구), nav store(`pendingScroll` 소비·다른 worktree 요청 무시), `commentFixture` 새 필드 기본값.

**실제 확인** (fixture, `WTBOARD_CONFIG`, debug hook `WTBOARD_HOOK_BIN`)
1. 코멘트 2개 (한 파일에 하나씩) → `claude -p "리뷰 코멘트 반영해 줘. 첫 번째 코멘트만 반영하고 두 번째는 건너뛰어"` → DB `sent_turn = 1` → 앱: 첫 코멘트 `응답 #1`, 둘째는 없음. header `💬 응답 1`. screenshot.
2. `응답 #1 보기` → 마지막 응답 탭 + 그 파일로 스크롤. screenshot.
3. `해결됨` → diff에서 사라짐, popover "해결된 코멘트 1개" 안에 `해결됨 · HH:MM`. `되돌리기` → 돌아옴.
4. 둘째 `다시 보내기` → 미전송 → `claude --resume <id> -p "이제 반영해 줘"` → `sent_turn = 2`, 응답 뒤 `응답 #2`.
5. `WTBOARD_HOOK_BIN=/usr/bin/true pnpm tauri dev` → sidebar 경고 문구. 실제 debug hook이면 경고 없음.
6. fixture `.git/index` mtime 전후 동일.

## 9. 작업 방식

- branch `stage6-comment-lifecycle`에서 하고 끝나면 `main`에 fast-forward (5단계와 같다).
- 사용자 문구 (그대로): `다시 보내기`, `해결됨`, `되돌리기`, `응답 #n 보기`, `응답 #n 진행 중 보기`, `응답 #n`, `해결됨 · HH:MM`, `해결된 코멘트 n개`, `💬 미전송 a · 응답 b`, `아직 전송되지 않은 코멘트는 해결됨으로 바꿀 수 없습니다`, `hook binary가 옛 version입니다 (schema 2 필요). scripts/install-hook.sh를 다시 실행하세요. 그때까지 리뷰 코멘트가 첨부되지 않습니다`.
- 끝나면 `docs/progress.md`에 6단계 행·결과·정한 것·알려진 한계를 적고, `CLAUDE.md`의 command 목록과 hook subcommand를 갱신하고, 사용자가 `scripts/install-hook.sh`를 다시 실행해야 한다는 것을 progress에 적는다.
