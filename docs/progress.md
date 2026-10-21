# 진행 상황

설계: `docs/draft.md`. 단계별 plan: `docs/superpowers/plans/`.

## 단계

| 단계 | 상태 | plan |
|---|---|---|
| 1. 레포 1개·worktree 1개 브랜치 전체 뷰 + 실시간 갱신 | 완료 (2026-10-15) | `plans/2026-10-15-stage1-branch-view.md` |
| 2. 여러 레포·worktree 자동 발견 + 사이드바 + 프로필 3종 | 완료 (2026-10-15) | `plans/2026-10-15-stage2-multi-repo.md` (spec: `specs/2026-10-15-stage2-multi-repo-design.md`) |
| 3. 봤음 체크 + 다시 바뀜 표시 | 완료 (2026-10-19) | `plans/2026-10-19-stage3-viewed.md` (spec: `specs/2026-10-19-stage3-viewed-design.md`) |
| 4. plugin(hooks) + 이번 세션 / 마지막 응답 탭 | 완료 (2026-10-19) | `plans/2026-10-19-stage4-sessions.md` (spec: `specs/2026-10-19-stage4-sessions-design.md`) |
| 5. 리뷰 코멘트 자동 첨부 + 메뉴바 | 완료 (2026-10-19) | `plans/2026-10-19-stage5-review-comments.md` (spec: `specs/2026-10-19-stage5-review-comments-design.md`) |
| 6. 전송 뒤 코멘트의 생애: 다시 보내기·응답 연결·해결됨 + 옛 hook binary 경고 | 완료 (2026-10-21) | `plans/2026-10-21-stage6-comment-lifecycle.md` (spec: `specs/2026-10-21-stage6-comment-lifecycle-design.md`) |
| 7. 미정 | 아직 없음 | "7단계 후보" 참고 |

## 1단계 결과

- `omofictions-back-1371`(`back/feat/1371`, base `origin/develop`, 변경 파일 617개)으로 화면 확인.
- plan 이후 추가: sticky 파일 헤더를 스크롤 영역 맨 위에 붙임, 왼쪽 목록을 디렉터리 트리로 변경 (단일 자식 폴더는 한 줄로 합침, diff 순서도 트리 순서).
- 최종 리뷰에서 고친 것: `git diff`의 index auto-refresh(`diff.autoRefreshIndex=false`), HEAD 변경 → merge-base 재계산, 큰 event batch(>200 경로) → 목록 재계산, untracked symlink/FIFO를 따라가 읽지 않음.

## 1단계에서 정한 것 (plan과 다른 점)

- watcher는 spec 4절과 달리 `app`이 아니라 `core::watch`에 있다 (Tauri 없이 테스트하려고).
- hash는 화면이 patch를 가져간 파일만 저장한다 (3단계에서 봤음 체크한 파일까지 넓혔다).
- 커밋 감지는 HEAD 파일이 아니라 refs 감시가 담당한다 (spec 8절 표 보정).
- rename 감지 없음 (`--no-renames`, 삭제 + 추가로 보임).

## 2단계 결과

- `scripts/fixtures.sh`로 만든 Python / Node / Flutter fixture(`WTBOARD_CONFIG`)와 실제 `~/.wtboard/config.toml`(레포 5개)로 화면 확인. 실제 레포의 `git status`와 `.git/index`는 앱 실행 전후로 같았다.
- sidebar: 레포별 profile badge, worktree별 변경 수와 마지막 변경 시각, config 오류. `git worktree add/remove`와 `rm -rf`가 1초 안에 반영된다. 선택은 `localStorage`에 남는다.
- 접기: profile pattern + `linguist-generated`. file tree 맨 아래 "접힌 파일 (n)" 폴더(처음엔 닫힘), diff는 펼칠 때만 계산.
- base: config `base` → `gh pr view`(background) → `origin/HEAD`. PR base면 header에 `(PR)`. 실제 레포는 지금 PR이 있는 브랜치가 없어 화면에서는 `origin/develop`만 확인했고, PR 경로는 stub gh test로 검증했다.
- 목록 계산이 500ms를 넘고 fsmonitor가 꺼진 레포에 안내 banner (`WTBOARD_SLOW_LIST_MS`로 기준 변경).

## 2단계에서 정한 것 (spec과 다른 점)

- `list_repos`에 `ready`가 없다. command가 registry가 열릴 때까지 기다린다 (`OnceLock::wait`). UI poll도 없다.
- profile 고유 제외 목록은 공통 목록과 겹치지 않는 것만 적었다 (실제 제외 집합은 spec 표와 같다).
- `.gitignore` 수정도 `.gitattributes`처럼 목록 전체를 다시 계산한다 (새로 ignore된 파일이 바로 빠지게).
- 모든 파일 diff header를 접을 수 있다. 생성 파일은 접힌 채로 시작한다.
- 자동으로 worktree를 고를 때는 열기 오류가 없는 것을 먼저 고른다.
- 사용자가 할 일: `~/.wtboard/config.toml`의 `~/projects/omofictions-back-1371`(지워진 worktree)을 `~/projects/omofictions-back`으로 바꾼다.

## 3단계 결과

- fixture(`WTBOARD_CONFIG`)로 화면 확인: 체크 → 파일 접힘·☑·header 확인 n/m·sidebar 미확인/전체. 체크한 파일을 수정하면 1초 안에 "다시 바뀜"으로 돌아온다. 앱을 다시 띄워도 체크가 남는다. DB 쓰기가 실패하면 체크가 되돌아가고 header에 오류가 보인다.
- 실제 `~/.wtboard/config.toml`(레포 5개, `back/fix/2007` 변경 339개)로 체크·해제를 확인했다. 끝난 뒤 `state.sqlite` row는 0개. 실제 레포의 `.git/index`와 status는 4개 레포에서 App 실행 전후가 같았다. `omofictions-back-2007`은 확인 중 그 worktree에서 다른 작업(golden test 파일 생성)이 돌아 비교하지 못했다.
- 실제 레포 확인 중 발견: 긴 diff가 worktree 화면을 창보다 넓혀 header의 확인 수·split 버튼·봤음 checkbox가 창 밖으로 밀렸다. `WorktreeView` root에 `min-w-0`을 줘서 고쳤다 (1단계부터 있던 문제).

## 3단계에서 정한 것 (spec과 다른 점)

- `files()`는 그대로 두고 `entries()`(파일 + 봤음 상태)를 추가했다. spec은 `files()`가 바뀌는 것으로 적었다.
- `set_viewed`는 `hashes`에 값이 없을 때만 넣는다. 화면이 모르는 hash를 넣으면 아직 처리 전인 FS event를 삼킨다 (spec도 보정).
- `refresh_paths`는 checked 파일의 새 hash를 버리지 않고 유지한다 (spec도 보정).
- 해제(`viewed = false`)는 목록에 없는 경로여도 성공한다. 체크만 `UnknownFile`.
- 폴더 경로가 섞인 event 묶음도 화면이 아는 파일의 hash를 비교한다.
- checked 파일 하나의 hash 계산 실패는 건너뛰고 그 파일을 `Changed`로 보인다. worktree 열기·갱신 전체는 실패하지 않는다.
- `set_viewed` 실패는 체크를 되돌리고 header에 `봤음 저장 실패: …`로 보인다 (spec 3절).

## 3단계 알려진 한계

- poll/Rescan backup은 +/- 줄 수가 같은 checked 파일 변경을 못 잡는다 (FS event로만 잡힌다).
- 처리 전 event 창에서 `patch()`가 checked 파일의 hash를 덮으면 그 event가 사라질 수 있다.
- `set_viewed`가 state lock을 쥔 채 SQLite에 쓴다. DB lock이 걸리면 최대 5초 멈춘다.
- merge-base가 바뀌면 hunk 줄 번호 때문에 `changed`로 보일 수 있다.
- 생성 파일 체크가 실패해 되돌아가면 펼쳐진 채로 남는다.
- detached HEAD는 모두 branch `''` 하나를 공유한다.
- 사라진 worktree·파일의 row를 지우지 않는다.

## 4단계 결과

- `crates/hook`(`wtboard-hook`)과 `plugin/wtboard`(hook 설정)를 추가했다. 앱은 `core::session`(format/list/view)으로 session 기록을 읽어 `이번 세션`·`마지막 응답` 탭, sidebar ●, `원본 불확실` 표시를 만든다.
- fixture(`WTBOARD_CONFIG`)에서 실제 `claude -p` session(claude 2.1.303)으로 확인했다.
  - turn 1: Edit tool, Bash `sed -i`, subagent의 Write. Bash가 바꾼 `next-env.d.ts`는 `원본 불확실`로 접힌 파일에 보인다.
  - subagent의 Write hook은 부모와 같은 `session_id`에 `agent_id`가 붙어 도착했고, 부모 session의 turn 1에 기록됐다 (spec 7절의 가정이 맞았다).
  - `claude --resume`으로 이어 가면 같은 session의 turn 2가 된다. `마지막 응답 #2`는 그 응답이 바꾼 파일 1개만 보인다.
  - 탭·session select·sidebar ●(실제 `claude -p`는 session을 끝내므로 `session-start`만 직접 hook에 넣어 확인)를 screenshot으로 확인했다. session이 없는 worktree는 두 탭이 비활성이다.
  - fixture의 `.git/index` mtime은 앱 실행 전후로 같았다.
- hook 속도(release): edit 약 3ms, pre-bash 약 19ms, post-bash 약 20~28ms.
- 설치: `scripts/install-hook.sh`가 hook binary를 `~/.wtboard/bin/`에 설치하고, 이어서 사용자가 직접 `claude plugin marketplace add <repo>/plugin`, `claude plugin install wtboard@wtboard-local`을 실행한다 (스크립트는 Claude 설정을 건드리지 않는다).
- 최종 리뷰에서 고친 것: Bash가 HEAD를 옮기는 명령(pull/checkout/rebase/merge/reset)이 session을 도배하던 것, 변경 파일마다 numstat process를 띄우던 것(session view cache), nested repo(`? dir/`)가 빈 baseline file을 만들던 것, 상대 data dir, turn snapshot 실패가 turn 증가를 막던 것.

## 4단계에서 정한 것 (spec과 다른 점)

- turn별 목록은 `deleted.txt`/`too_large.txt`가 아니라 `turns/<n>/index.jsonl`(`{path, kind: deleted|too_large, size?, hash?}`)에 둔다. 개행이 든 경로가 항목을 흉내 낼 수 없다.
- 1MB를 넘는 파일은 내용 대신 size + hash를 baseline과 snapshot에 기록한다.
- session 폴더는 0700, 그 안의 파일은 0600이다 (사용자 파일의 사본이 들어가기 때문).
- Bash가 만든 변경은 pre-bash/post-bash `git status` 비교로 잡는다. commit은 그 Bash 호출이 만든 것만 (HEAD가 pre-bash HEAD의 descendant이고 사이의 모든 commit의 committer time이 pre-bash 시각 이후) 변경 파일로 넣는다.
- rename은 새 경로와 원래 경로를 둘 다 기록한다 (원래 경로의 baseline은 pre-bash HEAD 내용).
- nested worktree(`.claude/worktrees/x` 등)의 경로는 가장 깊은 root 하나가 소유한다. 바깥 worktree의 session view와 branch view에서는 빠진다.
- `SessionSummary.turn`은 raw meta가 아니라 `마지막 응답` 탭이 보여 주는 turn이다.
- baseline 기록이 없는 경로(plugin을 session 도중 켰거나 crash)는 `원본 불확실`이고 before는 빈 내용이다 (전부 추가로 보인다).
- session view의 비교 결과(`same`, numstat)를 (path, len, mtime) key로 cache한다 (`SessionViewCache`, 4096개에서 비움). 변경이 없는 refresh는 파일을 다시 읽지 않는다.
- `prompt-submit`/`session-end`는 meta가 없으면 만든다 (session 도중 plugin을 켠 경우). `turn-end`는 meta가 없으면 아무것도 하지 않는다.
- turn 0(첫 prompt 전의 event)은 turn 1에 합쳐 보인다. turn 1을 닫을 때 turn 0 경로도 snapshot한다.
- split|unified 토글을 header가 아니라 탭 줄 오른쪽으로 옮겼다 (spec 6절 그림대로).
- sidebar ●는 `['repos']` refetch(60초 interval + 파일 event)로 갱신된다. 30분 비활성이 되는 순간을 알리는 timer event는 없다.

## 4단계 알려진 한계

- hook 시간 제약: SessionEnd는 timeout이 1.5초다. 큰 레포에서는 pre-bash(`git status`)가 느려질 수 있다.
- 추적하지 못하는 것: ignored 파일, session cwd 밖의 레포, `run_in_background` 또는 중단된 Bash가 만든 변경(놓칠 수 있다).
- Stop 뒤에 도착한 edit는 이미 snapshot한 turn에 들어가 다음 turn diff에 섞인다.
- LFS·eol filter가 걸린 파일은 `git show` 결과(clean 형태)가 working tree와 byte로 안 맞을 수 있다.
- Bash가 바꾼 파일이 1MB를 넘으면 `원본 불확실` 표시를 잃는다 (too_large로 기록된다).
- Bash가 만든 새 commit을 같은 Bash 안에서 amend/rebase하면 놓친다. committer time이 pre-bash보다 앞서면(`GIT_COMMITTER_DATE`) 무시한다.
- 앱은 `WTBOARD_CONFIG`가 상대 경로면 상대 data dir을 쓴다 (hook은 거부한다).
- sidebar ●는 최대 60초 늦을 수 있다.
- hook binary는 앱 번들에 들어 있지 않다. `scripts/install-hook.sh`로 따로 설치해야 한다.
- 최종 리뷰에서 보류한 것:
  - Bash가 commit으로 gitlink를 넣으면(`git init inner && git add -A && git commit`, submodule bump) 빈 baseline file이 생겨 그 아래 경로의 pre-edit가 ENOTDIR로 실패한다.
  - `SessionViewCache::numstat`이 `same: false`를 새 key로 저장해, `items()`와 `numstat` 사이에 되돌려진 파일은 다음 변경 전까지 `(0, 0)`으로 남는다.
  - cache key에 racy-mtime guard가 없어, mtime 해상도가 낮은 volume이나 mtime을 보존하는 도구가 만든 같은 크기 수정은 놓친다.

## 5단계 결과

- 줄 리뷰 코멘트(`state.sqlite`의 `comments`), `wtboard-hook prompt-submit`의 자동 첨부(`additionalContext`), 코멘트 목록 popover와 "마크다운으로 복사", sidebar 💬n, 메뉴바(tray) 미확인 수와 worktree 바로 가기, 창 닫기 = 숨기기, 항상 위에를 추가했다. `state.sqlite`에 `PRAGMA user_version`을 넣었다(schema 1).
- 새 command: `list_comments`, `add_comment`, `update_comment`, `delete_comment`, `clear_sent_comments`, `copy_comments`, `mark_comments_sent`. 새 event: `select-worktree`. `WorktreeSummary.unsentComments`, `list_repos`의 `autoAttachComments`.
- fixture(`/private/tmp/claude-501/...`, `WTBOARD_CONFIG`)에서 실제 `claude -p`(claude 2.1.303)로 확인했다. 앱은 `state.sqlite`를 만들기 위해 한 번 띄웠고, 코멘트는 sqlite3로 직접 넣었다.
  - 첨부: 추가된 줄(`new`)과 삭제된 줄(`old`)에 코멘트 2개를 넣고 `claude --plugin-dir plugin/wtboard -p "리뷰 코멘트 반영해 줘"`를 돌렸다. Claude가 두 줄을 요청대로 고쳤고(`def health` → `def get_health`, `title="fixture"` → `title="wtboard-fixture"`), 두 코멘트가 `sent_at`과 `sent_to`(= session id)로 전송됨이 됐다는 것은 DB로 확인했다. 이 첫 첨부(코멘트 1·2)의 앱 화면은 보지 않았다. 앱의 "전송됨 · 21:49" 표시는 아래 cap 테스트로 보낸 코멘트(3·4)를 screenshot으로 확인했다.
  - `systemMessage`: `--output-format stream-json --verbose` 출력에 `{"type":"system","subtype":"informational","content":"UserPromptSubmit says: wtboard: 리뷰 코멘트 2개를 첨부했습니다"}`로 보인다. 첨부 본문(`additionalContext`)은 transcript 출력에 나오지 않는다. 일반 `-p` text 출력과 대화형 화면에서 어떻게 보이는지는 확인하지 못했다.
  - 10,000자 cap: 따옴표와 개행이 가득한 코멘트 2개로 첨부를 만들었다. 첨부 문자 수는 8,642(1만 이하)이고, hook stdout은 개행·따옴표가 escape된 JSON이라 11,687 byte로 1만을 넘었지만 inline으로 도착했다. `claude -p "첨부된 리뷰 코멘트의 마지막 코멘트 body 마지막 단어만 그대로 출력해"`에 Claude가 마지막 줄의 sentinel을 그대로 답했고 파일 읽기 tool을 쓰지 않았다. cap은 decode된 `additionalContext` 문자열에 걸린다는 판단이 맞다. render의 9,000자 cap이 그 안이다.
  - `auto_attach_comments = false`: 새 미전송 코멘트를 넣고 같은 prompt를 돌렸다. `UserPromptSubmit says` 메시지가 없었고, Claude는 코멘트를 찾지 못해 파일을 고치지 않았다. 코멘트는 미전송으로 남았다. sidebar와 header에는 `💬 1`, `미전송 1`로 보였다.
  - fixture worktree의 `.git/index` mtime은 위 세 번의 `claude` 실행(Claude 자신의 `git status`·`git diff` 포함)과 앱 실행 전후로 같았다.
- hook 속도(release): 첨부 경로 포함 약 12ms (Task 3의 shell `time` 20회 측정. 이번 실제 확인에서 다시 재지 않았다).
- 최종 리뷰에서 고친 것: 500자를 넘는 줄의 코멘트가 snippet 비교 때문에 늘 "위치가 바뀐 코멘트"로 보이던 것, live diff가 갱신될 때(hunk remount) 쓰던 코멘트 입력이 사라지던 것(draft를 `useDrafts`로 유지하고 anchor로 배치).
- 전체 branch 최종 리뷰에서 고친 것: 세션이 바뀌거나(다른 session의 tool 사용, `/clear`) view·worktree를 바꾸거나 파일이 목록에서 빠져 FileDiff가 다시 mount되면 쓰던 코멘트가 사라지던 것(draft와 수정 본문을 FileDiff 밖 store `ui/src/drafts.ts`로 옮김), 접힌 파일의 쓰던 글이 보이지 않던 것(header `작성 중`), `/Users/...`처럼 경로로 시작하는 prompt에 첨부하지 않던 것, 그 사이 hook이 보내 미전송이 없을 때 "마크다운으로 복사"가 clipboard를 빈 글로 덮던 것, 코멘트 목록이 삭제된 줄의 옛 줄 번호를 표시 없이 보이던 것.

## 5단계에서 정한 것 (spec과 다른 점)

- spec 3절의 "JSON escape 뒤 길이도 10,000자 이하" 재검사는 하지 않는다. 위 실제 확인에서 escape 뒤 길이가 1만을 넘는 첨부(stdout 11,687 byte)도 그대로 도착했다. render의 문자(char) cap 9,000자만 쓴다.
- 코멘트 하나가 cap을 넘더라도 그 코멘트가 첫 번째면 포함한다 (아니면 진행이 없다). body 4,000자 + snippet 500자라 실제로는 거의 없다.
- 남은 수 안내("코멘트 n개가 더 있습니다")는 닫는 tag 안쪽, 마지막에 둔다.
- `StoreError`에 `Emit`을 추가했다 (hook이 출력에 실패하면 commit하지 않는다).
- `core::store`를 `db`(열기·migration), `comments`, `settings`, `viewed`, `config`로 나눴다. hook용 `open_existing(path, busy)`는 DB를 만들지 않는다.
- 작업은 `main`이 아니라 branch `stage5-review-comments`에서 했다. 끝나면 `main`에 fast-forward한다.
- 메뉴바는 모델(미확인 합계·worktree 줄)이 바뀌지 않으면 메뉴를 다시 만들지 않는다. 갱신은 1초에 한 번으로 묶는다.
- 사용자가 직접 확인하지 못한 항목은 아래 "5단계 사용자 확인"에 둔다.
- 쓰던 새 코멘트는 worktree·path별로 하나씩, 수정 본문은 comment id별로 메모리 store(`ui/src/drafts.ts`, 저장하지 않음)에 둔다. 새 코멘트는 연 view를 함께 둬서 그 view로 저장하고, `old` side는 그 view에서만 줄에 맞춘다 (다른 view에서는 "위치가 바뀐 코멘트" 묶음에 보인다).
- slash command 판정: 첫 단어가 `/`로 시작하고 그 뒤에 `/`가 없을 때만 명령으로 보고 첨부를 건너뛴다.

## 5단계 사용자 확인

실제 앱의 키 입력과 menu bar 클릭이 필요해 사용자가 fixture 앱에서 직접 확인했다 (2026-10-19).

- [x] 새 코멘트 입력: 줄 gutter의 `+` → 글 입력 → Cmd+Enter 또는 `저장` → 미전송 widget과 header `💬 미전송 n`.
- [x] 메뉴바 메뉴를 열어 worktree 줄을 고르면 창이 앞으로 오고 그 worktree가 선택된다 (`select-worktree`). 숫자(title)는 구현 중 screenshot으로 봤다.
- [x] 창 닫기(빨간 버튼) → Dock 아이콘 클릭으로 다시 열기.
- [x] "항상 위에 띄우기" 토글 → 앱 재시작 뒤 유지. 토글이 `settings.always_on_top`에 저장되는 것을 DB로 확인했다.
- [ ] (미확인) 대화형 `claude`에서 `systemMessage`("wtboard: 리뷰 코멘트 n개를 첨부했습니다")가 화면에 보이는지.

## 5단계 알려진 한계

- hook이 옛 binary면 자동 첨부가 없다. 앱이 옛 binary를 감지하지 않으므로 `scripts/install-hook.sh`를 다시 실행해야 한다.
- UserPromptSubmit은 사용자가 친 prompt가 아닌 입력(scheduled task, background subagent 보고)에서도 불려 그때도 첨부한다. slash command(첫 단어가 `/`로 시작하고 그 뒤에 `/`가 없는 prompt)만 건너뛴다.
- 같은 worktree에서 session이 둘이면 먼저 prompt를 보낸 쪽에 붙는다. 화면은 session을 구분하지 않는다.
- 앱이 DB를 열지 못해 메모리 DB를 쓰는 동안 단 코멘트는 hook이 보지 못한다.
- worktree가 지워져도 코멘트 row는 남는다 (전송된 것은 `keep_days` 뒤 지워진다). 레포가 하나도 없으면 오래된 코멘트도 지워지지 않는다.
- 코멘트가 가리키는 줄이 바뀌면 "위치가 바뀐 코멘트"로 모인다.
- 입력 중인 코멘트(draft)는 앱을 끄면 사라진다. 파일이 목록에서 빠진 채 돌아오지 않으면 그 draft는 메모리에 남아 보이지 않는다. 글이 있는 draft가 있으면 다른 줄의 `+`가 아무 반응 없어 보인다. IME 조합 중 hunk가 remount되면 조합 중인 글자를 잃을 수 있다.
- 입력 한 글자마다 diff widget 전체를 다시 그린다 (큰 diff에서 typing이 느릴 수 있다).
- 저장 직후 refetch 전에 widget이 잠깐 깜빡일 수 있다. popover의 copy/clear 오류는 다시 열어도 남는다.
- 접근성: `+` 버튼은 keyboard로 못 누르고, popover에 aria 속성과 Esc 닫기가 없고, textarea에 label이 없다.
- `select-worktree`가 `['repos']` cache가 생기기 전에 오면 버려진다. 앱을 띄운 직후 메뉴바에서 worktree를 고르면 반응이 없을 수 있다.
- 메뉴바: 메뉴 갱신이 실패해도 갱신한 것으로 기록해 다음 변경까지 stale하다. `set_always_on_top`이 실패하면 check 표시가 어긋난 채 남는다. settings가 열리지 않으면(레포 없음 등) 항상 위에 저장이 조용히 빠진다. 토글이 main thread에서 SQLite에 쓴다 (busy 최대 5초).
- `list_repos`의 미전송 수 query가 실패하면 0으로 보인다. 깨진 `state.sqlite`는 경고를 3개(봤음, 코멘트, 설정) 띄운다.
- hook: attach가 panic하면 turn 기록도 건너뛴다. DB 존재를 확인하기 전에 `git rev-parse`(약 6ms)를 먼저 한다.
- `PRAGMA journal_mode = WAL`의 결과를 확인하지 않는다. 다른 연결이 DB를 잡고 있어 WAL로 바꾸지 못하면 rollback journal로 조용히 진행한다.
- 코멘트 `삭제`는 확인 없이 바로 지운다.

## 6단계 결과

- `state.sqlite` schema 2 (`comments.sent_turn`, `comments.resolved_at`). 코멘트 상태가 미전송 → 전송됨 → 해결됨이 되고, `다시 보내기`로 미전송에 되돌린다. hook `prompt-submit`이 `sent_turn`(그 prompt가 연 turn)을 적고, 앱은 `list_comments`마다 그 세션의 event에서 코멘트별 `replyTurn`(`sent_turn` 이후 그 파일을 건드린 가장 큰 turn)을 붙인다. hook `version` subcommand와 앱 시작 때의 옛 binary 경고를 더했다.
- 새 command: `resolve_comment`, `unresolve_comment`, `resend_comment`. `list_comments`는 `CommentItem`(`Comment` + `replyTurn`).
- fixture(`WTBOARD_CONFIG`, debug hook `WTBOARD_HOOK_BIN`)에서 실제 `claude -p`(claude 2.1.303)로 확인했다 (2026-10-21).
  - turn 1: 코멘트 2개(`app/main.py:7`, `app/users.py:2`)를 첨부하고 "1번만 반영"을 시켰다. Claude가 main.py의 `health`를 `get_health`로 바꿨다. DB `sent_turn = 1` 둘 다. 앱: header `💬 응답 1`, main.py 코멘트는 줄이 바뀌어 "위치가 바뀐 코멘트"에 `응답 #1 보기`와 함께, users.py 코멘트는 응답 버튼 없이 `전송됨`.
  - `응답 #1 보기` → 마지막 응답 #1 탭으로 가서 그 변경만 보였다.
  - users.py 코멘트 `다시 보내기` → 미전송 → `claude --resume <id> -p` → `sent_turn = 2`, meta turn 2. 응답 뒤 `응답 #2 보기`(마지막 응답 #2 → users.py 변경), main.py의 `응답 #1 보기`는 마지막 turn이 아니라 "이번 세션"으로 갔다. header `💬 응답 2`.
  - `해결됨`·`되돌리기`·`다시 보내기` widget·popover 동작은 seed한 코멘트로 screenshot 확인했다 (해결됨은 diff에서 사라지고 popover "해결된 코멘트 n개"에만 보인다).
  - 옛 hook binary 경고: 실제 설치된 `~/.wtboard/bin/wtboard-hook`(5단계 build, schema 1)이 sidebar 경고로 잡혔다.
  - fixture `.git/index` mtime은 `claude` 2회와 앱 실행 전후로 같았다.
  - 확인 중 관찰: UI를 HMR로 고치던 중 React tree가 잠시 죽은 사이 온 `worktree-changed`가 유실되어 branch view의 users.py patch가 옛 내용으로 남았다 (`staleTime: Infinity`). 그 파일에 다음 event가 오면 돌아온다. 개발 중 HMR에서만 나는 일이다.
- hook 속도는 다시 재지 않았다 (첨부 경로에 `meta.json` 읽기 1회가 더해졌다).

## 6단계에서 정한 것 (spec과 다른 점)

- `SCHEMA_VERSION`은 `MIGRATIONS.len()`이다. migration을 더하면 같이 오른다.
- hook `version` 줄의 format과 parser(`hook_version_line`, `parse_hook_schema`)는 `core::store::db`에 한 쌍으로 둔다 (round-trip test). hook binary 경로 규칙(`hook_binary_path`)은 `core::store::config`에 두고 run.sh와 같은 규칙을 test한다.
- 앱의 hook 확인은 별도 thread + 2초 timeout(`check_with_timeout`)이다. 멈춘 binary가 registry 열기를 막지 않는다. binary가 없으면(`NotFound`) 경고가 없고, 실행 오류(권한 등)는 경고 뒤에 붙인다.
- `reply_turns`는 해결됨 코멘트를 건너뛴다 (화면이 응답을 보이지 않는다). `copy`는 `sent_turn`이 없어 저절로 빠진다. `SENT_TO_COPY` 상수는 앱의 복사 경로가 쓴다.
- `touched_turns`는 `turns_by_rel`로 `session::view::items`와 묶기를 공유한다.
- UI: 코멘트 action 7개(`onAdd` … `onOpenReply`)를 `CommentActions` 객체 하나로 내려보낸다. 응답이 있는 코멘트가 없으면 세션 목록이 바뀌어도 그 객체를 새로 만들지 않는다 (FileDiff memo 유지).
- popover 목록(`CommentList`)은 열려 있을 때만 mount된다. 복사·지우기 오류는 닫았다 열면 사라진다 (5단계 한계 하나가 없어졌다).
- `해결됨`·`되돌리기`는 `['repos']`를 다시 읽지 않는다 (미전송 수가 안 바뀐다).
- `replyTarget`은 `hasReply`(전송됨·미해결·응답 있음)일 때만 있고, 세션이 이 worktree 목록에 없으면 없다 (`pickSession`이 첫 세션으로 바꿔 버려 이동이 안 된다).
- 스크롤 예약(`ui/src/nav.ts`)은 slot 하나다. 다른 worktree의 요청은 그 worktree가 선택될 때까지 남는다.
- **사용자가 할 일: `scripts/install-hook.sh`를 다시 실행한다** (schema 2 hook). 그 전까지 sidebar에 경고가 보이고 자동 첨부가 되지 않는다.

## 6단계 알려진 한계

- 응답 판정은 파일 단위다. 그 turn이 그 파일을 건드렸으면 `응답 #n`이고, 그 줄을 고쳤는지는 기존 위치 맞추기(snippet)가 보여 준다 ("위치가 바뀐 코멘트"면 줄이 바뀐 것). 되돌려진 변경(before == after)도 건드린 것으로 센다.
- `복사됨` 코멘트는 응답이 없다 (어느 세션에 붙였는지 모른다).
- 세션 기록이 `keep_days`로 지워지면 `응답 #n`이 사라진다. 코멘트는 남는다.
- 응답 turn이 마지막 turn이 아니면 `응답 #n 보기`는 "이번 세션"(누적)으로 간다. turn 하나만 보는 화면이 없다.
- 옛 hook binary 경고는 앱 시작 때 한 번이다. 시작 뒤에 binary를 바꿔도 다시 시작해야 사라진다. 새 hook(schema 2)이 아직 version 1인 DB(새 앱을 한 번도 안 띄움)를 만나면 첨부하지 않는다.
- `list_comments`마다 전송된 코멘트의 세션 `events.jsonl`을 다시 읽는다 (cache 없음).
- 해결됨 코멘트에는 widget이 없다 (popover에서만 되돌린다). `다시 보내기`·`해결됨`에 확인이 없다 (삭제와 같다).
- 세션 목록의 내용이 바뀌면(session event 등. 60초 refetch는 내용이 같으면 참조가 유지된다) 응답이 있는 코멘트가 하나라도 있을 때 FileDiff 전부가 다시 그려진다.
- `prompt-submit`에서 첨부(`sent_turn = N`)는 commit됐는데 뒤이은 `write_meta`가 실패하면(disk full 등) meta는 N-1에 머물러 그 turn의 edit event가 N-1로 적히고 `응답 #N`이 잡히지 않는다 (4단계 corrupt meta 한계와 같은 부류).

## 7단계 후보

- spec 범위 밖에 둔 것: 여러 줄 범위 코멘트, 파일 단위 코멘트, 답글(thread), 줄 단위 응답 판정, 알림(notification center).
- hook binary를 앱 번들에 포함하고 설치를 앱에서 안내 (감지·경고는 6단계에서 했다).
- fork 레포의 PR base: `remote_branch`가 `origin`만 본다 (`upstream` remote 미지원).
- 파일 event마다 sidebar 갱신을 위해 모든 worktree summary를 다시 계산한다 (worktree당 git 2회: branch, fsmonitor). 레포가 많아지면 branch·fsmonitor cache를 검토.
- 4단계 보류 항목(gitlink commit, numstat cache `same: false` race, racy-mtime guard)의 수정. gitlink는 committed diff에 `--ignore-submodules=all`을 주는 방향.
- 5단계 미확인 항목: 대화형 `claude`에서 `systemMessage`가 보이는지.

## 미룬 Minor (최종 리뷰)

- `useInView`가 viewport 기준이라 스크롤 컨테이너 안에서 `rootMargin`이 효과 없음. 보일 때 로드되며 layout이 밀린다.
- `Git::command`가 상속된 `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`을 지우지 않는다.
- poll/목록 재계산마다 untracked 파일(≤1MB)을 전부 다시 읽는다.
- `git rm --cached` 파일이 D와 untracked 양쪽에 나와 untracked가 덮어쓴다.
- revert로 목록에서 빠진 파일이 잠깐 `UnknownFile` 오류로 깜빡일 수 있다.
- vite 번들 500kB 초과 경고 (refractor 전체 언어 포함).
- (3단계) `useSetViewed` hook과 component 단위 UI test가 없다 (화면은 실제 앱으로 확인).
- (3단계) 봤음 checkbox에 파일별 `aria-label`이 없고, FileTree의 ☑/●에 text alternative가 없다.
- (3단계) `state.sqlite` schema에 `user_version`이 없다. schema를 바꾸게 될 때 같이 넣는다.
- (3단계) `rev-parse`가 일시적으로 실패하면 detached(branch `''`)로 보고, 다음 HEAD event까지 체크가 숨는다.
- (3단계) 겹치는 `set_viewed` mutation은 서로 되돌리며 깜빡일 수 있다 (settle 뒤 맞춰진다).
- (3단계) `refresh_folders`는 `apply_paths`가 실패하면 List event를 버린다.
- (4단계) session 기록: partial append가 개행 없는 조각을 남길 수 있다. corrupt meta는 없는 것으로 보고 turn 0으로 덮어쓴다. non-UTF-8 경로가 snapshot index를 만들다 실패할 수 있다. `snapshot_file`의 stat→open 사이에 1MB를 넘게 자란 파일을 복사할 수 있고, `NotADirectory`를 Deleted가 아니라 오류로 본다.
- (4단계) Bash 추적: 개행·끝 CR이 든 경로의 clean 파일은 sticky New baseline을 받는다. post-bash는 `rev-parse` 때문에 8~10ms 더 든다. 큰 blob은 `read_blobs`가 메모리에 올린다. `/var`와 `/private/var` 표기가 pre-edit와 post-bash 기록에서 갈릴 수 있다.
- (4단계) session view: `worktrees_of`와 touched 판정이 경로 == root일 때 다르다. relabel이 mode 줄을 버려 chmod 변경이 보이지 않는다. numstat 실패 분기가 `same_as` short-circuit 때 `uncertain` 없이 목록에 올린다. `active_roots`는 `all_sessions`가 dir만 거르지 않는다.
- (4단계) UI: `listSessions` 오류가 "세션 없음"으로 보인다. 기억된 session·turn 탭이 있으면 cold start에 branch view가 잠깐 보인다. 첫 prompt 전에 `마지막 응답 #0`이 보인다. 탭·select에 a11y 속성이 없다. persisted `views`/`sessions` map을 지우지 않는다. `setView`/`selectSession`/`useWorktreeEvents`에 test가 없다.
- (4단계) plugin: `WTBOARD_HOOK_BIN`이 실행 불가일 때의 test가 없다.
- (6단계) 스크롤 예약 slot(`nav.ts`)이 view·세션을 모르므로, `응답 #n 보기`가 그 파일이 없는 view(되돌려진 변경은 session view 목록에서 빠진다)에 도착하면 요청이 남았다가 나중에 branch view로 돌아올 때 스크롤된다. 요청에 view·session id를 넣어 다르면 버리면 된다.
- (6단계) `hook::check_with_timeout`이 timeout 때 child process를 죽이지 않는다 (`spawn` + `try_wait` + `kill`로 바꾸면 된다).
- (6단계) 옛 hook 경고 문구가 앱보다 새 hook(schema 3)에도 "옛 version"으로 나온다. 찾은 schema 번호를 문구에 넣으면 낫다.
- (6단계) header `💬 미전송 a · 응답 b` 버튼에 spec 6절의 `title`(풀어 쓴 설명)이 없다.
- (6단계) popover row가 `<button>`에서 `<li onClick>`으로 바뀌어 키보드로 "파일로 가기"를 못 한다 (`role="button"` + Enter/Space).
- (6단계) `list_comments`마다 세션별 `events.jsonl`을 다시 읽는다. (session id, mtime·len) key cache를 `SessionViewCache`처럼 두면 된다.
