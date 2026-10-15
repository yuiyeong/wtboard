# 2단계 설계: 여러 레포·worktree 자동 발견 + sidebar + profile

상위 설계: `docs/draft.md` (6절, 7절, 10절, 13절). 이 문서는 2단계에서 draft와 달라진 점과 구체 설계를 적는다.

## 목표와 완료 기준

- config에 등록한 모든 레포의 모든 worktree가 sidebar에 보이고, 각 worktree의 변경 파일 수가 실시간으로 바뀐다.
- Python / Node / Flutter worktree에서 생성·lock 파일이 접혀 있다.
- `git worktree add` / `remove`가 1초 안에 sidebar에 반영된다.
- PR이 있는 브랜치는 PR base를 쓰고, header에 `(PR)`로 표시한다.
- 목록 계산이 느린 레포에는 `core.fsmonitor` 안내 banner가 뜬다.

범위 밖: 미확인 수(3단계), 활성 세션 표시(4단계), config 파일 감시(재시작해야 반영).

## draft와 달라진 점

| draft | 2단계 | 이유 |
|---|---|---|
| profile = FastAPI / Next / Flutter | profile = Python / Node / Flutter (+ 기본) | 실제 back·worker 레포는 `pyproject.toml` 없이 `requirements.txt` + `venv/`를 쓰고, worker는 FastAPI가 아니다. front·admin은 `package-lock.json`을 쓴다 |
| fsmonitor 안내는 설정 화면 | 목록 계산이 500ms를 넘는 레포에만 banner | 설정 화면이 없고, 빠른 레포에는 안내가 필요 없다 |
| watcher는 `app`에 | `core::watch`에 (1단계 결정 유지) | Tauri 없이 test |

## 1. core 구조

| 단위 | 하는 일 | 의존 |
|---|---|---|
| `git::worktrees` (신규) | `git worktree list --porcelain` 파싱. `prunable`, `bare`, 폴더 없는 entry는 제외. detached HEAD는 포함 | `Git` |
| `gh` (신규) | `gh pr view --json baseRefName` 실행. 실패하면 `None` | 외부 `gh` |
| `git::base` (수정) | base 결정: config `base` → gh PR base → `origin/HEAD` | `Git`, `gh` |
| `git::attrs` (신규) | `git check-attr linguist-generated -z --stdin`으로 생성 파일 판정 | `Git` |
| `profile` (수정) | worktree root의 marker 파일로 profile 판별. 제외 폴더 pattern과 접기 pattern 제공 | 파일 시스템 |
| `Worktree` (수정) | `ChangedFile.collapsed` 계산. 목록 계산 시간, 마지막 변경 시각 기록 | 위 전부 |
| `Repo` (신규) | 레포 하나: common dir, worktree들과 각 watcher. `rediscover()`가 worktree 목록 차이만큼 열고 닫는다 | `Worktree`, `watch` |
| `Registry` (신규) | config entry를 common dir로 묶어 `Repo`를 만든다. 실패한 entry는 오류로 보관 | `Repo`, `Config` |
| `watch` (수정) | 제외 pattern을 profile에서 받는다. 레포별 `<common dir>/worktrees/` 감시 추가 | `profile` |

1단계의 hash 비교, 감시 3군데, 읽기 전용 규칙은 그대로다. `Worktree` 하나의 동작은 1단계와 같다.

### profile

worktree마다 그 root에서 판별한다 (브랜치마다 다를 수 있다). 위에서부터 처음 맞는 것을 쓴다.

| profile | marker | 제외 폴더 | 접기 |
|---|---|---|---|
| Flutter | `pubspec.yaml` | `.dart_tool`, `build`, `ios/Pods`, `.fvm` | `*.g.dart`, `*.freezed.dart`, `pubspec.lock`, `Podfile.lock`, `GeneratedPluginRegistrant.*` |
| Node | `package.json` | `node_modules`, `.next`, `out`, `.turbo` | `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `next-env.d.ts` |
| Python | `pyproject.toml` 또는 `requirements.txt` | `venv`, `.venv`, `__pycache__`, `.pytest_cache`, `.ruff_cache`, `.mypy_cache` | `uv.lock`, `poetry.lock` |
| 기본 | 판별 실패 | 공통 제외만 | 없음 |

- 모든 profile에 공통 제외 폴더(1단계 `COMMON_EXCLUDED_DIRS`)를 더한다.
- 제외 pattern은 경로 segment 단위로 맞춘다. `ios/Pods`는 연속된 두 segment `ios`, `Pods`와 맞고 깊이는 상관없다.
- 접기 pattern은 파일 이름(마지막 segment)에 glob으로 맞춘다.
- 제외 폴더는 감시 event만 거른다. 그 안의 tracked 파일이 바뀌어도 fallback poll이 잡는다.
- sidebar의 레포 badge는 main worktree의 profile이다. main이 bare면 첫 worktree의 profile.

### 접기 판정

`collapsed = profile 접기 pattern에 맞음 || linguist-generated`. core에서 계산하고 UI는 값만 본다. `check-attr`는 목록 전체를 다시 계산할 때 전체 경로로, 파일 event 때는 그 파일만 묻는다. `check-attr`가 실패하면 생성 파일이 아닌 것으로 보고 목록 계산은 계속한다.

### base와 gh

- config에 `base`가 있으면 gh를 부르지 않는다.
- 없으면 worktree를 `origin/HEAD`로 먼저 열고, background에서 gh를 부른다. 결과가 현재 base와 다르면 `refresh_base`를 하고 `scope: base` event를 보낸다.
- gh는 worktree를 열 때와 HEAD가 바뀔 때(브랜치 전환) 다시 부른다.
- PR base가 `develop`이면 `origin/develop`이 `rev-parse --verify`로 확인될 때만 쓴다.
- `WorktreeSummary.baseSource`는 `config` / `pr` / `origin`.

gh 실행 규칙 (실패는 모두 조용히 `None`):
- Finder에서 띄운 앱은 PATH에 `/opt/homebrew/bin`이 없다. `gh`를 PATH → `/opt/homebrew/bin` → `/usr/local/bin` 순으로 찾는다.
- `GH_PROMPT_DISABLED=1`, timeout 5초 (넘으면 kill).
- gh 없음, 로그인 안 됨, PR 없음, timeout, 파싱 실패 → `None`.
- test를 위해 gh binary 경로를 주입받는다.

## 2. 데이터 흐름

### 시작

1. `setup`은 창을 막지 않는다. background thread에서 `Registry`를 만들고 끝나면 `worktrees-updated`를 보낸다.
2. 그 전의 `list_repos`는 `ready: false`를 돌려준다.
3. worktree는 병렬로 연다. 하나가 실패해도 나머지는 계속 연다.

### worktree 추가·삭제

1. `<common dir>/worktrees/` event → `Repo::rediscover()` → 새 worktree를 열고 watcher를 붙인다. 사라진 worktree는 `WatchHandle`을 drop한다 → `worktrees-updated`.
2. `rm -rf`로 폴더만 지우면 `worktrees/`에 event가 오지 않는다. worktree 재계산이 root 없음으로 실패하면 그 레포를 rediscover한다. fallback poll 때도 rediscover한다.
3. rediscover는 레포 단위 mutex로 직렬화한다.

### Tauri interface

| 종류 | 이름 | 내용 |
|---|---|---|
| command | `list_repos` | `{ ready, repos: RepoSummary[], errors: { path, message }[] }` |
| | `RepoSummary` | `id`, `name`, `profile`, `worktrees` |
| | `WorktreeSummary` | 1단계 + `baseSource`, `lastChangedAt`, `slowListMs`, `fsmonitor`, `error` |
| command | `list_files(worktree_id, view)` | `ChangedFile`에 `collapsed` 추가 |
| command | `get_patch(worktree_id, view, path)` | 그대로 |
| event | `worktree-changed` | 그대로. UI는 `repos` query도 invalidate |
| event | `worktrees-updated` | 신규. 시작 완료, worktree 추가·삭제. UI는 `repos` query를 invalidate |

- `lastChangedAt`: 앱이 그 worktree에 마지막으로 `worktree-changed`를 보낸 시각. 시작 직후에는 `null`.
- `slowListMs`: 마지막 목록 계산이 500ms를 넘었을 때만 값이 있다.
- `fsmonitor`: `git config --get core.fsmonitor`가 켜져 있는지.
- command는 `spawn_blocking`에서 실행한다 (1단계 Minor).
- config 경로는 `WTBOARD_CONFIG` env var로 바꿀 수 있다 (fixture로 UI를 확인하기 위함).

## 3. UI

```
┌─────────────────────┬──────────────────┬───────────────────────────────────┐
│ omofictions-back  Py│ omofictions-front · front/feat/1116 ← develop (PR)   │
│   develop      0    │ [split|unified]                                       │
│ omofictions-front  N├──────────────────┬───────────────────────────────────┤
│   develop      0    │ 변경 파일 14개    │ (fsmonitor banner, 느릴 때만)       │
│ ● front/feat/1116 14│ ▾ app/login      │ app/login/page.tsx                │
│              3분 전 │   M page.tsx +40 │ - old       │ + new                │
│ Omofictions-App  Fl │ ▾ lib            │                                   │
│   main         0    │   M user.dart +3 │ ...                               │
│                     │ ▸ 접힌 파일 (3)   │ ▸ lib/user.g.dart     접힘 +120 -4 │
│ ⚠ omofictions-back- │                  │ ▸ package-lock.json   접힘 +88 -12 │
│   1371: 경로 없음     │                  │                                   │
└─────────────────────┴──────────────────┴───────────────────────────────────┘
```

- **sidebar** (약 220px): 레포 줄 = 이름 + profile badge (`Py` / `Node` / `Flutter`, 기본은 없음). worktree 줄 = 브랜치(detached면 폴더 이름), 변경 파일 수, 마지막 변경 시각("3분 전"). 폴더 경로는 hover `title`.
- 선택한 worktree는 배경색으로 표시하고 `localStorage`에 저장한다. 선택한 worktree가 사라지면 같은 레포의 첫 worktree, 레포도 없으면 첫 레포의 첫 worktree로 옮긴다.
- config entry 오류는 sidebar 맨 아래에 ⚠로 보인다. worktree 오류는 그 줄에 ⚠, main 영역에 메시지.
- **header**: 레포 · 브랜치 ← base. `baseSource`가 `pr`이면 `(PR)`.
- **접힌 파일**: file tree 맨 아래 `▸ 접힌 파일 (n)` group (기본 닫힘, 안은 같은 디렉터리 트리). diff 목록에서는 맨 뒤에 header만 보이고, 펼칠 때 patch를 가져온다. 화면에 보여도 펼치기 전에는 계산하지 않는다.
- **fsmonitor banner**: `slowListMs`가 있고 `fsmonitor`가 꺼진 worktree 위에 뜬다. `git config core.fsmonitor true`, `git config core.untrackedCache true`와 각각 복사 버튼, 닫기 버튼. 닫으면 그 레포는 앱 실행 중 다시 띄우지 않는다.

## 4. 오류 처리와 경계 사례

- config entry 경로가 없으면 오류 목록에 넣고 나머지는 계속 연다.
- 두 entry가 같은 common dir이면 하나로 합친다. `base`가 다르면 첫 entry를 따르고 오류 목록에 경고를 남긴다.
- `fallback_poll_secs`는 최소 5초로 clamp한다.
- prunable, bare worktree는 건너뛴다.
- base를 정할 수 없는 worktree(`origin/HEAD` 없음, gh 실패)는 worktree 오류로 보인다.
- fetch로 refs가 바뀌면 같은 레포의 worktree가 모두 merge-base를 다시 계산한다. 레포당 worktree 수 × 약 150ms라 그대로 둔다.

## 5. test

test를 먼저 쓰고 실제 git과 실제 FSEvents를 쓴다.

**core (`cargo test`)**
1. `git::worktrees`: `git worktree add` fixture, `rm -rf`한 prunable entry, detached HEAD.
2. `Registry`: 같은 레포를 가리키는 두 entry → 레포 1개, `base` 충돌 경고. 없는 경로 → 오류 목록, 나머지는 열림.
3. `profile`: marker별 판별, `ios/Pods` segment 매칭, `*.g.dart` 접기, `.gitattributes`의 `linguist-generated`.
4. base: stub gh script로 PR base 반영, `origin/<base>` 없으면 fallback, timeout, gh 없음.
5. watcher: `git worktree add` / `remove` / `rm -rf` 후 1초 안에 worktree 목록 갱신 알림.
6. `read_only.rs` 확장: worktree 여러 개 + `check-attr` 후에도 `.git/index`와 `.git/worktrees/*/index`가 그대로.

**UI (`pnpm --dir ui test`)**: 상대 시각, 접힌 파일 분리, 선택 fallback, event → `repos` invalidate.

**실제 UI 확인**: UI task마다 `pnpm tauri dev` + `screencapture`로 확인한다. scratchpad에 Python / Node / Flutter fixture 레포를 만들고 `WTBOARD_CONFIG`로 띄워 worktree를 추가·삭제해 본다. 마지막에 실제 `~/.wtboard/config.toml`(레포 5개)로 읽기만 하며 확인한다.
