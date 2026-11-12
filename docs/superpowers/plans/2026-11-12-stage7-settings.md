# 7단계: 설정창 (theme · 레포 관리) + 창 크기 기억 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `⌘,`로 여는 별도 설정창에서 Dark/Light theme을 바꾸고 레포를 추가·삭제·base 변경하며, main 창은 마지막 크기·위치를 기억해 지금 화면에 맞춰 뜬다.

**Architecture:** main 창을 `tauri.conf.json`이 아니라 `setup`에서 직접 만든다. 그 전에 `SettingsStore`를 열어 저장된 theme과 창 rect를 읽고, 순수 함수 `window_fit::fit`으로 띄울 rect를 정한다. theme은 `<html data-theme>` + Tailwind `dark:` variant로 나누고 `theme-changed` event로 모든 창에 알린다. 레포 변경은 `toml_edit`로 `config.toml`의 해당 entry만 고친 뒤 `Registry::apply`가 바뀐 레포만 열고 닫는다.

**Tech Stack:** Rust (tauri 2.12.4, rusqlite 0.32, 새로 `toml_edit` 0.22, `tauri-plugin-dialog` 2), React 19 + TanStack Query + Tailwind 4, vitest.

**Spec:** `docs/superpowers/specs/2026-11-12-stage7-settings-design.md`

## Global Constraints

- 앱은 레포에 아무것도 쓰지 않는다. 모든 git 호출은 `Git` 래퍼를 거친다. 앱이 새로 쓰는 파일은 `config.toml` 하나이고, 설정창의 추가·삭제·base 변경 때만 쓴다.
- parse에 실패하는 `config.toml`에는 쓰지 않는다. 쓸 때마다 파일을 새로 읽고 그 entry만 고친다. 주석·빈 줄·다른 table을 유지한다. 같은 폴더의 임시 파일에 쓴 뒤 rename.
- `state.sqlite` schema는 그대로(`user_version` 2). 새 `settings` key: `theme` = `dark` | `light` (없거나 모르는 값 = `dark`), `window_main` = JSON `{"x","y","width","height"}` (logical point).
- 창 기본 크기 1400×900. 저장된 가로나 세로가 200 미만이면 저장 값이 없는 것으로 본다. 복원 때 앱이 줄이거나 옮긴 rect는 저장하지 않는다 (사용자가 움직이거나 크기를 바꾼 뒤에만 저장).
- 배경색: dark `#030712`, light `#ffffff`. light 화면은 commit `39f989c`의 화면이다.
- 설정창: label `settings`, title `Settings`, 560×520, 크기 고정, 하나만.
- 사용자 문구 (그대로): `Settings…`(app menu, `⌘,`), `설정…`(tray), `Theme`, `Dark`, `Light`, `Repositories`, `＋ Repository 추가…`, `삭제`, `삭제할까요?`, `취소`, `미전송 코멘트 n개가 남습니다`, `git repository가 아닙니다`, `이미 등록된 repository입니다`, `자동 (origin/develop)`(괄호 안은 `origin/HEAD`가 가리키는 branch), `자동 (정할 수 없음)`, `PR이 있는 worktree는 PR base가 우선합니다`, `등록된 레포가 없습니다`, `Settings 열기`.
- 테스트를 먼저 쓴다 (실패 확인 → 구현 → 통과). 실제 git, 실제 SQLite 파일, 실제 파일. `source ~/.cargo/env` 먼저.
- branch `stage7-settings`. task마다 commit 직후 `simplify`를 돌리고 바뀐 것이 있으면 별도 commit. UI가 있는 task는 실제 앱 screenshot으로 확인한 뒤 끝낸다 (`scripts/fixtures.sh <dir>`, `WTBOARD_CONFIG=<dir>/config.toml pnpm tauri dev`).
- 실제 앱 자동화 규칙: 캡처·클릭 전에 wtboard를 frontmost로 올리고 좌표가 창 rect 안인지 확인한다. 키 입력은 보내지 않는다. app menu는 System Events의 `click menu item`으로 누른다. `⌘,` 키 자체와 폴더 dialog는 Task 9의 "사용자 확인"으로 남긴다.

## Review Focus

1. 저장된 rect가 지금 없는 모니터(primary 왼쪽, x가 음수)에 있음 → primary 화면 안으로 들어온다 (Task 1 `fit_moves_to_primary_when_monitor_is_gone`).
2. `settings`의 `theme`·`window_main`에 모르는 값이나 깨진 JSON → dark, 기본 크기. 오류 없음 (Task 1 `decode_rejects_garbage_and_tiny`, Task 3 `theme_from_setting_defaults_to_dark`).
3. 주석이 붙은 첫 `[[repo]]`를 삭제 → 주석이 같이 사라지지 않는다 (Task 5 `remove_keeps_leading_comment`).
4. 고른 폴더 경로에 한글·공백·`"`·`\` → 올바른 TOML 문자열로 쓰이고 다시 읽으면 같은 경로 (Task 5 `add_escapes_path`).
5. 설정창을 연 뒤 `config.toml`을 손으로 고쳐 그 entry가 없어짐 → 삭제·base 변경이 `RepoNotFound`로 실패하고 파일은 그대로 (Task 5 `edit_unknown_repo_is_error`). config에 적힌 base가 branch 목록에 없음 → 선택지에 그 값이 그대로 보인다 (Task 8 `baseOptions` test).

---

## File Structure

| 파일 | 할 일 |
|---|---|
| `crates/core/src/window_fit.rs` (새) | `Rect`, `Screen`, `Placement`, `decode`, `encode`, `fit`, `moved_by_user` |
| `crates/core/src/store/settings.rs` | `Theme`, `THEME_KEY`, `WINDOW_KEY` |
| `crates/core/src/store/config.rs` | `Config: Default`, `load_or_default`, `ConfigError` variant 추가 |
| `crates/core/src/store/config_edit.rs` (새) | `add_repo`, `remove_repo`, `set_repo_base`, `update_file`, `abbreviate_home` |
| `crates/core/src/git/branches.rs` (새) | `base_candidates`, `toplevel` |
| `crates/core/src/git/base.rs` | `origin_head` (기존 `resolve_base`에서 뽑는다) |
| `crates/core/src/registry.rs` | 내부 가변, `apply`, `repo`, `check_new_repo`, `inspect_entry` |
| `crates/app/src/window.rs` (새) | main 창 만들기, 모니터 → `Screen`, rect 저장 |
| `crates/app/src/theme.rs` (새) | theme 적용, `init_script`, command `get_settings`·`set_theme` |
| `crates/app/src/settings_window.rs` (새) | 설정창 열기, command `open_settings` |
| `crates/app/src/menu.rs` (새) | app menu (`Settings…`) |
| `crates/app/src/repos.rs` (새) | 레포 command 5개 |
| `crates/app/src/main.rs`, `tray.rs`, `tauri.conf.json`, `capabilities/default.json`, `Cargo.toml` | 연결 |
| `ui/index.html`, `ui/src/index.css`, `ui/src/components/*.tsx` | `data-theme`, light/dark class |
| `ui/src/lib/theme.ts`, `ui/src/useTheme.ts`, `ui/src/lib/settings.ts` (새) | theme 해석·구독, base 선택지 |
| `ui/src/settings/Settings.tsx`, `ui/src/settings/Repositories.tsx` (새) | 설정창 화면 |
| `ui/src/main.tsx`, `ui/src/App.tsx`, `ui/src/api.ts` | 창 label로 화면 고르기, 레포 0개 화면, command 래퍼 |

---

### Task 0: branch와 문서 commit

- [ ] **Step 1:** `git switch -c stage7-settings`
- [ ] **Step 2:** `git add docs/superpowers/specs/2026-11-12-stage7-settings-design.md docs/superpowers/plans/2026-11-12-stage7-settings.md && git commit -m "docs: 7단계 spec·plan (설정창, theme, 레포 관리, 창 크기 기억)"`

---

### Task 1: `core::window_fit` — 띄울 rect 정하기

**Files:**
- Create: `crates/core/src/window_fit.rs`, `crates/core/tests/window_fit.rs`
- Modify: `crates/core/src/lib.rs` (`pub mod window_fit;`)

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }
/// 모니터의 work area (logical point)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen { pub area: Rect, pub primary: bool }
/// position이 None이면 OS 기본 위치
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement { pub width: f64, pub height: f64, pub position: Option<(f64, f64)> }
pub const DEFAULT_SIZE: (f64, f64) = (1400.0, 900.0);
pub const MIN_SAVED: f64 = 200.0;
pub fn decode(value: Option<&str>) -> Option<Rect>;
pub fn encode(rect: &Rect) -> String;
pub fn fit(saved: Option<Rect>, screens: &[Screen]) -> Placement;
pub fn moved_by_user(baseline: &Rect, current: &Rect) -> bool;
```

- [ ] **Step 1: 실패하는 test 작성** (`crates/core/tests/window_fit.rs`). `mac` = `Screen { area: Rect { x: 0.0, y: 25.0, width: 1440.0, height: 875.0 }, primary: true }`, `ext` = `Screen { area: Rect { x: 1440.0, y: 0.0, width: 2560.0, height: 1440.0 }, primary: false }`.

| test | 입력 | 기대 |
|---|---|---|
| `fit_default_when_nothing_saved` | `None`, `[ext를 primary로]` | `{1400, 900, None}` |
| `fit_default_shrinks_to_screen` | `None`, `[mac]` | `{1400, 875, None}` |
| `fit_keeps_rect_inside_screen` | `{100,100,1200,800}`, `[mac]` | `{1200, 800, Some((100,100))}` |
| `fit_shrinks_both_axes` | `{1440,0,2560,1440}`, `[mac]` | `{1440, 875, Some((0,25))}` |
| `fit_shrinks_width_only` | `{200,100,2000,600}`, `[mac]` | `{1440, 600, Some((0,100))}` |
| `fit_moves_to_primary_when_monitor_is_gone` | `{-2460,200,1800,1000}`, `[mac]` | `{1440, 875, Some((0,25))}` |
| `fit_pulls_partly_offscreen_rect_in` | `{1000,500,800,600}`, `[mac]` | `{800, 600, Some((640,300))}` |
| `fit_picks_screen_with_most_overlap` | `{1500,100,2000,1200}`, `[mac, ext]` | `{2000, 1200, Some((1500,100))}` |
| `fit_without_screens_returns_saved` | `{10,10,900,700}`, `[]` | `{900, 700, Some((10,10))}` |
| `decode_roundtrip` | `decode(Some(&encode(&r)))` | `Some(r)` |
| `decode_rejects_garbage_and_tiny` | `None`, `"x"`, `{"x":0,"y":0,"width":150,"height":900}`, `{"x":0,"y":0,"width":900,"height":199}`, width가 `null` | 모두 `None` |
| `moved_by_user_ignores_subpoint_noise` | 같은 rect / y만 0.5 다름 / width만 10 다름 | `false` / `false` / `true` |

- [ ] **Step 2:** `cargo test -p wtboard-core --test window_fit` → compile 실패 확인
- [ ] **Step 3: 구현.** `fit` 순서: ① saved 없으면 `DEFAULT_SIZE`를 primary(없으면 첫 screen) area 크기로 줄이고 `position: None`. ② saved가 있으면 겹치는 넓이가 가장 큰 screen, 겹침이 0이면 primary(없으면 첫 screen). ③ 가로·세로를 따로 area 크기까지 줄인다. ④ `x = clamp(x, area.x, area.x + area.width - width)`, y도 같게. `moved_by_user`는 네 성분 중 하나라도 차이가 1.0을 넘으면 true. `decode`는 `serde_json`, 유한하지 않은 수도 `None`.
- [ ] **Step 4:** `cargo test -p wtboard-core --test window_fit` → 12개 통과
- [ ] **Step 5:** `git commit -m "feat(core): window_fit — 저장된 창 rect를 지금 화면에 맞춘다"` → simplify

---

### Task 2: main 창을 `setup`에서 만들고 크기·위치를 기억

**Files:**
- Create: `crates/app/src/window.rs`
- Modify: `crates/app/tauri.conf.json` (`"windows": []`), `crates/app/src/main.rs`, `crates/app/src/tray.rs`, `crates/core/src/store/settings.rs` (`pub const WINDOW_KEY: &str = "window_main";`)

**Interfaces:**
- Consumes: Task 1 전부.
- Produces:

```rust
// crates/app/src/window.rs
pub const MAIN: &str = "main";
/// physical work area → logical Screen
pub fn to_screen(x: i32, y: i32, width: u32, height: u32, scale: f64, primary: bool) -> Screen;
/// 저장된 rect로 main 창을 만든다. theme·배경은 지금은 Dark·#030712 (Task 3이 바꾼다)
pub fn create_main(app: &AppHandle, settings: &SettingsStore) -> tauri::Result<WebviewWindow>;
pub struct GeometrySaver { /* request(), flush() */ }
impl GeometrySaver {
    pub fn spawn(app: AppHandle, baseline: Rect) -> Self;
    pub fn request(&self);   // Moved·Resized마다. 마지막 요청 뒤 500ms에 한 번 저장을 시도한다
    pub fn flush(&self);     // 종료 직전. 기다리지 않고 바로 시도한다
}
// main.rs AppState
settings: SettingsStore,          // OnceLock이 아니다. setup에서 연다
geometry: OnceLock<GeometrySaver>,
```

- [ ] **Step 1: 실패하는 test** (`window.rs`의 `#[cfg(test)]`): `to_screen(0, 50, 2880, 1750, 2.0, true)` == `Screen { area: Rect { x: 0.0, y: 25.0, width: 1440.0, height: 875.0 }, primary: true }`; `to_screen(-2560, 0, 2560, 1440, 1.0, false)`의 `area.x == -2560.0`.
- [ ] **Step 2:** `cargo test -p wtboard-app` → 실패 확인
- [ ] **Step 3: 구현.**
  - `setup`: `SettingsStore`를 `data_dir().join("state.sqlite")`로 연다 (실패하면 `in_memory` + 기존 문구 `설정을 저장할 수 없음` 경고를 보관했다가 `open_registry`가 `reg.warn`). `open_registry`에서 settings 여는 부분을 지운다. `tray::apply_saved_on_top`은 창을 만든 직후 `setup`에서 부른다.
  - `create_main`: `app.available_monitors()` + `primary_monitor()` → `Screen` (`Monitor::work_area()`, `scale_factor()`), `fit(decode(settings.get(WINDOW_KEY)), &screens)` → `WebviewWindowBuilder::new(app, MAIN, WebviewUrl::default())` `.title("wtboard")` `.inner_size` `.position`(있을 때만).
  - baseline은 요청한 rect가 아니라 **창을 만든 뒤 실제로 읽은 rect**다 (OS가 조정할 수 있다).
  - 저장 시도: 창이 fullscreen·minimized·숨김이면 건너뛴다. 현재 rect(logical: `outer_position`, `inner_size`를 창의 `scale_factor`로)와 baseline에 `moved_by_user`가 true일 때만 `settings.set(WINDOW_KEY, encode)` 후 baseline을 갱신한다.
  - `on_window_event`: label이 `MAIN`인 `Moved`·`Resized` → `request()`. `RunEvent::ExitRequested` → `flush()`.
  - `tray.rs`의 `WINDOW`는 `window::MAIN`을 쓴다. `toggle_on_top`의 `settings.get()` 분기(OnceLock)는 없앤다.
- [ ] **Step 4:** `cargo test -p wtboard-app && cargo test -p wtboard-core` → 통과
- [ ] **Step 5: 실제 앱 확인** (fixture `<dir>`):
  1. 실행 → System Events로 `set size of window 1 to {1000, 700}`, `set position ... to {120, 90}` → 1초 뒤 `sqlite3 <dir>/state.sqlite "select value from settings where key='window_main'"` 가 그 값 → tray `종료` → 다시 실행 → `get {position, size} of window 1`이 같다.
  2. 종료 → `window_main`을 `{"x":-3000,"y":100,"width":5000,"height":4000}`로 직접 넣고 실행 → 창 rect가 Finder desktop bounds 안 → 창을 건드리지 않고 종료 → `window_main`이 넣은 값 그대로.
  3. `항상 위에 띄우기` 토글 → 재시작 뒤 유지 (settings 여는 자리를 옮겼으므로 회귀 확인).
- [ ] **Step 6:** `git commit -m "feat(app): main 창 크기·위치 기억, 지금 화면보다 크면 줄여서 띄운다"` → simplify

---

### Task 3: theme 저장·적용, 설정창, app menu

**Files:**
- Create: `crates/app/src/theme.rs`, `crates/app/src/settings_window.rs`, `crates/app/src/menu.rs`, `ui/src/lib/theme.ts`, `ui/src/lib/theme.test.ts`, `ui/src/useTheme.ts`, `ui/src/settings/Settings.tsx`
- Modify: `crates/core/src/store/settings.rs`, `crates/app/src/main.rs`, `crates/app/src/window.rs`, `crates/app/src/tray.rs`, `crates/app/capabilities/default.json` (`"windows": ["main", "settings"]`), `ui/index.html`, `ui/src/main.tsx`, `ui/src/api.ts`

**Interfaces:**
- Produces:

```rust
// core::store::settings
pub const THEME_KEY: &str = "theme";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme { Dark, Light }
impl Theme {
    pub fn from_setting(value: Option<&str>) -> Self;   // "light"만 Light
    pub fn as_str(self) -> &'static str;                // "dark" | "light"
    pub fn background(self) -> (u8, u8, u8);            // (3,7,18) | (255,255,255)
}
// app::theme
pub fn current(settings: &SettingsStore) -> Theme;
pub fn init_script(theme: Theme) -> String;             // `window.__WTBOARD_THEME__ = "light";`
pub fn apply(app: &AppHandle, theme: Theme);            // 모든 창 set_theme + set_background_color, emit "theme-changed" { theme }
#[tauri::command] fn get_settings(app) -> Result<SettingsDto, String>;   // { theme }
#[tauri::command] fn set_theme(app, theme: Theme) -> Result<(), String>; // 저장 + apply
// app::settings_window
pub const SETTINGS: &str = "settings";
pub fn open(app: &AppHandle);                           // 있으면 show + focus, 없으면 만든다
#[tauri::command] fn open_settings(app);
// app::menu
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>>;
```

```ts
// ui/src/lib/theme.ts
export type Theme = 'dark' | 'light';
export function parseTheme(value: unknown): Theme;      // 'light'만 light
// ui/src/api.ts
export function getSettings(): Promise<{ theme: Theme }>;
export function setTheme(theme: Theme): Promise<void>;
export function openSettings(): Promise<void>;
```

- [ ] **Step 1: 실패하는 test.**
  - core (`settings.rs` `#[cfg(test)]`): `theme_from_setting_defaults_to_dark` — `None`, `""`, `"LIGHT"`, `"solarized"` → `Dark`; `"light"` → `Light`; `"dark"` → `Dark`. `theme_roundtrips_through_store` — `in_memory()`에 `set(THEME_KEY, Theme::Light.as_str())` 후 `from_setting(get)` == `Light`.
  - app (`theme.rs`): `init_script(Theme::Light)` == `window.__WTBOARD_THEME__ = "light";`.
  - ui (`theme.test.ts`): `parseTheme('light')` → `'light'`; `'dark'`, `undefined`, `null`, `'x'`, `1` → `'dark'`.
- [ ] **Step 2:** `cargo test -p wtboard-core settings && cargo test -p wtboard-app && pnpm --dir ui test` → 실패 확인
- [ ] **Step 3: 구현 (Rust).**
  - `create_main`과 `settings_window::open`이 `.theme(Some(..))`, `.background_color(..)`, `.initialization_script(init_script(theme))`를 쓴다. 설정창은 `.inner_size(560.0, 520.0).resizable(false).title("Settings")`.
  - 설정창 닫기는 기존 `on_window_event`(닫기 = 숨기기)를 그대로 탄다. 다시 열면 show + focus. main 창을 같이 보이지 않는다.
  - `menu::build`: `Menu::default(app)`의 첫 submenu(app menu)에서 About 다음 자리에 separator와 `MenuItemBuilder::with_id("settings", "Settings…").accelerator("CmdOrCtrl+,")`를 넣는다. Edit·View·Window 등 나머지는 건드리지 않는다. `app.set_menu` + `app.on_menu_event`에서 id `settings` → `settings_window::open`.
  - tray 메뉴: `wtboard 열기` 다음에 `.text("settings", "설정…")`, `on_menu`에 `"settings" => settings_window::open(app)`. (app-level handler가 tray event도 받아 두 번 불려도 show + focus라 무해하다.)
- [ ] **Step 4: 구현 (UI).**
  - `ui/index.html` `<head>`에 inline script: `document.documentElement.dataset.theme = window.__WTBOARD_THEME__ === 'light' ? 'light' : 'dark';`
  - `useTheme()`: `theme-changed`를 듣고 `dataset.theme`을 바꾼다. main·설정 화면 둘 다 부른다.
  - `main.tsx`: `getCurrentWebviewWindow().label === 'settings'`이면 `<Settings />`, 아니면 `<App />`.
  - `Settings.tsx`: `Theme` 줄에 `Dark` | `Light` 두 칸 control. 누르면 `setTheme`. 현재 값은 `getSettings` + `theme-changed`. 지금은 dark 색만 입힌다 (light는 Task 4).
- [ ] **Step 5:** 위 test 전부 + `pnpm --dir ui build` → 통과
- [ ] **Step 6: 실제 앱 확인.**
  1. `osascript`로 `click menu item "Settings…" of menu "wtboard" of menu bar item "wtboard" of menu bar 1` → 설정창이 뜬다. 한 번 더 → 창이 하나다 (`count of windows`).
  2. 그 menu item의 `AXMenuItemCmdChar`가 `,`.
  3. tray `설정…`으로도 열린다. main 창을 숨긴 상태에서 열면 main 창은 숨은 채다.
  4. `Light` 클릭 → 두 창의 title bar가 light, `sqlite3 ... key='theme'` = `light`, 설정창 screenshot에서 `Light` 칸이 선택됨.
  5. 재시작 → 뜨자마자 찍은 screenshot의 title bar와 배경이 light. `Dark`로 되돌려 둔다.
- [ ] **Step 7:** `git commit -m "feat: 설정창(⌘,)과 theme 저장·적용"` → simplify

---

### Task 4: UI light 색

**Files:**
- Modify: `ui/src/index.css`, `ui/src/App.tsx`, `ui/src/components/{CommentWidget,CommentsPopover,FileDiff,FileTree,FsmonitorBanner,Sidebar,ViewTabs,WorktreeView}.tsx`, `ui/src/settings/Settings.tsx`

**Interfaces:**
- Consumes: Task 3의 `<html data-theme>`.
- Produces: 모든 component가 `<light class> dark:<dark class>` 형태. 뒤 task의 새 UI도 이 형태로 쓴다.

- [ ] **Step 1: 대응표 확보.** `git show 74a406c 7b5f434 -- ui/src`. `-` 줄의 class가 light, `+` 줄의 class가 dark다.
- [ ] **Step 2: `index.css`.**
  - `@custom-variant dark (&:where([data-theme=dark], [data-theme=dark] *));`
  - `:root[data-theme=dark] { color-scheme: dark; }`, `:root[data-theme=light] { color-scheme: light; }`
  - body: light는 배경 `#ffffff`와 `39f989c`의 글자색(기본), dark는 지금 값.
  - `--diff-*` 색 변수 묶음과 dark `.token.*` 색은 `[data-theme=dark]` 아래로. light `.token.*` 색은 `39f989c`의 값(`#6a737d`, `#d73a49`, `#032f62`, `#6f42c1`, `#005cc5`, `#22863a`). `--diff-font-family`와 `.diff { font-size }`는 theme과 무관하게 둔다.
- [ ] **Step 3: component.** 대응표의 각 class 쌍을 `X dark:Y`로 바꾼다. `7b5f434`가 지운 `text-gray-200`(body 기본색과 같아서 지운 것)은 되살리지 않는다.
- [ ] **Step 4: 기계적 확인.** 아래가 아무것도 출력하지 않아야 한다 (dark variant를 지우면 light 시절 파일과 같다):

```bash
for f in App.tsx components/{CommentWidget,CommentsPopover,FileDiff,FileTree,FsmonitorBanner,Sidebar,ViewTabs,WorktreeView}.tsx; do
  diff <(git show 39f989c:ui/src/$f | tr -s ' ') <(sed -E 's/ ?dark:[^ "`'"'"']+//g' ui/src/$f | tr -s ' ') >/dev/null || echo "DIFF $f"
done
```

  Task 3이 `App.tsx`에 더한 줄(`useTheme`) 같은 의도한 차이만 남으면 된다. 남은 차이는 `diff`로 직접 보고 색과 무관한지 확인한다.
- [ ] **Step 5:** `Settings.tsx`에 light 색을 입힌다 (main 창 light와 같은 gray 계열, 선택된 칸은 dark와 같은 blue).
- [ ] **Step 6:** `pnpm --dir ui test && pnpm --dir ui build` → 통과
- [ ] **Step 7: 실제 앱 screenshot.** light: sidebar(선택된 worktree, ●, 💬), diff(추가·삭제 줄, syntax 색), 코멘트 widget(쓰는 중·미전송·전송됨), 코멘트 popover, `WTBOARD_SLOW_LIST_MS=0`의 banner, 설정창. dark: 같은 화면이 Task 3 끝의 모습과 같다. 글자가 배경에 묻히는 곳이 없는지 본다.
- [ ] **Step 8:** `git commit -m "feat(ui): light theme (data-theme + dark: variant)"` → simplify

---

### Task 5: `config.toml` 고치기 (core)

**Files:**
- Create: `crates/core/src/store/config_edit.rs`
- Modify: `crates/core/src/store/config.rs`, `crates/core/src/store/mod.rs` (`pub mod config_edit;`), `crates/core/Cargo.toml` (`toml_edit = "0.22"`), `crates/core/tests/config.rs`

**Interfaces:**
- Produces:

```rust
// config.rs
#[derive(Debug, Default, Deserialize)] pub struct Config { .. }       // Default 추가
impl Config { pub fn load_or_default(path: &Path) -> Result<Self, ConfigError>; }  // 파일이 없으면 빈 Config
pub enum ConfigError { Io{..}, Parse(..),
    Write { path: PathBuf, source: std::io::Error },   // "설정 파일을 쓸 수 없음: {path}: {source}"
    Edit(#[from] toml_edit::TomlError),
    RepoNotFound(String),                                // "설정 파일에 없는 레포: {0}"
}
// config_edit.rs — 문자열 → 문자열. repo_path는 config에 적힌 path 문자열 그대로
pub fn add_repo(src: &str, repo_path: &str) -> Result<String, ConfigError>;
pub fn remove_repo(src: &str, repo_path: &str) -> Result<String, ConfigError>;
pub fn set_repo_base(src: &str, repo_path: &str, base: Option<&str>) -> Result<String, ConfigError>;
/// 읽기(없으면 "") → Config::parse 확인 → f → 결과도 parse 확인 → 임시 파일 → rename. 폴더가 없으면 만든다
pub fn update_file(path: &Path, f: impl FnOnce(&str) -> Result<String, ConfigError>) -> Result<(), ConfigError>;
pub fn abbreviate_home(p: &Path, home: &Path) -> String;
```

- [ ] **Step 1: 실패하는 test** (`crates/core/tests/config.rs`에 추가). 공통 `SRC`:

```toml
# base 생략 → origin/HEAD
[[repo]]
path = "~/a"

[[repo]]
path = "~/b"
base = "origin/main"

[review]
auto_attach_comments = false
```

| test | 동작 | 기대 |
|---|---|---|
| `add_appends_repo_and_keeps_rest` | `add_repo(SRC, "~/c")` | parse한 `repos`의 path가 `[~/a, ~/b, ~/c]`, 마지막 `base` 없음, `review.auto_attach_comments == false`, 결과에 `# base 생략 → origin/HEAD`가 있다, `SRC`의 모든 줄이 원래 순서대로 남아 있다 |
| `add_to_empty_source` | `add_repo("", "~/c")` | repos == `[~/c]` |
| `add_escapes_path` | `add_repo(SRC, "/Users/x/한글 폴더/a\"b\\c")` | parse한 마지막 path가 입력과 같다 |
| `remove_keeps_leading_comment` | `remove_repo(SRC, "~/a")` | repos == `[~/b]` (base `origin/main`), 결과에 `# base 생략 → origin/HEAD`가 있다, `[review]` 그대로 |
| `remove_last_repo_leaves_valid_file` | `~/a`, `~/b` 차례로 삭제 | repos 비었음, `review.auto_attach_comments == false` |
| `set_base_writes_and_clears` | `set_repo_base(SRC, "~/a", Some("origin/develop"))` → `~/a`의 base가 그 값, `~/b`는 `origin/main` 그대로. `set_repo_base(SRC, "~/b", None)` → `~/b`의 base 없음 |
| `edit_unknown_repo_is_error` | `remove_repo(SRC, "~/zzz")`, `set_repo_base(SRC, "~/zzz", None)` | `Err(RepoNotFound("~/zzz"))` |
| `duplicate_path_edits_first` | 같은 path 둘인 src에서 `remove_repo` | 하나 남는다 |
| `update_file_creates_missing_file_and_dir` | tempdir의 `x/y/config.toml`에 add | 파일이 생기고 repos 1개, 폴더에 `config.toml`뿐 (임시 파일 없음) |
| `update_file_refuses_broken_file` | 내용 `[[repo` | `Err(Parse)`, 파일 내용 그대로 |
| `load_or_default_missing_file_is_empty` | 없는 경로 | `repos` 비었음. 깨진 파일은 `Err` |
| `abbreviate_home_cases` | (`/Users/jo/p/x`, `/Users/jo`) → `~/p/x`; (`/opt/x`, ..) → `/opt/x`; (`/Users/jo`, ..) → `~`; (`/Users/jo2/x`, `/Users/jo`) → `/Users/jo2/x` |

- [ ] **Step 2:** `cargo test -p wtboard-core --test config` → 실패 확인
- [ ] **Step 3: 구현.** `toml_edit::DocumentMut`. `remove_repo`는 지우는 table 앞에 붙은 주석(decor prefix)을 다음 `[[repo]]`(없으면 문서의 다음 항목이나 끝)로 옮긴다 — `toml_edit`는 파일 첫 주석을 첫 table의 decor로 들고 있어서 그냥 지우면 주석이 같이 사라진다. `abbreviate_home`은 문자열 prefix가 아니라 `Path::strip_prefix`(component 단위).
- [ ] **Step 4:** `cargo test -p wtboard-core --test config` → 통과
- [ ] **Step 5:** `git commit -m "feat(core): config.toml의 [[repo]]를 주석을 유지한 채 추가·삭제·base 변경"` → simplify

---

### Task 6: base 후보와 worktree root (core::git)

**Files:**
- Create: `crates/core/src/git/branches.rs`, `crates/core/tests/branches.rs`
- Modify: `crates/core/src/git/mod.rs` (`pub mod branches;`), `crates/core/src/git/base.rs`, `crates/core/tests/read_only.rs`

**Interfaces:**
- Produces:

```rust
// git/branches.rs
/// remote branch(이름순, `<remote>/HEAD` 제외) 다음 local branch(이름순). 예: ["origin/develop", "origin/main", "feat", "main"]
pub fn base_candidates(git: &Git) -> Result<Vec<String>, GitError>;
/// `rev-parse --show-toplevel`
pub fn toplevel(git: &Git) -> Result<PathBuf, GitError>;
// git/base.rs
/// `origin/HEAD`가 가리키는 branch (`origin/develop`). 없으면 None. resolve_base도 이것을 쓴다
pub fn origin_head(git: &Git) -> Option<String>;
```

- [ ] **Step 1: 실패하는 test** (`tests/branches.rs`, `common::clone_on_feat_with_develop()` 사용 — upstream에 `main`·`develop`, clone은 `feat` branch):
  - `candidates_list_remote_then_local`: 결과가 `origin/`로 시작하는 것들이 먼저 이름순, 그 뒤 local이 이름순. `origin/develop`, `origin/main`, `feat`를 포함. `HEAD`로 끝나는 항목과 `origin` 한 단어짜리 항목이 없다.
  - `candidates_without_remote`: `TestRepo::new()` → local branch만.
  - `toplevel_from_subdirectory`: `<repo>/sub/dir`에서 → `<repo>` (둘 다 `canonicalize`해서 비교. macOS tempdir는 `/private/var`).
  - `toplevel_outside_repo_is_error`: 빈 tempdir → `Err`.
  - `origin_head_some_and_none`: clone → `Some(format!("origin/{}", upstream의 `git branch --show-current`))`, `TestRepo::new()` → `None`.
  - `read_only.rs`: 기존 불변 확인 묶음에 `base_candidates`, `toplevel`, `origin_head` 호출을 더한다.
- [ ] **Step 2:** `cargo test -p wtboard-core --test branches --test read_only` → 실패 확인
- [ ] **Step 3: 구현.** `for-each-ref --format=%(refname) refs/remotes refs/heads`로 full name을 받아 prefix를 직접 뗀다 (`%(refname:short)`는 `refs/remotes/origin/HEAD`를 `origin`으로 줄인다).
- [ ] **Step 4:** `cargo test -p wtboard-core` → 전체 통과
- [ ] **Step 5:** `git commit -m "feat(core): base 후보 branch 목록, worktree root, origin_head"` → simplify

---

### Task 7: `Registry::apply` — 열린 뒤에도 config에 맞춘다

**Files:**
- Modify: `crates/core/src/registry.rs`, `crates/core/tests/registry.rs`, `crates/app/src/main.rs` (`reg.warn`이 `&self`가 되는 것, `errors()` 반환형만 맞춘다)

**Interfaces:**
- Consumes: Task 6 `toplevel`, `origin_head`; 기존 `common_dir`, `list_worktrees`.
- Produces:

```rust
impl Registry {
    pub fn open(config: &Config, gh: Gh, viewed: Arc<ViewedStore>, sink: Sink) -> Self;  // 빈 registry + apply. signature 그대로
    /// config에 맞춘다. common dir와 base가 같은 레포는 그대로 두고, 새 레포는 열고, 없어진 레포는 닫는다.
    /// base가 바뀐 레포는 닫고 다시 연다. 돌려주는 것: 이번에 (다시) 연 레포의 worktree id
    pub fn apply(&self, config: &Config) -> Vec<String>;
    pub fn repo(&self, id: &str) -> Option<Arc<Repo>>;      // id = common dir 문자열 (RepoSummary.id)
    pub fn errors(&self) -> Vec<EntryError>;                // 마지막 apply의 entry 오류 + warn. (&[..]에서 바뀜)
    pub fn warn(&self, e: EntryError);                      // (&mut self에서 바뀜). apply 뒤에도 남는다
}
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum AddRepoError {
    #[error("git repository가 아닙니다")] NotGitRepo,
    #[error("이미 등록된 repository입니다")] AlreadyRegistered,
}
/// 고른 폴더를 등록해도 되는지 보고, 등록할 경로(그 worktree의 root)를 돌려준다
pub fn check_new_repo(config: &Config, picked: &Path) -> Result<PathBuf, AddRepoError>;
/// 설정창의 한 줄. 열 수 없는 entry는 error만 채운다
pub struct EntryInfo { pub name: String, pub auto_base: Option<String>, pub worktrees: Vec<PathBuf>, pub error: Option<String> }
pub fn inspect_entry(entry: &RepoConfig) -> EntryInfo;
```

- [ ] **Step 1: 실패하는 test** (`tests/registry.rs`. config는 기존 helper처럼 문자열로 만든다):
  - `empty_config_is_not_an_error`: `Config::parse("")` → `repos()` 비었음, `errors()` 비었음.
  - `apply_opens_added_repo`: A로 open → A,B config로 `apply` → `repos()` 2개, 반환값이 B의 worktree id들이고 A의 것은 없다.
  - `apply_keeps_unchanged_repo`: `Arc::ptr_eq(&before_a, &reg.repo(a_id).unwrap())`.
  - `apply_closes_removed_repo`: A,B → A config → `repos()` 1개, `reg.worktree(b_wt_id)` == `None`, `roots()`에 B 없음.
  - `apply_reopens_repo_when_base_changes`: A(base 없음) → A(base `main`) → `ptr_eq`가 false, 반환값에 A의 worktree id, summary의 `base == "main"`·`base_source == Config`. 같이 있던 B는 `ptr_eq` true.
  - `apply_refreshes_entry_errors_and_keeps_warnings`: 없는 경로 entry로 open + `warn(x)` → `errors()` 2개 → 그 entry를 뺀 config로 `apply` → `errors()` == `[x]`.
  - `check_new_repo_cases`: 일반 폴더 → `NotGitRepo`; `<repo>/sub` → `Ok(<repo>)` (canonicalize 비교); 등록된 레포의 linked worktree → `AlreadyRegistered`; 등록된 레포 자신 → `AlreadyRegistered`; config에 열 수 없는 entry가 섞여 있어도 판단에 영향 없음.
  - `inspect_entry_cases`: clone → `name` = 폴더 이름, `auto_base` = `origin_head`, `worktrees`에 root; 없는 경로 → `error == Some("경로 없음")`, 나머지 비었음.
- [ ] **Step 2:** `cargo test -p wtboard-core --test registry` → 실패 확인
- [ ] **Step 3: 구현.** `Registry`가 `gh`·`viewed`·`sink`를 들고, 레포 목록은 `RwLock<Vec<(common_dir, base, Arc<Repo>)>>`, entry 오류와 warn은 따로 `Mutex`. `apply`끼리는 `Mutex<()>`로 하나씩. 새 레포를 여는 동안(수 초) 목록 write lock을 잡지 않는다 — 다 연 뒤 한 번에 바꾼다. 목록 순서는 config 순서. 기존 묶기 규칙(같은 common dir entry는 하나로, base가 다르면 경고)은 그대로.
- [ ] **Step 4:** `cargo test -p wtboard-core && cargo build -p wtboard-app` → 통과
- [ ] **Step 5:** `git commit -m "feat(core): Registry::apply — 재시작 없이 레포 추가·삭제·base 변경"` → simplify

---

### Task 8: 레포 관리 command와 설정창 UI, 레포 0개 화면

**Files:**
- Create: `crates/app/src/repos.rs`, `ui/src/settings/Repositories.tsx`, `ui/src/lib/settings.ts`, `ui/src/lib/settings.test.ts`
- Modify: `crates/app/Cargo.toml` (`tauri-plugin-dialog = "2"`), `crates/app/src/main.rs`, `ui/src/api.ts`, `ui/src/settings/Settings.tsx`, `ui/src/App.tsx`, `ui/src/lib/selection.test.ts`

**Interfaces:**
- Consumes: Task 5 `config_edit::*`, `Config::load_or_default`; Task 6 `base_candidates`; Task 7 `apply`, `check_new_repo`, `inspect_entry`; Task 3 `openSettings`.
- Produces:

```rust
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct RepoEntry { path: String, name: String, base: Option<String>, auto_base: Option<String>, error: Option<String>, unsent_comments: usize }
#[derive(Serialize)] #[serde(rename_all = "camelCase")]
pub struct RepoEntries { entries: Vec<RepoEntry>, config_path: String, error: Option<String> }  // error = config parse 실패 문구
#[tauri::command] async fn list_repo_entries(app) -> Result<RepoEntries, String>;
#[tauri::command] async fn list_base_candidates(path: String) -> Result<Vec<String>, String>;   // path = config에 적힌 문자열
#[tauri::command] async fn add_repo(app) -> Result<Option<String>, String>;   // dialog 취소 = Ok(None). Ok(Some(쓴 path))
#[tauri::command] async fn remove_repo(app, path: String) -> Result<(), String>;
#[tauri::command] async fn set_repo_base(app, path: String, base: Option<String>) -> Result<(), String>;
// AppState: config_lock: Mutex<()>
```

```ts
// ui/src/lib/settings.ts
/** current가 후보에 없으면 맨 앞에 더한다. current가 null이면 후보 그대로 */
export function baseOptions(candidates: string[], current: string | null): string[];
/** `자동 (origin/develop)` / `자동 (정할 수 없음)` */
export function autoLabel(autoBase: string | null): string;
```

- [ ] **Step 1: 실패하는 test.**
  - `settings.test.ts`: `baseOptions(['origin/main','main'], 'v1.2')` → `['v1.2','origin/main','main']`; `(.., 'origin/main')`·`(.., null)` → 후보 그대로. `autoLabel('origin/develop')` → `자동 (origin/develop)`; `autoLabel(null)` → `자동 (정할 수 없음)`.
  - `selection.test.ts`: 선택된 worktree의 레포가 목록에서 사라지면 `resolveSelection`이 남은 첫 worktree를 돌려주고, 레포가 0개면 `null` (이미 있으면 더하지 않는다).
- [ ] **Step 2:** `pnpm --dir ui test` → 실패 확인
- [ ] **Step 3: 구현 (Rust).**
  - `open_registry`: `Config::load_or_default`. "등록된 레포가 없습니다" `Err`를 없앤다 (빈 registry). parse 실패는 지금처럼 `Err`이고 문구에 설정 파일 경로를 남긴다. 손으로 `[[repo]]`를 쓰라는 hint는 parse 실패 때만 붙인다.
  - 세 변경 command의 공통 흐름: `config_lock` → `config_edit::update_file` → `Config::load` → `registry(&app)?.apply(&config)` → 반환된 id마다 `worktree-changed { worktree_id, changed_paths: [], scope: Base }` emit → `worktrees-updated` emit → `request_tray`.
  - `add_repo`: 환경 변수 `WTBOARD_PICK_FOLDER`가 있으면 dialog 대신 그 경로 (확인용. `WTBOARD_SLOW_LIST_MS`와 같은 성격), 없으면 `tauri-plugin-dialog`의 폴더 선택. `check_new_repo` → `abbreviate_home` → `config_edit::add_repo`. `AddRepoError`는 문구 그대로 `Err`.
  - `list_repo_entries`: registry를 기다리지 않는다. `Config::load_or_default` (parse 실패면 `entries` 비고 `error`), entry마다 `inspect_entry`, `unsent_comments`는 `CommentStore::unsent_counts()`에서 그 entry의 `worktrees` 합 (comments store가 아직 없으면 0). entry끼리 병렬.
  - `main.rs`: `.plugin(tauri_plugin_dialog::init())`, command 등록.
- [ ] **Step 4: 구현 (UI).**
  - `Repositories.tsx`: query `['repo-entries']`, `['base-candidates', path]`(그 줄의 select를 처음 열 때). 변경 뒤와 `worktrees-updated` 때 `['repo-entries']`를 다시 읽는다. 줄: 이름, config의 path, base `<select>`(첫 option `autoLabel`, 그 아래 `baseOptions`. `title`에 `PR이 있는 worktree는 PR base가 우선합니다`), `삭제`. `삭제`를 누르면 그 줄이 `삭제할까요?` + (`unsentComments > 0`이면 `미전송 코멘트 n개가 남습니다`) + `취소`·`삭제`로 바뀐다. `error`가 있는 줄은 이유를 보이고 select를 끈다. command가 `Err`를 주면 목록 아래에 그 문구를 보인다. `RepoEntries.error`가 있으면 그 문구와 `configPath`를 보이고 추가·삭제·select를 모두 끈다.
  - `App.tsx`: `repos.data.repos.length === 0`이면 본문 자리에 `등록된 레포가 없습니다`와 `Settings 열기` 버튼(`openSettings`, 옆에 `⌘,`). sidebar의 오류 목록은 그대로 보인다.
  - 새 화면은 Task 4의 `X dark:Y` 형태로 쓴다.
- [ ] **Step 5:** `pnpm --dir ui test && pnpm --dir ui build && cargo test` → 통과
- [ ] **Step 6: 실제 앱 확인.** fixture `<dir>`와 따로 만든 git 레포 `<extra>`(remote 있는 clone 하나, `git init`만 한 것 하나).
  1. `WTBOARD_CONFIG=<empty>/config.toml`(파일 없음)로 실행 → main 창에 `등록된 레포가 없습니다`. `Settings 열기` 클릭 → 설정창.
  2. `WTBOARD_PICK_FOLDER=<extra clone>/src`로 실행해 `＋ Repository 추가…` → `<empty>/config.toml`이 생기고 path가 clone root, sidebar에 그 레포. base 표시가 `자동 (origin/…)`.
  3. 같은 폴더로 한 번 더 → `이미 등록된 repository입니다`, 파일 그대로. 일반 폴더 → `git repository가 아닙니다`.
  4. fixture config 첫 줄에 주석을 넣고 실행 → 한 레포의 base를 `origin/main` 등으로 변경 → `config.toml`에 `base = ...`, 주석 그대로, main 창 그 worktree header의 base와 파일 목록이 바뀜 (다른 레포의 `lastChangedAt` 표시는 그대로).
  5. 미전송 코멘트가 있는 레포 `삭제` → `미전송 코멘트 n개가 남습니다` → `삭제` → sidebar에서 사라지고 선택이 다른 worktree로 넘어감. `sqlite3`로 그 코멘트가 남아 있는지 확인.
  6. `config.toml`을 `[[repo`로 깨고 설정창을 다시 열기 → 오류 문구, 버튼 비활성, theme 스위치는 동작.
  7. light에서 설정창·빈 화면 screenshot.
- [ ] **Step 7:** `git commit -m "feat: 설정창에서 레포 추가·삭제·base 변경, 레포 0개 화면"` → simplify

---

### Task 9: 문서, 전체 확인, 사용자 확인

**Files:**
- Modify: `README.md`, `CLAUDE.md`, `docs/progress.md`

- [ ] **Step 1:** `README.md` — 설정창(`⌘,`, `wtboard > Settings…`, tray `설정…`), theme, 레포 등록(설정창이 기본, `config.toml` 직접 편집도 가능), 창 크기 기억, `WTBOARD_PICK_FOLDER`.
- [ ] **Step 2:** `CLAUDE.md` — 구조 절(`core::window_fit`, `store/config_edit`, `git/branches`, `Registry::apply`, `crates/app`의 `window.rs`·`theme.rs`·`settings_window.rs`·`menu.rs`·`repos.rs`와 새 command·`theme-changed` event, `ui/src/settings/`), "지켜야 할 것"에 "`config.toml`은 설정창의 추가·삭제·base 변경 때만 쓴다. parse에 실패하는 파일에는 쓰지 않는다".
- [ ] **Step 3:** `docs/progress.md` — 단계 표 7번, "7단계 결과", "7단계에서 정한 것 (spec과 다른 점)", "7단계 알려진 한계"(삭제한 레포의 미전송 코멘트를 hook이 계속 붙인다, `config.toml` 바깥 편집은 재시작 필요), "7단계 사용자 확인". 5단계 알려진 한계의 "settings가 열리지 않으면 … 저장이 조용히 빠진다"는 해결됐다고 고친다. "7단계 후보"는 "8단계 후보"로.
- [ ] **Step 4:** `pnpm test` (UI + cargo 전체) → 통과. `cargo test -p wtboard-hook` → 통과.
- [ ] **Step 5:** spec의 완료 기준 6개를 fixture에서 처음부터 한 번씩 다시 확인하고 screenshot을 남긴다.
- [ ] **Step 6:** `git commit -m "docs: 7단계 결과 (설정창, theme, 레포 관리, 창 크기 기억)"`
- [ ] **Step 7: 사용자 확인 목록** (자동화하지 않은 것. `docs/progress.md` "7단계 사용자 확인"에 체크박스로 적는다):
  - `⌘,` 키로 설정창이 열린다.
  - `＋ Repository 추가…`의 폴더 dialog로 실제 레포를 고른다.
  - 외부 모니터에서 키운 창 → 모니터 없이 실행 → 화면 안에 들어옴 → 그대로 종료 → 모니터 연결 뒤 실행 → 큰 크기로 돌아옴.
  - 설정 창에서 `⌘C`/`⌘V`, main 창에서 `⌘=`/`⌘-`/`⌘0`이 그대로 동작한다 (app menu를 바꿨으므로).
- [ ] **Step 8:** branch 마무리는 `superpowers:finishing-a-development-branch`. `/Applications/wtboard.app` 교체(`pnpm tauri build` 후 복사)는 사용자에게 확인한 뒤에 한다.
