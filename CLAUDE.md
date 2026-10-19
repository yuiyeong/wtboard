# wtboard

여러 git worktree의 변경 사항을 PR 스타일 diff로 실시간 보여 주는 Mac 로컬 앱 (Tauri 2: Rust core + React UI).

- 설계: `docs/draft.md`
- **진행 상황과 다음 단계: `docs/progress.md`** — 새 세션은 여기서 시작한다. 단계를 끝내면 이 파일을 갱신한다.
- 단계별 plan: `docs/superpowers/plans/`

## 명령

```bash
source ~/.cargo/env          # rustup 설치. 새 셸이 아니면 PATH에 cargo가 없을 수 있다
pnpm tauri dev               # 앱 실행 (ui dev server + Rust 빌드)
cargo test                   # Rust 테스트 (실제 git, 실제 FSEvents 사용)
cargo test -p wtboard-hook   # hook binary 테스트
pnpm --dir ui test           # UI 테스트 (vitest)
pnpm --dir ui build          # 타입 체크 + 번들
scripts/fixtures.sh <dir>    # UI 확인용 fixture 레포 3개 + config.toml
WTBOARD_CONFIG=<dir>/config.toml pnpm tauri dev   # fixture로 실행
scripts/install-hook.sh      # wtboard-hook release 빌드 → ~/.wtboard/bin/
```

plugin 설치는 사용자가 직접 한다: `claude plugin marketplace add <repo>/plugin` 후 `claude plugin install wtboard@wtboard-local`.

설정: `~/.wtboard/config.toml` (`[[repo]] path = ...`). `WTBOARD_CONFIG`로 경로를 바꿀 수 있다.

## 구조

- `crates/core`: git CLI 읽기 래퍼(`git/`), 변경 목록·diff·hash, 감시(`watch/`), worktree 상태(`worktree.rs`), 레포 하나(`repo.rs`), config 전체(`registry.rs`), profile(`profile.rs`), PR base(`gh.rs`), 설정(`store/config`)과 봤음 상태(`store/viewed`, SQLite, config.toml 옆 `state.sqlite`)
- `core::session`: Claude Code session 기록(`format`: 디스크 형식, `list`: session 목록·active 판정, `view`: 이번 세션·마지막 응답 diff)
- `crates/hook`: `wtboard-hook` binary. Claude Code hook이 호출해 session 기록을 쓴다 (레포는 읽기만)
- `plugin/wtboard`: hook 설정 plugin (`plugin/.claude-plugin/marketplace.json`)
- `crates/app`: Tauri command(`list_repos`, `list_files`, `get_patch`, `set_viewed`, `list_sessions`)와 `worktree-changed`, `worktrees-updated`, `session-updated` event
- `ui/`: React + TanStack Query + react-diff-view

## 지켜야 할 것

- 앱은 레포에 아무것도 쓰지 않는다. 모든 git 호출은 `Git` 래퍼를 거친다 (`GIT_OPTIONAL_LOCKS=0`, `diff.autoRefreshIndex=false` 등). `tests/read_only.rs`가 `.git/index` 불변을 검증한다.
- 변경 판단은 `git status`가 아니라 파일별 diff hash.
- 파일 diff는 화면에 보일 때만 계산한다.
- 테스트를 먼저 쓴다 (실패 확인 → 구현 → 통과).
