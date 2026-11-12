# 7단계 설계: 설정창 (theme · 레포 관리) + 창 크기 기억

상위 설계: `docs/draft.md`. 이 문서는 6단계까지의 앱 위에 더하는 것만 적는다.

사용자와 대화로 정한 것은 그대로 적고, 설계자가 정한 것은 "(내 판단)"으로 표시한다.

## 왜

- 레포를 등록하려면 `~/.wtboard/config.toml`을 찾아가 편집기로 열고, 레포 경로를 손으로 쳐 넣고, `base`에 무엇을 적어야 하는지 스스로 알아내야 한다. 사용자는 이 세 가지가 모두 번거롭다고 했다.
- 화면은 2주 전(`74a406c`)부터 항상 dark다. 사용자는 원할 때 light로 바꾸고, 그 선택이 다음 실행에도 남기를 바란다. 바꾸는 이유는 따로 없다. 그때그때 바꾸고 싶을 때 바꾼다.
- 창은 켤 때마다 1400×900으로 뜬다. 사용자는 외부 모니터에서는 크게, MacBook만 쓸 때는 그 화면에 맞춰 쓰므로 켤 때마다 창을 다시 맞춘다.

## 목표와 완료 기준

1. **설정창**: `⌘,` 또는 app menu `wtboard > Settings…`로 여는 별도 창. 안에 theme 스위치와 레포 목록이 있다.
2. **Theme**: Dark / Light 스위치. 바꾸면 열려 있는 모든 창에 바로 적용되고 다음 실행에도 유지된다.
3. **레포 관리**: 설정창에서 레포를 추가·삭제하고 base를 바꾼다. 경로를 타이핑하지 않는다. 앱을 다시 켜지 않아도 sidebar에 반영된다.
4. **창 크기 기억**: main 창의 마지막 크기와 위치를 기억해 다음 실행 때 그대로 띄운다. 지금 화면보다 크면 화면 크기까지만 줄인다. 설정 항목은 없다 (자동).

완료 기준 (실제 앱에서 확인):

- `⌘,`와 메뉴 양쪽으로 설정창이 열린다. 이미 열려 있으면 새로 만들지 않고 앞으로 가져온다.
- 설정창에서 Light를 고르면 main 창(배경, diff, syntax 색, title bar)과 설정창이 바로 light가 된다. 앱을 껐다 켜면 처음부터 light로 뜬다 (dark가 먼저 보였다가 바뀌지 않는다).
- `config.toml`이 없는 상태로 켠다 → main 창이 "등록된 레포가 없습니다"와 설정 열기 버튼을 보인다 → 설정창에서 폴더를 골라 추가한다 → `config.toml`이 생기고 sidebar에 그 레포가 나타난다.
- 주석이 있는 `config.toml`에서 레포를 추가·삭제·base 변경해도 주석과 `[watch]`·`[sessions]`·`[review]` 값이 그대로 남는다.
- base를 `자동`에서 `origin/main`으로 바꾸면 그 레포 worktree의 header base가 바뀌고 diff가 다시 계산된다. 다른 레포는 다시 열리지 않는다.
- 외부 모니터에서 창을 크게 키우고 종료 → 그 모니터 없이 켜면 창이 MacBook 화면 안에 다 들어온다 → 창을 건드리지 않고 종료 → 다시 외부 모니터에서 켜면 원래의 큰 크기로 뜬다.

범위 밖 (내 판단):

- macOS system appearance를 따라가는 theme 옵션. Dark / Light 둘뿐이다.
- 창 크기를 숫자로 정하는 설정 (사용자가 필요 없다고 했다).
- `[watch] fallback_poll_secs`, `[sessions] keep_days`, `[review] auto_attach_comments`, "항상 위에"를 설정창에 넣는 것. 지금 자리(`config.toml`, tray 메뉴)에 둔다.
- 앱이 켜져 있는 동안 `config.toml`을 바깥에서 고친 것을 감시해 반영하는 것. 지금처럼 재시작해야 한다. 단, 설정창의 동작은 매번 파일을 새로 읽으므로 바깥에서 고친 내용을 덮어쓰지 않는다 (3.4절).
- 레포 목록 순서 바꾸기.
- 설정창의 크기·위치 기억, fullscreen·maximize 상태 기억.

하지 말아야 할 것:

- 레포에 쓰지 않는다는 기존 규칙은 그대로다. 이번에 앱이 새로 쓰게 되는 파일은 `config.toml` 하나뿐이고, 설정창에서 사용자가 레포를 추가·삭제·base 변경할 때만 쓴다.
- 읽을 수 없는(parse 실패) `config.toml`을 덮어쓰지 않는다.
- 레포 삭제가 레포 폴더나 그 레포의 코멘트·봤음 기록을 지우지 않는다.

## 1. 구성

| 단위 | 하는 일 |
|---|---|
| `core::store::config` (수정) | `config.toml`을 주석·형식을 유지한 채 고치는 `add_repo`·`remove_repo`·`set_repo_base`. 파일이 없으면 만든다 |
| `core::store::settings` (수정) | key `theme`, `window_main` 추가. schema는 그대로다 (`settings` table은 key-value) |
| `core::window_fit` (새 module, 이름은 builder가 정한다) | 저장된 창 rect와 지금 모니터 목록으로 띄울 rect를 정하는 순수 함수 |
| `core::registry` (수정) | 열린 뒤에도 config에 맞춰 레포를 더하고 빼는 `apply`. 레포 0개도 정상 상태 |
| `core::git` (수정) | base 후보 branch 목록, 고른 폴더의 worktree root 읽기 (`Git` 래퍼로, 읽기만) |
| `crates/app` (수정) | main 창을 `setup`에서 직접 만든다 (저장된 theme·크기 적용). app menu에 `Settings…`. 설정창. 새 command. 창 크기 저장 |
| `ui/` (수정) | `data-theme`에 따른 light/dark 색. 설정창 화면. 레포 0개 화면 |

## 2. 설정창

- 별도 창이다 (label `settings`, title `Settings`). 사용자가 mockup 두 개(별도 창 / main 창 안 overlay) 중 별도 창을 골랐다.
- 여는 길: app menu `wtboard > Settings…` (accelerator `⌘,`), tray 메뉴의 `설정…` (내 판단). 지금은 Tauri 기본 menu를 쓰므로 app menu를 직접 구성해야 한다. 기본 menu의 다른 항목은 그대로 둔다. 특히 Edit menu가 빠지면 webview에서 `⌘C`/`⌘V`가 동작하지 않는다.
- 하나만 뜬다. 열려 있을 때 다시 열면 앞으로 가져온다. main 창이 숨겨져 있어도 설정창만 뜬다 (main 창을 같이 보이지 않는다).
- 닫기(`⌘W`, 빨간 버튼)는 창을 없애거나 숨긴다. 어느 쪽인지는 builder가 정한다. main 창의 "닫기 = 숨기기"는 그대로다.
- 크기는 고정이다 (약 560×520, 내 판단). 레포 목록이 길면 목록 안에서 스크롤한다.
- registry가 아직 열리는 중이어도(앱 시작 직후 수 초) 설정창은 바로 뜨고 theme 스위치가 동작해야 한다. 레포 목록은 `config.toml`을 직접 읽어 보인다.
- `capabilities/default.json`의 `windows`에 `settings`를 더한다.

배치 (위에서 아래로):

```
Theme        [ Dark | Light ]

Repositories
┌──────────────────────────────────────────────────────────────┐
│ omofictions-back                 base: 자동 (origin/develop) ▾  [삭제] │
│ ~/projects/omofictions-back                                    │
├──────────────────────────────────────────────────────────────┤
│ omofictions-front                base: origin/main ▾           [삭제] │
│ ~/projects/omofictions-front                                   │
└──────────────────────────────────────────────────────────────┘
[＋ Repository 추가…]
```

mockup: `.superpowers/brainstorm/59739-1794445588/content/settings-layout.html` (git에는 없다. 위 그림이 같은 내용이다).

## 3. 레포 관리

### 3.1 목록

- 한 줄 = `config.toml`의 `[[repo]]` entry 하나. 이름(폴더 이름), config에 적힌 경로, base 선택, 삭제 버튼.
- 열지 못한 entry(경로 없음, git 레포 아님)도 줄로 보이고 그 이유를 같이 보인다. 삭제할 수 있어야 하기 때문이다. base 선택은 끈다.
- `config.toml`이 없으면 빈 목록이다. parse에 실패하면 오류 문구를 보이고 추가·삭제·base 변경을 모두 끈다 (theme 스위치는 동작한다).

### 3.2 추가

1. `＋ Repository 추가…` → macOS 폴더 선택 dialog (`tauri-plugin-dialog` 추가, 내 판단).
2. 고른 폴더를 확인한다:
   - git 레포가 아니면 "git repository가 아닙니다"를 설정창 안에 보이고 아무것도 쓰지 않는다.
   - 레포의 하위 폴더를 골랐으면 그 worktree의 root(`git rev-parse --show-toplevel`)를 등록한다 (내 판단).
   - 이미 등록된 레포(common dir가 같은 entry가 있음. 같은 레포의 다른 worktree를 고른 경우 포함)면 "이미 등록된 repository입니다"를 보이고 쓰지 않는다. 레포 하나를 등록하면 모든 worktree를 찾으므로 두 번 등록할 이유가 없다.
3. `config.toml` 끝에 `[[repo]]`를 더한다. `path`는 home 아래면 `~/…`로 줄여 쓴다 (기존 파일의 표기와 맞춘다, 내 판단). `base`는 쓰지 않는다 (= 자동).
4. registry에 반영한다 (3.5절).

추가할 때 base를 묻지 않는다. 사용자가 "base를 어떻게 설정해야 하는지 모르겠다"고 했고, 비워 두면 앱이 알아서 정하기 때문이다. 바꾸고 싶으면 추가한 뒤 그 줄에서 바꾼다.

### 3.3 base 변경

- 줄마다 base 선택(`<select>` 또는 같은 역할의 control)이 있다. 선택지:
  - `자동`: `base`를 쓰지 않는다. 지금의 결정 순서 그대로다 (worktree의 현재 branch에 PR이 있으면 PR base, 없으면 `origin/HEAD`).
  - 그 레포의 remote branch (`origin/main`, `origin/develop`, …. `origin/HEAD`는 뺀다), 그 다음 local branch (내 판단: 둘 다 보이고 remote를 먼저).
- `자동`일 때는 실제로 무엇이 쓰이는지 옆에 보인다: `origin/HEAD`가 가리키는 branch를 괄호로 적는다 (`자동 (origin/develop)`). PR이 있는 worktree에서는 PR base가 우선한다는 것을 tooltip이나 한 줄 설명으로 알린다 (내 판단). `origin/HEAD`가 없는 레포(remote 없음 등)는 `자동 (정할 수 없음)`으로 보여, 직접 골라야 한다는 것이 드러나게 한다.
- config에 이미 적힌 base가 후보 목록에 없으면(지워진 branch, tag, commit hash) 그 값을 선택지에 그대로 더해 보인다. 조용히 다른 값으로 바꾸지 않는다.
- 고르면 바로 저장한다 (저장 버튼 없음, 내 판단). `자동`이면 `base` key를 지우고, 아니면 `base = "<이름>"`을 쓴다.

### 3.4 삭제와 `config.toml` 쓰기

- `삭제`를 누르면 그 줄 안에서 한 번 확인한다 (`삭제할까요? [취소] [삭제]`, native dialog가 아니라 줄 안에서, 내 판단). 확인하면 그 `[[repo]]` entry만 지운다.
- 레포 폴더, 그 레포 worktree의 코멘트·봤음 기록(`state.sqlite`)은 건드리지 않는다. 다시 등록하면 그대로 돌아온다 (내 판단).
- 그 레포 worktree에 미전송 코멘트가 있으면 확인 줄에 `미전송 코멘트 n개가 남습니다`를 같이 보인다 (내 판단). hook은 레포 등록 여부를 보지 않고 cwd의 worktree root로 미전송 코멘트를 찾으므로(`crates/hook/src/comments.rs`), 삭제한 뒤에도 그 worktree에서 보낸 prompt에 그 코멘트가 붙는다. 이 동작은 바꾸지 않는다 (`crates/hook`은 이번 범위 밖). 알려진 한계로 `docs/progress.md`에 적는다.
- 쓰기 규칙 (추가·삭제·base 변경 공통):
  - 동작할 때마다 파일을 새로 읽고, 그 entry만 고쳐서 쓴다. 메모리에 들고 있던 옛 내용을 통째로 쓰지 않는다. 그래야 앱이 켜진 동안 사용자가 손으로 고친 내용이 남는다.
  - 주석, 빈 줄, key 순서, 다른 table을 유지한다 (`toml_edit` 추가, 내 판단). 지금 사용자 파일 첫 줄에 주석이 있다.
  - entry는 config에 적힌 `path` 문자열로 가린다. 같은 `path`가 둘이면 첫 번째를 고친다.
  - 같은 폴더의 임시 파일에 쓴 뒤 rename한다.
  - 파일이나 `~/.wtboard/`가 없으면 만든다. `WTBOARD_CONFIG`가 가리키는 경로를 그대로 따른다.
  - parse에 실패하는 파일에는 쓰지 않고 오류를 돌려준다.

### 3.5 재시작 없이 반영

- 지금 `Registry`는 `OnceLock`에 한 번 열리고 바뀌지 않는다. 이것을 config에 맞춰 바꿀 수 있게 한다: 새 config와 열려 있는 레포를 비교해, 그대로인 레포(common dir와 base가 같음)는 두고, 새 레포만 열고, 없어진 레포는 닫는다(감시 중단). base가 바뀐 레포는 닫고 새 base로 다시 연다 (내 판단: base만 갈아 끼우는 길을 따로 만들지 않는다).
- 그대로인 레포를 다시 열지 않는 것이 요구 사항이다. 레포 6개를 모두 다시 열면 수 초 걸리고 화면이 비기 때문이다. 자료 구조(`RwLock`, `Mutex` 등)는 builder가 정한다.
- 반영이 끝나면 기존 `worktrees-updated` event를 보낸다. UI는 이미 이 event에 `['repos']`를 다시 읽는다. tray도 갱신한다.
- `worktrees-updated`만으로는 모자란다. 이 event는 `['repos']`만 invalidate하고(`ui/src/useWorktreeEvents.ts`), `['files', id]`·`['patch', id]`는 `worktree-changed`가 와야 다시 읽힌다 (`ui/src/lib/events.ts`, `staleTime: Infinity`). 레포를 닫고 다시 열어도 worktree id(경로)가 같아 query key가 그대로라서, base를 바꿨거나 삭제 뒤 다시 추가한 레포는 header의 base만 바뀌고 파일 목록과 diff가 옛 base로 남는다. 그래서 다시 연 레포의 worktree마다 `worktree-changed`를 내(또는 같은 효과의 다른 방법으로) 파일 목록과 diff가 다시 읽히게 한다.
- 레포 0개는 오류가 아니다. 지금 `open_registry`는 config가 없거나 레포가 0개면 `Err`를 돌려주고 화면이 "wtboard를 시작할 수 없습니다"를 띄운다. 바꾼 뒤에는:
  - `config.toml`이 없음 / 레포 0개 → 빈 registry. main 창은 "등록된 레포가 없습니다"와 `Settings 열기` 버튼(`⌘,` 안내 포함)을 보인다.
  - `config.toml` parse 실패 → 지금처럼 오류 화면. 문구에 설정 파일 경로를 남긴다 (손으로 고쳐야 한다).
- 지워진 레포의 worktree가 선택돼 있었으면 UI는 기존 `resolveSelection`의 규칙대로 다른 worktree로 넘어간다. 새로 만들 것이 있는지는 builder가 확인한다.
- 설정창의 동작이 registry가 아직 열리는 중에 오면 열릴 때까지 기다렸다가 반영한다.

## 4. Theme

- 값은 `dark` | `light`. `state.sqlite`의 `settings` table에 key `theme`으로 둔다 (`always_on_top`과 같은 자리, 내 판단). 값이 없거나 모르는 값이면 `dark` (지금 화면과 같다).
- 적용되는 곳: main 창과 설정창의 webview 색, 창 title bar(native window theme), 창이 뜰 때의 배경색(`backgroundColor`).
- 바꾸는 순간: 설정창에서 스위치를 누르면 저장하고, 모든 창에 event로 알려 바로 바꾼다. 재시작이 필요 없다.
- 시작할 때: 저장된 theme을 창을 만들기 **전에** 읽어, 창을 그 theme·배경색으로 만들고 webview가 처음 그리기 전에 `<html data-theme>`가 정해져 있게 한다 (예: `initialization_script`). light로 저장한 사용자가 켤 때 dark가 먼저 보였다가 바뀌면 안 된다. 그래서:
  - main 창을 `tauri.conf.json`의 `windows`가 아니라 `setup`에서 직접 만든다 (5절의 크기 복원과 같은 이유).
  - `SettingsStore`를 지금처럼 registry와 함께 background에서 여는 것이 아니라 `setup`에서 먼저 연다. 열 수 없으면 기본값(dark, 1400×900)으로 띄우고 기존 경고(sidebar의 "설정을 저장할 수 없음")를 남긴다. 이렇게 하면 "레포가 없으면 settings가 열리지 않아 항상 위에 저장이 빠진다"는 5단계 알려진 한계(`docs/progress.md`)도 같이 없어진다.
- 색 (내 판단):
  - light는 `74a406c` 직전(`39f989c`)의 화면을 되살린다. 그 commit이 바꾼 class·CSS가 그대로 light↔dark 대응표다 (`git show 74a406c`).
  - 구현은 Tailwind 4의 `dark:` variant를 `data-theme="dark"`에 묶고(`@custom-variant`), component는 `<light class> dark:<dark class>`로 쓴다. `index.css`의 diff 색 변수·syntax token 색·`color-scheme`·body 색은 `data-theme`별로 나눈다. `74a406c` 뒤에 색이 더 들어간 곳이 있으면(`7b5f434` 등) 같은 방식으로 맞춘다.
  - 설정창의 light 모습은 새로 정해야 한다. main 창의 light 색과 같은 계열로 builder가 정하고 screenshot으로 확인한다.
- tray 아이콘은 template image라 theme과 무관하다.

## 5. 창 크기 기억

- 대상은 main 창뿐이다.
- 저장: 사용자가 창을 움직이거나 크기를 바꾸면 (모아서, 예: 마지막 변화 뒤 0.5초) 위치와 크기를 `settings`의 key `window_main`에 쓴다. 값의 형식(JSON 등)은 builder가 정한다. 단위는 logical point다 (모니터마다 scale factor가 달라도 같은 크기로 돌아오게).
- 복원 때 앱이 줄이거나 옮긴 결과는 저장하지 않는다 (내 판단). 사용자가 그 뒤에 창을 직접 움직이거나 크기를 바꿔야 저장 값이 바뀐다. 외부 모니터에서 크게 쓰다가 MacBook만으로 켜서 창이 줄어든 채로 쓰고 끈 뒤, 다시 외부 모니터에서 켜면 원래의 큰 크기로 돌아와야 하기 때문이다. 그래서 "숨길 때·종료할 때 지금 rect를 무조건 쓴다"는 방식은 쓰지 않는다. 창을 만든 직후 앱이 적용한 rect와 같은 값의 event는 사용자 동작으로 치지 않는다.
- fullscreen 중의 크기는 저장하지 않는다 (내 판단). 다음 실행은 fullscreen 전의 크기로 뜬다.
- 복원: 창을 만들 때 저장 값과 지금 연결된 모니터로 띄울 rect를 정한다.
  1. 저장 값이 없으면 1400×900, 위치는 OS 기본 (지금과 같다). 이 크기도 3번의 줄이기를 거친다.
  2. 저장된 rect와 가장 많이 겹치는 모니터를 고른다. 겹치는 모니터가 없으면(그 모니터가 지금 없음) primary 모니터다 (내 판단).
  3. 크기가 그 모니터의 쓸 수 있는 영역(menu bar·Dock을 뺀 work area, 내 판단. Tauri 2.12의 `Monitor::work_area`를 builder가 확인하고, 없으면 모니터 전체 크기)보다 크면 그 영역 크기까지 줄인다. 가로·세로를 따로 본다. 사용자가 말한 "현재 화면보다 크면 화면 전체 크기까지만"이 이것이다.
  4. 창이 그 영역 밖으로 나가면 안으로 옮긴다. 크기는 3번에서 정한 것을 유지한다.
- 2~4번은 입력(저장된 rect, 모니터 목록)만으로 답이 정해지는 순수 함수로 `crates/core`에 두고 test한다. 모니터 정보를 읽고 창에 적용하는 것만 `crates/app`에 둔다.
- 최소 크기는 두지 않는다. 저장된 값이 터무니없이 작으면(예: 가로·세로 200 미만) 기본값으로 본다 (내 판단).
- `tauri-plugin-window-state`를 쓰지 않는다 (내 판단). 그 plugin은 상태를 app data 폴더에 따로 두어 `WTBOARD_CONFIG`(fixture 실행)를 따르지 않고, 줄이기 규칙(3번)을 우리가 정할 수 없다.

## 6. command와 event

이름은 builder가 바꿔도 된다. 필요한 동작만 적는다.

| command | 하는 일 |
|---|---|
| `get_settings` | `{ theme }`. 설정창이 처음 그릴 때 쓴다 |
| `set_theme(theme)` | 저장, 모든 창의 native theme·배경색 변경, `theme-changed` event |
| `list_repo_entries` | `config.toml`을 읽어 entry마다 `{ path, name, base, autoBase, error }`. `autoBase`는 `origin/HEAD`가 가리키는 branch (없으면 null) |
| `list_base_candidates(path)` | 그 entry 레포의 remote branch, local branch 목록 |
| `add_repo` | 폴더 dialog → 확인 → `config.toml` 쓰기 → registry 반영. 취소하면 아무 일도 없다 |
| `remove_repo(path)` | `config.toml` 쓰기 → registry 반영 |
| `set_repo_base(path, base \| null)` | `config.toml` 쓰기 → registry 반영 |
| `open_settings` | 설정창을 연다 (main 창의 빈 화면 버튼이 부른다) |

- event `theme-changed` `{ theme }`: 모든 창. `<html data-theme>`을 바꾼다.
- event `worktrees-updated`: 기존 것. 레포 추가·삭제·base 변경 뒤에도 보낸다.
- git을 부르는 command는 지금처럼 blocking pool에서 돈다. `Git` 래퍼만 쓴다.

## 7. test와 확인

테스트를 먼저 쓴다 (프로젝트 규칙).

- `core::store::config`: 주석·다른 table이 있는 파일에 add / remove / set base / unset base를 했을 때 결과 문자열. 파일 없음 → 생성. parse 실패 → 쓰지 않음. 같은 path 둘. home 아래 경로의 `~/` 표기.
- `core::window_fit`: 저장 값 없음, 한 모니터 안에 다 들어옴, 화면보다 큼(가로만 / 세로만 / 둘 다), 저장된 모니터가 없음, 일부만 화면 밖, 터무니없이 작은 값. 줄여서 띄운 뒤 사용자 동작 없이 끝나면 저장 값이 그대로인 것 (저장 여부를 정하는 부분도 순수 로직으로 뺀다).
- `core::registry`: `apply` 뒤 새 레포가 열리고, 지운 레포가 사라지고, 그대로인 레포는 같은 `Arc<Repo>`로 남는다. base가 바뀐 레포만 다시 열린다. 레포 0개 config가 오류가 아니다. 실제 git 레포로 test한다 (기존 test와 같은 방식).
- `core::git`: base 후보 목록 (remote 먼저, `origin/HEAD` 제외), 하위 폴더 → worktree root.
- `crates/core/tests/read_only.rs`: 새로 더한 git 호출(branch 목록, toplevel)도 `.git/index`를 바꾸지 않는다.
- `ui/lib`: theme 값 해석 같은 순수 로직이 생기면 vitest.
- 실제 앱 screenshot (fixture: `scripts/fixtures.sh`, `WTBOARD_CONFIG`): 설정창 dark/light, main 창 light(diff·syntax·코멘트 widget·popover 포함), 레포 0개 화면, 추가·삭제·base 변경 뒤 sidebar, light로 저장한 뒤 재시작한 첫 화면, 큰 창으로 종료 뒤 작은 화면에서 재시작. 모니터를 뽑는 확인은 자동화하기 어려우므로 `window_main`에 화면보다 큰 값을 직접 넣고 켜는 것으로 대신한다.

## 8. 만드는 순서

1. 창 크기 기억. 이때 main 창을 `setup`에서 만들고 `SettingsStore`를 먼저 여는 구조로 바꾼다 (2·3번이 이 위에 선다).
2. 설정창과 theme (app menu, `⌘,`, light/dark).
3. 레포 관리 (config 쓰기, registry `apply`, 레포 0개 화면).

단계마다 실제 앱으로 확인하고 commit한다.

## 9. 문서

- `README.md`: 설정창, theme, 레포 등록 방법(설정창이 기본, `config.toml` 직접 편집도 가능), 창 크기 기억.
- `CLAUDE.md`: 구조 절의 `crates/core`·`crates/app`·`ui/` 설명, "앱은 레포에 아무것도 쓰지 않는다" 옆에 `config.toml`은 설정창 동작으로만 쓴다는 것.
- `docs/progress.md`: 7단계 줄과 결과.
