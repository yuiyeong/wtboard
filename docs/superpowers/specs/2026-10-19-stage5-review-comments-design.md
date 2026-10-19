# 5단계 설계: 리뷰 코멘트 자동 첨부 + 메뉴바

상위 설계: `docs/draft.md` (5절 hook 표, 10절, 11절, 12절, 13절). 이 문서는 5단계에서 draft와 달라진 점과 구체 설계를 적는다.

사용자는 질문 없이 바로 진행하기로 했다. "(내 판단)"으로 표시한 것은 설계자가 정한 것이다. hook 규격은 2026-10-19 Claude Code 공식 문서(https://code.claude.com/docs/en/hooks)로 확인했다.

## 목표와 완료 기준

- diff의 줄에 리뷰 코멘트를 달 수 있다. 코멘트는 worktree별로 `state.sqlite`에 "미전송"으로 저장된다.
- 그 worktree에서 Claude Code에 다음 prompt를 보내면 미전송 코멘트가 `파일:줄 + 코드 + 코멘트` 형태로 Claude의 맥락에 붙고 "전송됨"이 된다. Claude가 그 코멘트를 반영한다 (draft 13절 완료 기준).
- 자동 첨부를 끌 수 있고(`[review] auto_attach_comments`), 끈 사람은 "마크다운으로 복사"로 직접 붙여 넣는다.
- Mac 메뉴바에 전체 미확인 파일 수가 보이고, 메뉴에서 worktree를 바로 연다. 창을 "항상 위에" 띄울 수 있다.

범위 밖 (내 판단):
- 여러 줄 범위 코멘트, 파일 단위 코멘트, 답글(thread). 한 줄에 코멘트 하나 이상은 달 수 있다.
- Claude의 답을 코멘트에 연결하기 ("해결됨" 자동 판정).
- `progress.md`의 "5단계로 넘어간 것" 4개 (`upstream` PR base, 일반 summary cache, hook binary 번들, 4단계 보류 수정). 6단계 후보로 다시 적는다. 단 메뉴바 갱신이 summary 비용을 키우지 않게 하는 것은 이번 범위다 (4절).
- 알림(notification center).

## draft와 달라진 점

| draft | 5단계 | 이유 |
|---|---|---|
| `add_comment(worktree_id, path, line, body)` | `side`(old/new), `view`, `snippet`도 받는다. 수정·삭제·목록 command 추가 | 삭제된 줄에도 달 수 있어야 하고, 파일이 바뀐 뒤에도 코멘트가 가리키던 코드를 보여야 한다 |
| 코멘트 상태: 미전송 → 전송됨 | 전송됨에 `sent_to`(session id 또는 `copy`)와 시각을 남긴다. "마크다운으로 복사"도 전송됨으로 바꾼다 | 복사해서 붙여 넣은 코멘트가 다음 prompt에 또 붙지 않게 (내 판단) |
| 첨부 크기 제한 없음 | 한 번에 9,000자까지. 넘치는 코멘트는 미전송으로 남아 다음 prompt에 붙는다 | hook 출력이 10,000자를 넘으면 Claude Code가 파일로 빼고 미리보기만 넣는다 (Claude는 그 파일을 읽지 않는다) |
| 메뉴바 아이콘에 미확인 수 | 아이콘 옆 숫자(title) + 메뉴에 worktree 목록 + "항상 위에" 토글 | 메뉴바에서 바로 worktree로 가는 길 (내 판단) |

## 1. 구성

| 단위 | 하는 일 |
|---|---|
| `core::store` (수정) | `state.sqlite` 열기를 한 곳으로 모은다: WAL, `user_version`, schema 1. `ViewedStore`는 그대로 쓰고 `CommentStore`, `SettingsStore`를 추가 |
| `core::review` (신규) | 코멘트 → 첨부 텍스트(markdown) 만들기. hook과 앱("마크다운으로 복사")이 같은 함수를 쓴다 |
| `crates/hook` (수정) | `prompt-submit`이 미전송 코멘트를 stdout JSON(`additionalContext`)으로 내보내고 전송됨으로 바꾼다 |
| `crates/app` (수정) | 코멘트 command, `WorktreeSummary.unsentComments`, tray(메뉴바), 창 닫기 = 숨기기, 항상 위에 |
| `ui/` (수정) | 줄 코멘트 작성·표시, 코멘트 목록과 복사, sidebar 💬 수, `select-worktree` event |

## 2. 저장

### `state.sqlite` schema

열 때마다 `PRAGMA journal_mode=WAL`, `busy_timeout`(앱 5초, hook 1초). 앱과 hook이 같은 파일에 쓰기 때문이다.

`PRAGMA user_version`을 이번에 넣는다 (3단계 Minor). 0(기존 `viewed`만 있음) → 1: `comments`, `settings` 생성 후 1로 올린다. 아는 것보다 큰 version이면 앱은 경고를 띄우고 메모리 DB로 대신한다 (3단계의 열기 실패와 같은 경로).

```sql
CREATE TABLE comments (
    id         INTEGER PRIMARY KEY,
    worktree   TEXT NOT NULL,    -- worktree id (= WatchLayout.root 문자열, viewed와 같은 key)
    path       TEXT NOT NULL,    -- worktree 상대 경로
    side       TEXT NOT NULL,    -- 'new' | 'old'
    line       INTEGER NOT NULL, -- 그 side의 줄 번호 (1부터)
    view       TEXT NOT NULL,    -- 'branch' | 'session' | 'turn': 단 화면
    snippet    TEXT NOT NULL,    -- 단 시점의 그 줄 내용 (앞의 +/-/공백 표시 없이)
    body       TEXT NOT NULL,
    created_at INTEGER NOT NULL, -- Unix ms
    updated_at INTEGER NOT NULL,
    sent_at    INTEGER,          -- NULL = 미전송
    sent_to    TEXT              -- session id 또는 'copy'
);
CREATE INDEX comments_unsent ON comments (worktree, sent_at);

CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
```

- 코멘트는 branch가 아니라 worktree에 붙는다 (봤음과 다름). 브랜치를 바꿔도 남는다 (내 판단).
- `body`는 1~4,000자, `snippet`은 500자에서 자른다. 그래서 코멘트 하나는 언제나 첨부 cap(9,000자) 안에 들어간다.
- 미전송 코멘트만 `body`를 고칠 수 있다. 전송된 코멘트는 지우기만 된다.
- 앱 시작 때 `sent_at`이 `[sessions] keep_days`보다 오래된 코멘트를 지운다 (세션 기록 정리와 같은 때, 내 판단).
- `settings`는 지금 `always_on_top`(`"1"`/`"0"`)만 쓴다.

### 동시성

- 앱의 `CommentStore`는 `ViewedStore`처럼 `Mutex<Connection>` 하나. 두 store가 연결을 따로 가져도 된다 (WAL).
- hook의 첨부는 transaction 하나로 한다 (3절). 두 Claude session이 같은 worktree에서 동시에 prompt를 보내도 한 코멘트는 한 쪽에만 붙는다.
- 앱이 DB를 열지 못해 메모리 DB를 쓰는 동안 단 코멘트는 hook이 보지 못한다. header 경고(기존)가 그 사실을 알린다.

## 3. hook: `prompt-submit` 첨부

`UserPromptSubmit`의 기존 `prompt-submit` subcommand에 붙인다. hooks.json은 바뀌지 않는다. 사용자는 `scripts/install-hook.sh`를 다시 실행해야 새 동작을 받는다.

순서:
1. `prompt`가 `/`로 시작하면(slash command) 첨부하지 않는다. `/compact` 같은 명령에 붙으면 반영되지 않은 채 전송됨이 된다. 코멘트는 다음 일반 prompt에 붙는다 (내 판단).
2. config(`WTBOARD_CONFIG` 또는 `~/.wtboard/config.toml`)의 `review.auto_attach_comments`를 읽는다. 기본 `true`. config를 못 읽어도 `true`로 본다.
3. `true`면 worktree id = `WatchLayout::detect(Git::new(cwd)).root` (앱과 같은 계산). `cwd`는 hook 입력의 값이다. `CLAUDE_PROJECT_DIR`이 아닌 `cwd`를 쓰는 것은 Claude Code가 세션 도중 다른 worktree로 들어간 경우를 따라가기 위해서다 (내 판단). git 레포가 아니면 첨부하지 않는다.
4. `<data dir>/state.sqlite`가 없으면 첨부하지 않는다 (hook은 DB를 만들지 않는다. `SQLITE_OPEN_READ_WRITE`만). `comments` table이 없어도(`user_version` < 1) 첨부하지 않는다.
5. `BEGIN IMMEDIATE` → 그 worktree의 미전송 코멘트를 `path, line, id` 순으로 읽는다 → cap 안에 들어가는 만큼 텍스트를 만든다 → **stdout에 JSON을 쓰고 flush** → 들어간 코멘트만 `sent_at = now, sent_to = session_id` → `COMMIT`.
   - 출력 뒤에 commit하므로 commit이 실패하면 다음 prompt에 다시 붙는다 (중복). 출력 전에 죽으면 미전송으로 남는다. 잃지 않는 쪽을 골랐다.
6. 기존 turn 기록(turn 닫기·열기)을 한다. 첨부와 turn 기록은 서로의 오류에 막히지 않는다. 오류는 둘 다 stderr에 한 줄씩.

stdout (첨부할 코멘트가 있을 때만. 없으면 지금처럼 아무것도 쓰지 않는다):

```json
{
  "systemMessage": "wtboard: 리뷰 코멘트 3개를 첨부했습니다",
  "hookSpecificOutput": { "hookEventName": "UserPromptSubmit", "additionalContext": "<아래 텍스트>" }
}
```

- `additionalContext`는 transcript에 보이지 않는다. 사용자에게 첨부 사실을 알리려고 `systemMessage`를 쓴다. UserPromptSubmit에서 `systemMessage`가 화면에 보이는지는 문서로 확인하지 못했다. 실제 확인 단계에서 본다. 안 보여도 기능에는 지장이 없다.
- `main.rs`의 "stdout에는 아무것도 쓰지 않는다" 규칙은 "prompt-submit의 첨부 JSON만 쓴다"로 바꾼다. 다른 subcommand는 그대로다.
- UserPromptSubmit은 사용자가 친 prompt 말고도 scheduled task, background subagent 보고, 다른 session의 메시지에서도 불린다 (문서). 이때도 첨부한다. 구분할 입력 필드가 문서에 없다 (알려진 한계).
- hook 속도 목표: 첨부 경로 포함 50ms 이하 (`rev-parse` 1회 + SQLite).

### 첨부 텍스트 (`core::review::render`)

````
<wtboard-review-comments worktree="/Users/jo/projects/my-api">
사용자가 wtboard에서 diff를 보며 남긴 리뷰 코멘트입니다. 각 코멘트가 가리키는 코드를 확인하고 반영해 주세요.

1. app/api/users.py:42
```
    return db.query(User).all()
```
페이지네이션이 필요합니다.

2. app/api/deps.py (삭제된 줄, 원래 17번째 줄)
```
from x import y
```
이 import는 다른 곳에서 쓰고 있어요.
</wtboard-review-comments>
````

- 경로는 worktree 상대 경로. 줄 번호는 단 시점의 번호라 그 뒤 수정으로 어긋날 수 있어 코드 줄을 같이 준다.
- `side = old`이면 "(삭제된 줄, 원래 N번째 줄)". 단 view가 branch면 base 기준, session·turn이면 그 view의 비교 전(before) 기준이다. 텍스트에는 view를 쓰지 않는다 (내 판단).
- code fence는 snippet에 ``` 이 들어 있으면 더 긴 fence를 쓴다.
- cap은 byte가 아니라 문자(char) 수다. JSON escape 뒤 길이도 10,000자를 넘지 않는지 한 번 더 본다.
- cap(9,000자)을 넘기 직전에 멈추고 끝에 "(코멘트 n개가 더 있습니다. 다음 메시지에 붙습니다)"를 넣는다. `systemMessage`도 "3개를 첨부했습니다 (2개 남음)".
- "마크다운으로 복사"는 같은 텍스트를 cap 없이 만든다 (붙여 넣는 사람이 판단한다).

## 4. 앱

### command

| command | 동작 |
|---|---|
| `list_comments(worktree_id)` | 그 worktree의 코멘트 전부. 미전송 먼저, 그 안에서 `path, line, id` 순 |
| `add_comment(worktree_id, path, side, line, view, snippet, body)` | 미전송으로 추가. 새 `Comment`를 돌려준다 |
| `update_comment(id, body)` | 미전송일 때만 (`WHERE sent_at IS NULL`). 전송된 코멘트면 오류 "이미 전송된 코멘트는 고칠 수 없습니다" |
| `delete_comment(id)` | 상태와 무관하게 지운다 |
| `clear_sent_comments(worktree_id)` | 전송된 코멘트를 모두 지운다 |
| `copy_comments(worktree_id)` | 미전송 코멘트의 markdown과 id 목록을 돌려준다. 상태는 바꾸지 않는다 |
| `mark_comments_sent(ids)` | `sent_to = 'copy'`. `WHERE sent_at IS NULL`이라 그사이 hook이 보낸 코멘트의 `sent_to`를 덮지 않는다. UI가 clipboard에 쓰기를 성공한 뒤 부른다 |

- `WorktreeSummary.unsent_comments: usize` 추가. `list_repos`가 worktree별 미전송 수를 query 한 번(`GROUP BY worktree`)으로 채운다 (`active_session`과 같은 방식).
- 코멘트를 쓰는 command가 끝나면 tray를 갱신하지 않는다 (tray는 미확인 수만 보인다).

### hook이 전송됨으로 바꾼 것을 화면이 알기

- prompt-submit은 세션 폴더에도 쓰므로 `session-updated`가 온다. UI는 그 worktree들의 `['comments', id]`도 invalidate한다.
- comments query 결과가 바뀌면 `['repos']`도 invalidate한다 (sidebar 💬n).
- 세션이 그 worktree에 귀속되지 않는 경우(4단계 귀속 규칙 밖)를 위해, 미전송 코멘트가 있는 worktree의 `['comments', id]` query는 5초마다 다시 읽는다 (내 판단. SQLite query 하나라 싸다).

### 메뉴바 (tray)

- Tauri 2 `TrayIconBuilder` (`tray-icon` feature). 단색 template icon (`icons/tray.png`, 메뉴바 높이에 맞춘 크기, `icon_as_template(true)`).
- title(아이콘 옆 글자) = 열린 모든 worktree의 `unviewed_files` 합. 0이면 title 없음.
- 메뉴:

```
미확인 12개                           (비활성 항목)
──────────
● my-web · ui-login            5
  my-api · feat-auth           7
──────────
wtboard 열기
✓ 항상 위에 띄우기
──────────
종료
```

  - worktree 줄은 미확인이 있거나 활성 세션이 있는 것만. sidebar 순서. 고르면 창을 보이고 앞으로 가져온 뒤 `select-worktree` event `{worktreeId}`를 보낸다. UI는 그 worktree를 선택한다.
  - "항상 위에 띄우기"는 check item. 바꾸면 `set_always_on_top` + `settings.always_on_top` 저장. 앱 시작 때 읽어 적용한다.
- 갱신 시점: `worktree-changed`, `worktrees-updated`, `set_viewed` 성공, `session-updated`, 그리고 60초 timer (활성 세션 만료). 여러 번 와도 1초에 한 번만 계산한다.
- **갱신 비용**: tray 갱신은 git을 부르지 않는다. 미확인 수는 `Worktree`의 메모리 상태로 센다 (`Worktree::unviewed_count()` 같은 가벼운 함수, `summary()`를 부르지 않는다). worktree label(branch)은 worktree를 열 때와 HEAD가 바뀔 때 갱신되는 값을 쓴다. 어떻게 cache할지는 builder가 정한다. 활성 세션 판정은 기존 `active_roots`(파일 stat)를 쓴다.

### 창

- 창 닫기(빨간 버튼, Cmd+W) = 숨기기. 앱은 메뉴바에 남는다. Dock 아이콘 클릭(`RunEvent::Reopen`)과 tray "wtboard 열기"가 다시 보인다. Cmd+Q와 tray "종료"가 끝낸다 (내 판단).
- Dock 아이콘은 그대로 둔다 (일반 앱).

## 5. UI

### 줄 코멘트

```
│ 41    40 │   def list_users():                                    │
│ 42  + 41 │ +     return db.query(User).all()                      │
│          │ ┌─ 미전송 ─────────────────────────────── 수정  삭제 ┐ │
│          │ │ 페이지네이션이 필요합니다.                          │ │
│          │ └────────────────────────────────────────────────────┘ │
│ 43    42 │                                                        │
```

- 줄 번호 gutter에 hover하면 `+` 버튼 (react-diff-view `renderGutter`). 누르면 그 줄 아래에 입력 widget (`widgets`, key = change key). textarea, `저장`·`취소`. Cmd+Enter 저장, Esc 취소.
- side: 삭제 줄은 `old` + 옛 줄 번호, 추가·문맥 줄은 `new` + 새 줄 번호. split 화면에서 문맥 줄의 왼쪽을 눌러도 `new`로 단다 (내 판단).
- 세 view(브랜치 전체·이번 세션·마지막 응답) 모두에서 달 수 있다. 텍스트 diff에만 (binary·1MB 초과는 없음).
- 저장된 코멘트는 그 줄 아래 widget으로 보인다. 미전송은 파란 테두리 + "미전송", 전송됨은 회색 + "전송됨 · 14:05" (복사면 "복사됨 · 14:05"). 미전송만 `수정`.
- **위치 맞추기**: 지금 diff에 (side, line)인 줄이 있고 그 내용이 `snippet`과 같으면 그 줄 아래에 보인다. `side = old`는 단 view와 지금 view가 같을 때만 맞춘다 (view마다 before가 다르다). 맞는 줄이 없으면 그 파일 diff 맨 위의 "위치가 바뀐 코멘트 n개"(접힘 가능)에 snippet과 함께 보인다.
- 파일이 접혀 있어도 header에 💬 n (그 파일의 미전송 수).

### 코멘트 목록

- worktree header 오른쪽에 `💬 미전송 n` 버튼 (0이면 `💬` 회색). 누르면 popover:
  - 미전송 → 전송됨 순 목록. 줄 하나 = `경로:줄`, body 첫 줄. 누르면 그 파일 diff로 스크롤한다 (기존 `fileAnchorId`).
  - `마크다운으로 복사` (미전송이 있을 때): `copy_comments` → `navigator.clipboard.writeText` → 성공하면 `mark_comments_sent`. clipboard 쓰기가 막히면 Tauri clipboard plugin으로 바꾼다 (builder 판단).
  - `전송된 코멘트 지우기`.
  - 자동 첨부가 꺼져 있으면 맨 위에 "자동 첨부 꺼짐: 복사해서 Claude Code에 붙여 넣으세요". 그러려면 `list_repos` 응답에 `autoAttachComments: bool`을 넣는다. hook이 매번 config를 읽으므로 앱도 `list_repos`마다 config에서 다시 읽는다.
- 파일 목록(FileTree)의 파일 이름 옆에도 미전송이 있으면 💬.

### sidebar, event

- sidebar worktree 줄에 미전송이 있으면 `💬n` (미확인 수 앞).
- `select-worktree` event → `select({repoId, worktreeId})`. repo id는 `['repos']` data에서 찾는다.
- query key: `['comments', worktreeId]`. 코멘트 command 성공 후 그것과 `['repos']`를 invalidate.

## 6. 오류 처리와 경계 사례

- 코멘트 저장 실패: 입력 widget을 닫지 않고 그 아래에 오류를 보인다 (입력한 글을 잃지 않게).
- 코멘트를 단 파일이 목록에서 사라짐 (되돌림, commit 후 merge-base 이동): 코멘트는 남는다. popover 목록에는 보이고, 누르면 아무 데도 스크롤하지 않는다 (경로 옆에 "목록에 없음").
- worktree가 지워짐: 코멘트 row는 남는다 (3단계 viewed와 같은 한계). 전송된 것은 keep_days 뒤 지워진다.
- 같은 worktree에 Claude session이 둘: 먼저 prompt를 보낸 session에 붙는다. 전송됨 표시에 session을 구분하는 정보는 보이지 않는다 (`sent_to`에는 남는다).
- 사용자가 `scripts/install-hook.sh`를 다시 실행하지 않으면 옛 hook이 돌아 자동 첨부가 아무 표시 없이 일어나지 않는다. 앱이 옛 binary를 감지하지는 않는다 (알려진 한계. hook 번들 작업과 같이 6단계 후보).
- hook timeout(UserPromptSubmit 기본 30초)에 걸리면 Claude Code는 출력을 버린다. 그때 commit 전이면 미전송으로 남고, commit 뒤면 잃는다 (출력과 commit 사이는 ms 단위라 받아들인다).

## 7. test

test를 먼저 쓴다. 실제 git, 실제 SQLite 파일, 실제 binary.

**core**
1. schema: 빈 파일 → version 1. `viewed`만 있는 version 0 DB → row 유지하고 1로 올라감. 더 큰 version → 오류.
2. `CommentStore`: 추가·목록 순서·수정(전송됨이면 거부)·삭제·`clear_sent`·`mark_sent`·worktree별 미전송 수·keep_days 정리.
3. `review::render`: 번호·경로·줄·old side 문구·fence 이스케이프·cap에서 멈추고 남은 수 표시·cap 없는 모드.
4. 두 연결(앱 store, hook과 같은 방식의 연결)에서 동시 claim → 각 코멘트는 한 번만 claim.

**hook (`cargo test -p wtboard-hook`)**
1. 미전송 코멘트가 있는 worktree에서 `prompt-submit` → stdout JSON의 `additionalContext`에 코멘트, DB에서 전송됨(`sent_to` = session id). turn 기록도 그대로 된다.
2. 다른 worktree의 코멘트는 붙지 않는다. 하위 폴더 `cwd`도 그 worktree로 본다. worktree id가 앱 계산과 같다 (`/var` → `/private/var` tempdir에서).
3. `auto_attach_comments = false` → stdout 비어 있음, 미전송 유지.
4. DB 없음 / `comments` table 없음 / 깨진 DB → stdout 비어 있음, exit 0, DB 파일을 만들지 않음.
5. cap 초과 → 일부만 전송됨, 나머지 미전송, 다음 호출에서 붙음.
6. 첨부 후에도 레포 `.git/index` 불변 (`read_only` 규칙).

**app**: tray 미확인 합계·메뉴 항목 계산을 순수 함수로 떼어 test (Tauri 없이).

**UI (`pnpm --dir ui test`)**: 코멘트 위치 맞추기(같은 줄·내용 다름·old side view 다름), side/line 결정, popover 정렬, `select-worktree` → selection, `session-updated` → comments invalidate key.

**실제 확인** (fixture `WTBOARD_CONFIG`)
1. 앱에서 줄 2개에 코멘트 (하나는 삭제 줄) → 미전송 widget과 header 💬 2 screenshot.
2. `WTBOARD_CONFIG=… WTBOARD_HOOK_BIN=<target>/debug/wtboard-hook claude --plugin-dir plugin/wtboard -p "리뷰 코멘트 반영해 줘"` → Claude가 그 줄을 고친다. 앱에서 코멘트가 "전송됨"으로 바뀐다. `systemMessage`가 보이는지 기록한다 (`-p`가 아닌 대화형에서도 한 번).
3. `auto_attach_comments = false`로 같은 순서 → 첨부 없음. 복사 → 전송됨(복사됨).
4. 메뉴바 숫자, 메뉴에서 worktree 선택, 창 닫고 Dock 클릭으로 다시 열기, 항상 위에 토글 후 재시작 유지 screenshot.
5. fixture `.git/index` mtime이 전후로 같다.
