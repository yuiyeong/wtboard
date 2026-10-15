# 진행 상황

설계: `docs/draft.md`. 단계별 plan: `docs/superpowers/plans/`.

## 단계

| 단계 | 상태 | plan |
|---|---|---|
| 1. 레포 1개·worktree 1개 브랜치 전체 뷰 + 실시간 갱신 | 완료 (2026-10-15) | `plans/2026-10-15-stage1-branch-view.md` |
| 2. 여러 레포·worktree 자동 발견 + 사이드바 + 프로필 3종 | 다음 | 아직 없음 |
| 3. 봤음 체크 + 다시 바뀜 표시 | — | — |
| 4. plugin(hooks) + 이번 세션 / 마지막 응답 탭 | — | — |
| 5. 리뷰 코멘트 자동 첨부 + 메뉴바 | — | — |

## 1단계 결과

- `omofictions-back-1371`(`back/feat/1371`, base `origin/develop`, 변경 파일 617개)으로 화면 확인.
- plan 이후 추가: sticky 파일 헤더를 스크롤 영역 맨 위에 붙임, 왼쪽 목록을 디렉터리 트리로 변경 (단일 자식 폴더는 한 줄로 합침, diff 순서도 트리 순서).
- 최종 리뷰에서 고친 것: `git diff`의 index auto-refresh(`diff.autoRefreshIndex=false`), HEAD 변경 → merge-base 재계산, 큰 event batch(>200 경로) → 목록 재계산, untracked symlink/FIFO를 따라가 읽지 않음.

## 1단계에서 정한 것 (plan과 다른 점)

- watcher는 spec 4절과 달리 `app`이 아니라 `core::watch`에 있다 (Tauri 없이 테스트하려고).
- hash는 화면이 patch를 가져간 파일만 저장한다. 3단계(봤음 체크)에서 "봤음 체크한 파일"까지 넓혀야 한다.
- 커밋 감지는 HEAD 파일이 아니라 refs 감시가 담당한다 (spec 8절 표 보정).
- rename 감지 없음 (`--no-renames`, 삭제 + 추가로 보임).

## 2단계로 넘어간 것

- worktree 자동 발견(`git worktree list --porcelain`) + common dir `worktrees/` 감시. **지금은 `~/.wtboard/config.toml`의 첫 번째 `[[repo]]` 경로 하나만 연다** (현재 `~/projects/omofictions-back-1371`).
- 여러 레포, 사이드바.
- 프로필별 제외 폴더(`build`, `out`, `ios/Pods`)와 생성 파일 접기, `linguist-generated`.
- base 결정에 `gh pr view` 추가.
- 큰 레포용 `core.fsmonitor` 안내.

## 미룬 Minor (최종 리뷰)

- `fallback_poll_secs = 0`이면 busy loop. 최소값 clamp 필요.
- `useInView`가 viewport 기준이라 스크롤 컨테이너 안에서 `rootMargin`이 효과 없음. 보일 때 로드되며 layout이 밀린다.
- async Tauri command가 blocking git/Mutex를 tokio worker에서 실행. `spawn_blocking` 권장.
- `Git::command`가 상속된 `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`을 지우지 않는다.
- poll/목록 재계산마다 untracked 파일(≤1MB)을 전부 다시 읽는다.
- `git rm --cached` 파일이 D와 untracked 양쪽에 나와 untracked가 덮어쓴다.
- revert로 목록에서 빠진 파일이 잠깐 `UnknownFile` 오류로 깜빡일 수 있다.
- vite 번들 500kB 초과 경고 (refractor 전체 언어 포함).
