# 4단계 설계: Claude Code plugin(hooks) + 이번 세션 / 마지막 응답 탭

상위 설계: `docs/draft.md` (4절, 5절, 9절, 10절, 12절, 13절). 이 문서는 4단계에서 draft와 달라진 점과 구체 설계를 적는다.

사용자는 질문 없이 바로 진행하기로 했다. "(내 판단)"으로 표시한 것은 설계자가 정한 것이다. hook 규격은 2026-10-19 Claude Code 공식 문서(hooks, plugins-reference, plugin-marketplaces)로 확인했다.

## 목표와 완료 기준

- Claude Code 세션이 고친 파일만 모은 "이번 세션" 탭과, 마지막 응답 하나가 고친 것만 보이는 "마지막 응답" 탭이 있다.
- Edit/Write뿐 아니라 Bash로 바뀐 파일(`dart run build_runner`, `sed -i`, `pnpm add` 등)도 세션 변경에 들어간다.
- 세션 중에 커밋해도 "이번 세션" diff가 깨지지 않는다 (git이 아니라 세션 원본과 비교).
- sidebar에 활성 세션이 있는 worktree는 ●로 보인다.
- hook은 어떤 경우에도 exit 0이고 Claude 작업을 막지 않는다.

범위 밖:
- UserPromptSubmit으로 리뷰 코멘트 첨부 (5단계). 4단계의 UserPromptSubmit hook은 turn 경계 기록만 한다.
- 앱 번들에 hook binary를 넣어 첫 실행 때 복사하기. 4단계는 개발용 설치 script로 대신한다 (내 판단).
- 세션·응답 탭의 봤음 체크. checkbox는 브랜치 전체 탭에만 있다 (내 판단).
- 3단계에서 넘어온 `upstream` PR base, summary cache.

## draft와 달라진 점

| draft | 4단계 | 이유 |
|---|---|---|
| matcher `Edit\|MultiEdit\|Write\|NotebookEdit` | `Edit\|Write\|NotebookEdit` | `MultiEdit` tool이 없어졌다 |
| turn 끝 = Stop | turn 시작 = UserPromptSubmit, 끝 = Stop 또는 StopFailure. 끝 없이 다음 prompt가 오면 그때 이전 turn을 닫는다 | 사용자가 Esc로 중단하면 Stop이 오지 않는다 |
| post-edit/post-bash = PostToolUse | PostToolUse와 PostToolUseFailure 둘 다 | 실패한 Bash(빌드 실패 등)도 파일을 바꿨을 수 있다 |
| 세션 → worktree는 cwd로 | 경로마다 worktree를 정한다. 세션은 그 세션이 건드린 파일이 있는 worktree, 또는 `CLAUDE_PROJECT_DIR`이 속한 worktree에 보인다 | hook의 `cwd`는 `cd`를 따라 바뀐다. 한 세션이 여러 worktree를 고칠 수도 있다 |
| `~/.wtboard/sessions/` | config 파일과 같은 폴더의 `sessions/` (기본 `~/.wtboard/sessions/`) | 3단계 `state.sqlite`와 같은 규칙. fixture 실행이 실제 기록을 건드리지 않는다 |
| `bin/wtboard-hook`은 앱 첫 실행 시 복사 | `scripts/install-hook.sh`가 release build 후 복사 | 번들 작업은 범위 밖 |

## 1. 구성

| 단위 | 하는 일 |
|---|---|
| `crates/hook` (신규, binary `wtboard-hook`) | stdin JSON을 읽어 세션 폴더에 기록만 한다. 계산은 하지 않는다. `core`의 `Git` wrapper와 경로 규칙을 쓴다 |
| `core::session` (신규) | 세션 폴더 형식(쓰기·읽기 공용), 세션 목록, 세션·응답 변경 목록과 patch 계산 |
| `core::store` (수정) | data 폴더 결정 `data_dir()`: `WTBOARD_CONFIG`가 있으면 그 파일의 폴더, 없으면 `~/.wtboard`. 앱과 hook이 같이 쓴다 |
| `core::watch` (수정) | `sessions/` 감시 → 바뀐 session id 알림 |
| `crates/app` (수정) | `list_sessions`, `list_files`/`get_patch`의 `view = session \| turn`, `session-updated` event, 시작 시 오래된 세션 정리 |
| `plugin/` (신규) | local marketplace + plugin (`hooks.json`, `run.sh`) |
| `ui/` (수정) | 탭 3개, 세션 선택, sidebar ● |

## 2. 세션 폴더

```
<data dir>/sessions/<session_id>/
├─ meta.json            # {started_at, project_dir, start_head?, turn, turn_open, ended_at?}
├─ events.jsonl         # 한 줄에 event 하나
├─ baseline/<abs path>  # 세션에서 처음 건드리기 직전의 원본. 경로는 절대 경로에서 앞의 / 를 뗀 것
├─ baseline.jsonl       # 원본 기록: {path, kind: "file"|"new"|"too_large"|"uncertain", head?}
├─ bash/<tool_use_id>.json  # pre-bash 스냅샷 (post-bash가 읽고 지운다)
└─ turns/<n>/<abs path> # turn n이 끝난 시점의 파일 내용 (그 turn에 건드린 파일만)
    └─ (turns/<n>/index.jsonl: 그 시점에 없던 파일(deleted)과 1MB 초과 파일(too_large, size·hash) 목록)
```

- 경로 key는 절대 경로다 (NFC). 한 세션이 여러 worktree를 고쳐도 섞이지 않는다.
- `events.jsonl` 줄: `{"t":..., "turn":n, "kind":"edit"|"bash"|"turn_start"|"turn_end", "tool"?, "paths":[abs...], "agent_id"?}`.
- 쓰기는 모두 `<data dir>/sessions/` 안에서만 한다. 레포에는 쓰지 않는다.
- 동시성: hook은 병렬로 돈다 (같은 event의 여러 hook, 병렬 tool 호출). baseline은 `create_new`(O_EXCL)로 만들어 먼저 만든 쪽이 이긴다. `events.jsonl`은 줄 하나를 `O_APPEND` write 한 번으로 쓴다. `meta.json`은 임시 파일에 쓰고 rename한다. turn 경계 hook(session-start, prompt-submit, turn-end, session-end)만 쓰고, 이 hook들은 한 세션 안에서 겹치지 않는다. 마지막 활동 시각은 meta에 두지 않고 `events.jsonl`의 mtime으로 본다 (모든 hook이 meta를 쓰면 turn 증가가 덮일 수 있다).
- `session_id`는 `[A-Za-z0-9_-]`만 허용한다. 경로는 절대 경로이고 `..` component가 없어야 한다. 아니면 기록하지 않는다 (세션 폴더 밖에 쓰지 않게).
- 1MB(`MAX_DIFF_BYTES`)를 넘는 파일은 내용을 복사하지 않고 `too_large`로 기록한다. 크기와 blake3 hash를 남겨 화면이 크기 변화를 보이고 같은지 판단할 수 있게 한다.
- 스냅샷은 경로마다 따로 처리한다. 한 경로의 오류(디렉터리, 읽기 실패, 그사이 사라짐)가 그 turn의 다른 경로를 막지 않는다.

## 3. hook 동작

모든 subcommand 공통: stdin JSON 파싱 실패, 쓰기 실패 등 어떤 오류도 stderr에 한 줄 남기고 exit 0. git 호출은 `core::git::Git`(`GIT_OPTIONAL_LOCKS=0`)을 거친다. 목표 50ms (pre/post-bash 제외).

| hook event | subcommand | 동작 |
|---|---|---|
| SessionStart | `session-start` | `meta.json`이 없으면 만든다 (`project_dir` = env `CLAUDE_PROJECT_DIR`, 없으면 `cwd`. `start_head` = 그 폴더의 HEAD, git 레포가 아니면 없음). `source`가 resume 등이라 이미 있으면 `ended_at`만 지운다 |
| UserPromptSubmit | `prompt-submit` | 열린 turn이 있으면 먼저 닫는다 (아래 turn-end와 같음). 그리고 `turn += 1`, `turn_open = true`, `turn_start` event |
| PreToolUse `Edit\|Write\|NotebookEdit` | `pre-edit` | 경로(`tool_input.file_path`, 없으면 `notebook_path`)의 baseline이 없으면 만든다: 파일이 있으면 복사(`file`), 없으면 `new` |
| PostToolUse, PostToolUseFailure `Edit\|Write\|NotebookEdit` | `post-edit` | `edit` event (turn = meta의 현재 turn) |
| PreToolUse `Bash` | `pre-bash` | `cwd`의 레포에서 HEAD와 `git status --porcelain=v2 -z --untracked-files=all` 결과, 변경 파일들의 (mtime, size)를 `bash/<tool_use_id>.json`에 저장. git 레포가 아니면 아무것도 안 한다 |
| PostToolUse, PostToolUseFailure `Bash` | `post-bash` | 스냅샷과 지금 status를 비교해 바뀐 파일을 찾는다 (아래). 각 파일 baseline을 정하고 `bash` event. 스냅샷 파일을 지운다 |
| Stop, StopFailure | `turn-end` | 열린 turn이 있으면 닫는다: 이 turn의 event에 나온 경로를 `turns/<n>/`에 복사(없으면 `index.jsonl`에 deleted), `turn_end` event, `turn_open = false` |
| SessionEnd | `session-end` | 열린 turn을 닫고 `ended_at` 기록. SessionEnd timeout이 1.5초라 복사가 오래 걸리면 중간에 끊길 수 있다 (알려진 한계) |

turn이 열리기 전(첫 prompt 전)의 edit/bash event는 turn 0으로 기록한다. 앱은 turn 0을 turn 1에 합친다 (내 판단).

### post-bash: 바뀐 파일 찾기

pre 스냅샷 S(경로 → status, mtime, size), 지금 status T.

| 경우 | 바뀐 파일로 보나 | baseline (아직 없을 때만) |
|---|---|---|
| S에 없고 T에 있음 (깨끗했다가 바뀜, 또는 새 untracked) | 예 | tracked면 pre HEAD의 내용(`git show <pre HEAD>:<path>`, `file`). untracked면 `new` |
| S·T 둘 다 있고 mtime 또는 size가 다름 | 예 | `uncertain` (Bash 전 내용을 모른다). hook이 세션 시작 HEAD의 내용을 원본 파일로 저장해 비교 기준으로 쓴다 |
| S에 있고 T에 없음 (Bash가 되돌림, 커밋, 삭제) | 예 | `uncertain` |
| 그 외 | 아니오 | — |

- ignored 파일, 제외 폴더(`node_modules` 등)는 git status에 안 나오므로 잡지 않는다.
- 큰 레포에서 `--untracked-files=all` status는 수백 ms 걸릴 수 있다. Bash 명령 자체보다 짧으므로 받아들인다 (알려진 한계).

## 4. 앱: 세션 변경 계산

### 세션 목록

- `list_sessions(worktree_id)` → `SessionSummary[]` (`id`, `started_at`, `last_event_at`(= `events.jsonl` mtime), `active`, `turn`, `turn_open`, `touched_files`(이 worktree 아래 event 경로 수)). 최근 활동 순.
- 세션이 worktree에 속하는 조건: 그 세션 event의 경로 중 하나가 worktree root 아래, 또는 `project_dir`가 worktree root 아래.
- `active` = `ended_at` 없음 && `last_event_at`이 30분 안 (내 판단. 앱이 죽어 SessionEnd가 안 온 세션을 끝난 것으로 본다).
- `WorktreeSummary.activeSession: bool` 추가 → sidebar ●.

### 이번 세션 (`view = session`)

- 파일 = 그 세션 event의 경로 중 이 worktree 아래인 것.
- 비교: 원본(baseline) ↔ 현재 파일. 원본이 `new`면 빈 파일, `uncertain`이면 세션 시작 HEAD의 내용(없으면 빈 파일), `too_large`면 크기만.
- 원본과 현재가 바이트 단위로 같으면 목록에서 뺀다 (되돌린 파일).
- status: 원본 `new` + 현재 있음 → added, 현재 없음 → deleted, 그 외 modified. `uncertain`은 `ChangedFile`에 `uncertain: true`로 표시하고 UI가 header에 "원본 불확실".
- 접기(profile pattern, `linguist-generated`)는 브랜치 뷰와 같은 규칙.

### 마지막 응답 (`view = turn`)

- 대상 turn: 열린 turn이 있고 그 turn에 event가 있으면 그 turn (진행 중), 아니면 마지막으로 닫힌 turn. event가 하나도 없으면 빈 목록.
- 파일 = 대상 turn의 event 경로 중 이 worktree 아래인 것.
- 비교 전(before): 그 파일의 가장 최근 `turns/<k>/` 스냅샷(k < 대상 turn), 없으면 원본(위 규칙).
- 비교 후(after): 닫힌 turn이면 `turns/<n>/` 스냅샷, 진행 중이면 현재 파일.
- before와 after가 같으면 뺀다.

### diff 계산

- `git diff --no-index`를 `Git` wrapper로 부른다 (레포에 쓰지 않는다. exit 1 = 차이 있음). 1단계 `file_patch`와 같은 flag, 같은 `PatchBody`(text/binary/too_large), 같은 hash.
- before가 세션 폴더 안의 파일이라도 diff header의 경로는 worktree 상대 경로로 보인다 (UI는 hunk만 쓰지만 header도 맞춘다).
- 화면에 보일 때만 계산한다 (기존 규칙).

### 갱신

- `sessions/` 폴더를 FSEvents로 감시한다 (300ms debounce). 바뀐 session id를 모아 `session-updated` event `{session_ids, worktree_ids}`를 보낸다. worktree_ids는 그 세션이 속한 worktree.
- UI는 `session-updated`로 `['sessions', worktreeId]`, `['files', worktreeId, 'session' | 'turn']`, `['repos']`를 invalidate한다. 파일 내용이 바뀌는 것은 기존 `worktree-changed`가 `['files', id]`·`['patch', id, path]`를 invalidate하므로 세션 탭에도 걸린다.
- 앱 시작 시 `keep_days`(config `[sessions] keep_days`, 기본 7)보다 오래된(`last_event_at` 기준) 세션 폴더를 지운다.

## 5. plugin

```
plugin/                              # local marketplace root
├─ .claude-plugin/marketplace.json   # {"name":"wtboard-local","owner":{...},"plugins":[{"name":"wtboard","source":"./wtboard"}]}
└─ wtboard/
   ├─ .claude-plugin/plugin.json     # {"name":"wtboard", ...}
   └─ hooks/
      ├─ hooks.json                  # 3절 표의 event → run.sh <subcommand>
      └─ run.sh                      # ~/.wtboard/bin/wtboard-hook 없으면 exit 0
```

- `run.sh`는 draft 그대로. `WTBOARD_HOOK_BIN` env가 있으면 그 경로를 쓴다 (test·fixture용, 내 판단).
- 설치 (사용자가 직접): `scripts/install-hook.sh` → `claude plugin marketplace add <repo>/plugin` → `claude plugin install wtboard@wtboard-local`. 앱은 사용자의 Claude 설정을 건드리지 않는다.
- 개발 중 확인: `claude --plugin-dir plugin/wtboard`로 plugin을 설치 없이 띄운다.
- `claude plugin validate plugin`이 통과해야 한다.

## 6. UI

```
┌──────────────────┬──────────────────────────────────────────────────────────────┐
│ ● front/feat 5/14│ omofictions-front · front/feat ← develop (PR)                │
│        3분 전    │ [브랜치 전체] [이번 세션 ▾ 14:02 시작] [마지막 응답 #3]   [split|unified] │
│                  ├──────────────────┬───────────────────────────────────────────┤
│                  │ 변경 파일 4개     │ lib/user.g.dart   원본 불확실   +120 -4   │
```

- header 아래 줄에 탭 3개. 세션이 하나도 없으면 세션 탭 두 개는 비활성(회색, hover에 "이 worktree의 Claude Code 세션 기록이 없습니다").
- 이번 세션 탭 옆 ▾: 세션이 여러 개면 고른다 (시작 시각, 활성이면 ●). 기본은 가장 최근 활동 세션. 고른 세션은 worktree별로 기억한다 (localStorage, 기존 store).
- 마지막 응답 탭에는 대상 turn 번호와, 진행 중이면 "진행 중" 표시.
- 세션·응답 탭에서는 `확인 n/m`과 봤음 checkbox를 숨긴다.
- `uncertain` 파일은 diff header에 회색 "원본 불확실" (hover: "Bash가 바꾼 파일이라 세션 전 내용을 모릅니다. 세션 시작 commit과 비교합니다").
- sidebar: 활성 세션이 있는 worktree 이름 앞에 ● (초록).

## 7. 오류 처리와 경계 사례

- 세션 폴더가 깨졌거나(JSON 오류) 일부만 있으면 그 세션은 읽을 수 있는 만큼 보이고, 못 읽은 줄은 건너뛴다.
- baseline이 없는 경로(hook이 늦게 설치됐거나 실패)는 `uncertain`으로 본다.
- 세션 중 브랜치 전환: 세션 diff는 git과 무관하므로 그대로다.
- worktree 밖 파일(예: `~/.zshrc`)은 어느 worktree에도 안 보인다.
- 같은 파일을 두 세션이 고치면 각 세션이 자기 원본 기준으로 보인다.
- subagent의 edit은 같은 `session_id`로 오는 것으로 보고(공식 문서 예시 기준) 부모 세션에 넣는다. 실제 확인은 실제 확인 단계에서 한다.

## 8. test

test를 먼저 쓴다. 실제 git, 실제 파일, 실제 FSEvents.

**hook (`cargo test -p wtboard-hook`)**: binary를 `WTBOARD_CONFIG=<tmp>/config.toml`로 실행하고 stdin에 hook JSON을 넣는다.
1. 어떤 입력에도 exit 0 (빈 stdin, 깨진 JSON, 없는 경로, 쓰기 불가 data dir).
2. pre-edit: 기존 파일 → baseline 복사, 없는 파일 → `new`, 두 번째 touch는 baseline을 바꾸지 않음, 병렬 10개 → baseline 하나.
3. post-bash: 깨끗한 tracked 파일 수정 → pre HEAD 내용이 baseline, 새 untracked → `new`, 이미 수정된 파일 재수정 → `uncertain`, Bash가 `git checkout -- f`로 되돌림 → `uncertain`, Bash 중 커밋(HEAD 바뀜) → pre HEAD 기준.
4. turn: prompt-submit → edit → turn-end → `turns/1/`에 내용. turn-end 없이 prompt-submit 두 번 → 이전 turn이 닫힘. turn 0 event.
5. 1MB 넘는 파일 → `too_large`.
6. `read_only.rs`처럼 hook 실행 전후 `.git/index` 불변.

**core (`cargo test -p wtboard-core`)**: 세션 폴더를 hook binary가 아닌 `core::session` 쓰기 API로 만들고 읽기를 검증.
1. session view: 수정·추가·삭제·되돌림(목록에서 빠짐)·`uncertain`·세션 중 커밋해도 diff 그대로.
2. turn view: 닫힌 turn, 진행 중 turn, 이전 turn에 안 건드린 파일의 before = baseline, 같은 파일을 turn 2·3에서 고침.
3. 세션 → worktree 귀속, `active` 판정, keep_days 정리.
4. watcher: 세션 폴더에 쓰면 1초 안에 session id 알림.

**UI (`pnpm --dir ui test`)**: 탭 비활성 조건, 세션 기본 선택과 기억, `session-updated` → invalidate key.

**실제 확인**: fixture 레포에서 `WTBOARD_CONFIG=<fixture>/config.toml WTBOARD_HOOK_BIN=<target>/debug/wtboard-hook claude --plugin-dir plugin/wtboard -p "<파일 하나 고치고 build 명령 하나 실행>"`로 실제 세션을 만들고, 앱을 띄워 세션·응답 탭을 screenshot으로 확인한다. subagent edit의 `session_id`도 이때 확인한다. 클릭은 3단계 안전 규칙(frontmost·창 안 확인, 키 입력 없음)을 따른다.
