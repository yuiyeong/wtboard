# 진행 상황

설계: `docs/draft.md`. 단계별 plan: `docs/superpowers/plans/`.

## 단계

| 단계 | 상태 | plan |
|---|---|---|
| 1. 레포 1개·worktree 1개 브랜치 전체 뷰 + 실시간 갱신 | 완료 (2026-10-15) | `plans/2026-10-15-stage1-branch-view.md` |
| 2. 여러 레포·worktree 자동 발견 + 사이드바 + 프로필 3종 | 완료 (2026-10-15) | `plans/2026-10-15-stage2-multi-repo.md` (spec: `specs/2026-10-15-stage2-multi-repo-design.md`) |
| 3. 봤음 체크 + 다시 바뀜 표시 | 다음 | 아직 없음 |
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

## 3단계로 넘어간 것

- hash는 화면이 patch를 가져간 파일만 저장한다. 봤음 체크한 파일까지 넓혀야 한다 (1단계에서 넘어옴).
- fork 레포의 PR base: `remote_branch`가 `origin`만 본다 (`upstream` remote 미지원).
- 파일 event마다 sidebar 갱신을 위해 모든 worktree summary를 다시 계산한다 (worktree당 git 2회: branch, fsmonitor). 레포가 많아지면 branch·fsmonitor cache를 검토.

## 미룬 Minor (최종 리뷰)

- `useInView`가 viewport 기준이라 스크롤 컨테이너 안에서 `rootMargin`이 효과 없음. 보일 때 로드되며 layout이 밀린다.
- `Git::command`가 상속된 `GIT_DIR`/`GIT_WORK_TREE`/`GIT_INDEX_FILE`을 지우지 않는다.
- poll/목록 재계산마다 untracked 파일(≤1MB)을 전부 다시 읽는다.
- `git rm --cached` 파일이 D와 untracked 양쪽에 나와 untracked가 덮어쓴다.
- revert로 목록에서 빠진 파일이 잠깐 `UnknownFile` 오류로 깜빡일 수 있다.
- vite 번들 500kB 초과 경고 (refractor 전체 언어 포함).
