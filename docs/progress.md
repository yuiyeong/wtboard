# 진행 상황

설계: `docs/draft.md`. 단계별 plan: `docs/superpowers/plans/`.

## 단계

| 단계 | 상태 | plan |
|---|---|---|
| 1. 레포 1개·worktree 1개 브랜치 전체 뷰 + 실시간 갱신 | 완료 (2026-10-15) | `plans/2026-10-15-stage1-branch-view.md` |
| 2. 여러 레포·worktree 자동 발견 + 사이드바 + 프로필 3종 | 완료 (2026-10-15) | `plans/2026-10-15-stage2-multi-repo.md` (spec: `specs/2026-10-15-stage2-multi-repo-design.md`) |
| 3. 봤음 체크 + 다시 바뀜 표시 | 완료 (2026-10-19) | `plans/2026-10-19-stage3-viewed.md` (spec: `specs/2026-10-19-stage3-viewed-design.md`) |
| 4. plugin(hooks) + 이번 세션 / 마지막 응답 탭 | 완료 (2026-10-19) | `plans/2026-10-19-stage4-sessions.md` (spec: `specs/2026-10-19-stage4-sessions-design.md`) |
| 5. 리뷰 코멘트 자동 첨부 + 메뉴바 | 다음 | 아직 없음 |

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

## 5단계로 넘어간 것

- fork 레포의 PR base: `remote_branch`가 `origin`만 본다 (`upstream` remote 미지원).
- 파일 event마다 sidebar 갱신을 위해 모든 worktree summary를 다시 계산한다 (worktree당 git 2회: branch, fsmonitor). 레포가 많아지면 branch·fsmonitor cache를 검토.
- hook binary를 앱 번들에 포함하고 설치를 앱에서 안내.
- 4단계 보류 항목(gitlink commit, numstat cache `same: false` race, racy-mtime guard)의 수정. gitlink는 committed diff에 `--ignore-submodules=all`을 주는 방향.

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
