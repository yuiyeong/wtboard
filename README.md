# wtboard

여러 git worktree의 변경 사항을 GitHub PR 스타일 split diff로 실시간 보여 주는 Mac 로컬 앱.
여러 worktree에서 Claude Code를 동시에 돌리면서 각 worktree가 무엇을 바꿨는지 한 화면에서 확인하고,
diff 줄에 단 리뷰 코멘트를 그 worktree의 Claude Code 다음 prompt에 자동으로 첨부한다.

- 서버 없음. 앱 프로세스가 직접 `git`을 실행하고 파일을 감시한다.
- 앱은 레포에 아무것도 쓰지 않는다. Claude Code의 git 작업과 충돌하지 않는다.
- Claude Code plugin(hooks)이 세션별 변경 기록을 남겨 "이번 세션" / "마지막 응답" 뷰를 만든다.

설계 전체는 `docs/draft.md`, 진행 상황은 `docs/progress.md`에 있다.

## 기술 스택

| 영역 | 선택 | 비고 |
|---|---|---|
| 앱 shell | [Tauri 2](https://tauri.app) | Rust core + webview UI. 상시 띄워 두는 앱이라 가볍게 |
| core (`crates/core`) | Rust 2021 | git CLI 래퍼, diff hash(`blake3`), 파일 감시(`notify` + FSEvents), SQLite(`rusqlite` bundled), config(`toml`) |
| 앱 (`crates/app`) | Tauri command / event, tray(메뉴바) | `tray-icon` feature |
| hook (`crates/hook`) | `wtboard-hook` binary | Claude Code hook이 호출. 세션 기록을 `~/.wtboard/sessions/`에 쓴다 |
| UI (`ui/`) | React 19, TypeScript, Vite, Tailwind CSS 4 | |
| UI 상태·데이터 | TanStack Query 5, zustand | |
| diff 렌더링 | react-diff-view, refractor(syntax highlight) | |
| 테스트 | `cargo test`(실제 git·FSEvents 사용), vitest | |
| 패키지 관리 | pnpm workspace(`ui`), Cargo workspace(`crates/*`) | |
| 외부 CLI | `git`(필수), `gh`(선택: PR base 자동 판별) | |

로컬 저장소는 모두 `~/.wtboard/` 아래에 있다.

```
~/.wtboard/
├─ config.toml               # 감시할 레포 목록과 설정
├─ state.sqlite              # 봤음 상태, 리뷰 코멘트, 앱 설정
├─ bin/wtboard-hook          # scripts/install-hook.sh가 설치하는 hook binary
└─ sessions/<session_id>/    # Claude Code 세션별 변경 기록 (hook이 쓴다)
```

## 환경 설정

### 요구 사항

- macOS (FSEvents, 메뉴바 tray를 쓴다)
- Xcode Command Line Tools (`xcode-select --install`)
- Rust toolchain ([rustup](https://rustup.rs)). 개발은 stable 1.99에서 했다
- Node.js + pnpm (`corepack enable` 또는 `npm i -g pnpm`). 개발은 Node 24, pnpm 12에서 했다
- `git`
- `gh` (선택). 설치되어 있고 로그인돼 있으면 현재 브랜치 PR의 base 브랜치를 자동으로 쓴다
- Claude Code (세션 추적·코멘트 자동 첨부를 쓰려면)

### 의존성 설치

```bash
git clone <this repo> wtboard && cd wtboard
source ~/.cargo/env        # 새 셸이 아니면 PATH에 cargo가 없을 수 있다
pnpm install               # root(@tauri-apps/cli)와 ui/ 둘 다 설치된다
```

Rust crate는 첫 `pnpm tauri dev` / `cargo build`에서 받는다.

### `~/.wtboard/config.toml` 작성

감시할 레포를 `[[repo]]`로 하나씩 등록한다. 레포 하나를 등록하면 그 레포의 모든 worktree를 자동으로 찾는다
(`git worktree list` 기준이라 worktree를 어디에 만들었든 잡힌다). 등록한 경로가 worktree여도 된다.

```toml
[[repo]]
path = "~/dev/my-api"
base = "origin/main"           # 생략하면 gh PR base → origin/HEAD 순으로 정한다

[[repo]]
path = "~/dev/my-web"

[watch]
fallback_poll_secs = 30        # FS event를 놓쳤을 때를 위한 backup poll 간격 (최소 5)

[sessions]
keep_days = 7                  # 이보다 오래된 세션 기록은 앱 시작 때 지운다

[review]
auto_attach_comments = true    # false면 hook이 리뷰 코멘트를 prompt에 붙이지 않는다
```

`WTBOARD_CONFIG=<path>` 환경 변수로 config 파일 위치를 바꿀 수 있다. `state.sqlite`와 `sessions/`는 그 config 파일 옆에 만들어진다.

프로젝트 종류(profile)는 worktree root 파일로 자동 판별한다. profile은 감시에서 뺄 폴더와 "접힌 파일"(생성·lock 파일)을 정한다.

| profile | 판별 | 접히는 파일 예 |
|---|---|---|
| Flutter | `pubspec.yaml` | `*.g.dart`, `*.freezed.dart`, `pubspec.lock`, `Podfile.lock` |
| Node | `package.json` | `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `next-env.d.ts` |
| Python | `pyproject.toml` 또는 `requirements.txt` | `uv.lock`, `poetry.lock` |
| Default | 그 외 | (없음) |

`.gitattributes`의 `linguist-generated` 파일도 접힌다. `node_modules`, `.venv`, `.next`, `.dart_tool` 같은 폴더는 모든 profile에서 감시하지 않는다.

### Claude Code plugin 설치 (선택)

"이번 세션" / "마지막 응답" 탭과 리뷰 코멘트 자동 첨부에 필요하다. 두 단계다.

1. hook binary를 release로 빌드해 `~/.wtboard/bin/`에 넣는다.

   ```bash
   scripts/install-hook.sh
   ```

2. 스크립트가 안내하는 두 명령을 직접 실행한다 (스크립트는 Claude 설정을 건드리지 않는다).

   ```bash
   claude plugin marketplace add /path/to/wtboard/plugin
   claude plugin install wtboard@wtboard-local
   ```

plugin의 hook은 `~/.wtboard/bin/wtboard-hook`이 없으면 아무것도 하지 않는다. hook을 고친 뒤에는 `scripts/install-hook.sh`를 다시 실행한다.
앱은 시작할 때 설치된 binary의 schema version을 확인하고, 앱과 다르면 sidebar에 경고를 띄운다.

## 실행하는 법

```bash
source ~/.cargo/env
pnpm tauri dev               # ui dev server(vite, :5173) + Rust 빌드 + 앱 창
```

처음 빌드는 Rust crate 컴파일 때문에 몇 분 걸린다. 이후에는 UI는 HMR, Rust는 바뀐 crate만 다시 빌드한다.

### fixture로 실행 (실제 레포 없이 화면 확인)

```bash
scripts/fixtures.sh /tmp/wtboard-fixtures          # Python / Node / Flutter fixture 레포 3개 + config.toml
WTBOARD_CONFIG=/tmp/wtboard-fixtures/config.toml pnpm tauri dev
```

fixture에는 일부러 없는 레포 하나가 들어 있어 sidebar의 config 오류 표시도 함께 볼 수 있다.

### 설치 없이 debug hook으로 세션 추적·코멘트 첨부 확인

```bash
W=$PWD          # wtboard repo root
cd /tmp/wtboard-fixtures/py-feat
WTBOARD_CONFIG=/tmp/wtboard-fixtures/config.toml \
WTBOARD_HOOK_BIN=$W/target/debug/wtboard-hook \
claude --plugin-dir $W/plugin/wtboard -p "리뷰 코멘트 반영해 줘" --permission-mode acceptEdits
```

`WTBOARD_HOOK_BIN`은 plugin의 `run.sh`와 앱 양쪽이 읽는다. `cargo build -p wtboard-hook`으로 debug binary를 먼저 만들어 둔다.

### 테스트·빌드

```bash
cargo test                   # Rust 전체 (실제 git, 실제 FSEvents 사용)
cargo test -p wtboard-hook   # hook binary만
pnpm --dir ui test           # UI (vitest)
pnpm --dir ui build          # 타입 체크 + 번들
pnpm test                    # UI 테스트 + cargo test
pnpm tauri build             # 배포용 .app / .dmg → target/release/bundle/
```

`crates/core/tests/read_only.rs`가 앱이 레포의 `.git/index`를 건드리지 않는지 검증한다.

### 환경 변수

| 변수 | 뜻 |
|---|---|
| `WTBOARD_CONFIG` | config.toml 경로 (기본 `~/.wtboard/config.toml`). 데이터 폴더도 이 파일 옆이다 |
| `WTBOARD_HOOK_BIN` | hook binary 경로 (기본 `~/.wtboard/bin/wtboard-hook`) |
| `WTBOARD_SLOW_LIST_MS` | 변경 목록 계산이 이 시간(ms)을 넘으면 fsmonitor 안내 banner (기본 500) |

## 사용하는 법

### 화면

```
┌──────────────────┬───────────────────┬──────────────────────────────────────┐
│ 레포 / worktree   │ 파일 트리          │ [브랜치 전체] [이번 세션] [마지막 응답 #n]│
│                  │                   │ [split | unified]   확인 12/18   💬    │
│ ▾ my-api  node   │ ▾ app/api         │──────────────────────────────────────│
│   ● feat-auth  7 │   ☐ users.py  +40 │  users.py                    ☐ 봤음   │
│     fix-cors   2 │   ☑ deps.py   +3  │  - old line    │ + new line           │
│ ▾ my-web  python │ ▾ tests           │       [+]      │ + new line           │
│   ● ui-login  12 │   ☐ test_u.py +55 │                                      │
│                  │ ▸ 접힌 파일 (2)    │                                      │
└──────────────────┴───────────────────┴──────────────────────────────────────┘
```

- **sidebar**: 레포(profile badge)별 worktree 목록. 브랜치 이름, 미확인 파일 수, 마지막 변경 시각, 진행 중 Claude Code 세션(●), 미전송 코멘트(💬 n), config 오류. 선택은 앱을 다시 띄워도 남는다.
- **파일 트리**: 디렉터리 트리. 생성·lock 파일은 맨 아래 "접힌 파일 (n)" 폴더에 접혀 있다. 체크한 파일은 ☑, 체크 뒤 다시 바뀐 파일은 ● 표시.
- **diff**: 보이는 파일만 계산한다. header 클릭으로 파일을 접고 펼친다. split / unified 전환.
- base가 PR에서 온 것이면 header에 `(PR)`이 붙는다.

### 봤음 체크

파일 header의 "봤음"을 체크하면 그 시점의 diff hash를 저장하고 파일이 접힌다. 이후 그 파일의 diff가 바뀌면 체크가 자동으로 풀리고 "다시 바뀜"으로 돌아온다. header의 `확인 n/m`과 sidebar의 미확인 수, 메뉴바 아이콘 숫자가 같이 움직인다.

### 세 가지 뷰

| 탭 | 보여 주는 것 | 기준 |
|---|---|---|
| 브랜치 전체 | PR 뷰. 브랜치가 base 대비 바꾼 전부 | merge-base(base는 config → `gh pr view` → `origin/HEAD`) |
| 이번 세션 | 고른 Claude Code 세션이 지금까지 바꾼 파일 | 세션에서 처음 건드리기 직전 원본(baseline) ↔ 현재 파일. git과 무관해 세션 중 commit해도 유지된다 |
| 마지막 응답 #n | 그 세션의 마지막 turn(응답)이 바꾼 파일 | 그 파일의 이전 turn 스냅샷(없으면 baseline) ↔ turn 끝 스냅샷(응답 진행 중이면 현재 파일) |

세션 탭 두 개는 plugin이 설치돼 있고 그 worktree에서 Claude Code를 돌린 기록이 있을 때만 켜진다. 세션이 여러 개면 탭 옆 select로 고른다.
Claude가 `Edit`/`Write`로 바꾼 파일은 원본을 정확히 안다. `Bash`(`sed`, 코드 생성기 등)로 바뀐 파일은 hook 전후 비교로 잡는데, Bash 전에 이미 수정돼 있어 원본을 모르는 파일은 "원본 불확실"로 표시한다 (세션 시작 commit 또는 빈 파일과 비교).

### 리뷰 코멘트 → Claude Code

1. diff의 줄 gutter에 있는 `+`를 눌러 코멘트를 쓴다. ⌘Enter 또는 `저장`으로 저장, Esc로 취소. 코멘트는 "미전송" 상태로 `state.sqlite`에 저장된다.
2. 그 worktree 안에서 Claude Code에 다음 prompt를 보내면 `UserPromptSubmit` hook이 미전송 코멘트를 `파일:줄 + 코드 일부 + 코멘트` 형태의 추가 맥락으로 붙이고 "전송됨"으로 바꾼다. hook은 `wtboard: 리뷰 코멘트 n개를 첨부했습니다`를 systemMessage로 알린다.
3. 응답이 와서 그 파일이 바뀌면 코멘트에 `응답 #n 보기`가 생긴다. 그 turn이 세션의 마지막이면 "마지막 응답 #n" 뷰로, 아니면 "이번 세션" 뷰로 가서 그 파일로 스크롤한다. 코멘트가 가리킨 줄이 바뀌면 "위치가 바뀐 코멘트" 묶음에 원래 줄 내용과 함께 남는다.
4. 코멘트마다 `수정` / `삭제` / `다시 보내기`(미전송으로 되돌려 다음 prompt에 다시 첨부) / `해결됨`(diff에서 숨김, popover의 "해결된 코멘트"에서 `되돌리기`)을 할 수 있다.

header의 💬 버튼이 worktree의 코멘트 목록 popover를 연다. 여기서 코멘트를 눌러 파일로 가거나, `마크다운으로 복사`로 미전송 코멘트를 clipboard에 복사해 직접 붙여 넣을 수 있다.
자동 첨부를 원하지 않으면 `config.toml`의 `[review] auto_attach_comments = false`로 끈다. 그러면 복사해서 붙이는 방식만 남는다.

### 메뉴바(tray)와 창

- 메뉴바 아이콘에 전체 미확인 파일 수가 보인다. 메뉴에서 worktree를 바로 고를 수 있다 (진행 중 세션은 ●).
- 창을 닫으면 종료가 아니라 숨김이다. Dock 아이콘 클릭이나 메뉴바의 `wtboard 열기`로 다시 연다. 종료는 메뉴바의 `종료`.
- `항상 위에 띄우기`를 켜면 창이 다른 창 위에 머문다. 설정은 저장된다.

### 느린 레포

큰 레포에서 변경 목록 계산이 500ms를 넘고 git fsmonitor가 꺼져 있으면 header 아래에 안내 banner가 뜬다. 앱은 레포에 쓰지 않으므로 레포 폴더에서 직접 실행한다.

```bash
git config core.fsmonitor true
git config core.untrackedCache true
```

## 문서

- `docs/draft.md`: 설계. 결정 사항과 이유, 저장소 형식, 핵심 동작, 화면 구성
- `docs/progress.md`: 단계별 진행 상황, 정한 것, 알려진 한계, 다음 단계 후보
- `docs/superpowers/specs/`, `docs/superpowers/plans/`: 단계별 spec과 구현 plan
- `CLAUDE.md`: 개발 명령과 crate 구조, 지켜야 할 규칙
