//! 메뉴바(tray): 전체 미확인 수와 worktree 바로 가기
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use serde::Serialize;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};
use wtboard_core::repo::TrayWorktree;
use wtboard_core::session::list::active_roots;
use wtboard_core::store::data_dir;
use wtboard_core::store::settings::SettingsStore;
use wtboard_core::worktree::now_ms;

const TRAY_ID: &str = "main";
const WINDOW: &str = "main";
/// `settings` key. 값은 `"1"` / `"0"`
const ON_TOP_KEY: &str = "always_on_top";
/// 요청이 몰려도 이 간격에 한 번만 계산한다
const THROTTLE: Duration = Duration::from_secs(1);
/// 요청이 없어도 이 간격으로 다시 계산한다 (활성 세션 만료)
const TIMER: Duration = Duration::from_secs(60);

/// 메뉴의 worktree 줄 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayItem {
    pub id: String,
    pub label: String,
    pub count: usize,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayModel {
    /// 아이콘 옆 글자. 미확인이 없으면 없다
    pub title: Option<String>,
    pub header: String,
    pub items: Vec<TrayItem>,
}

/// 합계는 모든 worktree, 메뉴 줄은 미확인이 있거나 활성 세션이 있는 것만 (입력 순서 = sidebar 순서)
pub fn model(wts: &[TrayWorktree], active: &HashSet<PathBuf>) -> TrayModel {
    let total: usize = wts.iter().map(|w| w.unviewed).sum();
    let items = wts
        .iter()
        .map(|w| (w, active.contains(Path::new(&w.id))))
        .filter(|(w, active)| w.unviewed > 0 || *active)
        .map(|(w, active)| TrayItem {
            id: w.id.clone(),
            label: format!("{} {} · {}", if active { '●' } else { ' ' }, w.repo, w.branch),
            count: w.unviewed,
            active,
        })
        .collect();
    TrayModel { title: (total > 0).then(|| total.to_string()), header: format!("미확인 {total}개"), items }
}

/// `select-worktree` event. UI가 그 worktree를 선택한다
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SelectWorktree {
    worktree_id: String,
}

/// tray 갱신 요청을 모아 계산한다. `request()`는 어디서 몇 번 불러도 된다
pub struct TrayUpdater {
    tx: mpsc::Sender<()>,
}

impl TrayUpdater {
    /// 갱신 thread를 띄운다. registry가 열릴 때까지 기다린 뒤 계산한다
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut shown = None;
            loop {
                if let Err(RecvTimeoutError::Disconnected) = rx.recv_timeout(TIMER) {
                    return;
                }
                while rx.try_recv().is_ok() {}
                refresh(&app, &mut shown);
                std::thread::sleep(THROTTLE);
            }
        });
        Self { tx }
    }

    pub fn request(&self) {
        let _ = self.tx.send(());
    }
}

/// 메뉴바 아이콘을 만든다. 숫자와 worktree 줄은 첫 갱신 때 채운다
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let empty = model(&[], &HashSet::new());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .menu(&menu(app, &empty, false)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;
    Ok(())
}

/// 저장된 "항상 위에"를 창에 적용한다. 앱 시작 때 설정을 연 직후 부른다
pub fn apply_saved_on_top(app: &AppHandle, settings: &SettingsStore) {
    if settings.get(ON_TOP_KEY).ok().flatten().as_deref() == Some("1") {
        if let Some(w) = app.get_webview_window(WINDOW) {
            let _ = w.set_always_on_top(true);
        }
    }
}

/// 창을 보이고 앞으로 가져온다 (닫기 = 숨기기라서 숨어 있을 수 있다)
pub fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(WINDOW) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// `shown`은 마지막으로 보인 (model, 항상 위에). 같으면 메뉴를 다시 만들지 않는다 (파일 event마다 불린다)
fn refresh(app: &AppHandle, shown: &mut Option<(TrayModel, bool)>) {
    let Ok(reg) = crate::registry(app) else { return };
    let m = model(&reg.tray_worktrees(), &active_roots(&data_dir(), &reg.roots(), now_ms()));
    // 창 상태를 그대로 보인다. 저장 값과 어긋나도 실제 동작이 보인다
    let on_top = app.get_webview_window(WINDOW).and_then(|w| w.is_always_on_top().ok()).unwrap_or(false);
    if shown.as_ref().is_some_and(|(sm, so)| *sm == m && *so == on_top) {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    match menu(app, &m, on_top) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => eprintln!("[wtboard] 메뉴바 메뉴를 만들 수 없음: {e}"),
    }
    let _ = tray.set_title(m.title.as_deref());
    *shown = Some((m, on_top));
}

fn menu(app: &AppHandle, m: &TrayModel, on_top: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let mut b = MenuBuilder::new(app).item(&MenuItemBuilder::new(&m.header).enabled(false).build(app)?).separator();
    for i in &m.items {
        let text = if i.count > 0 { format!("{}    {}", i.label, i.count) } else { i.label.clone() };
        b = b.text(format!("wt:{}", i.id), text);
    }
    if !m.items.is_empty() {
        b = b.separator();
    }
    b.text("open", "wtboard 열기")
        .item(&CheckMenuItemBuilder::with_id(ON_TOP_KEY, "항상 위에 띄우기").checked(on_top).build(app)?)
        .separator()
        .text("quit", "종료")
        .build()
}

fn on_menu(app: &AppHandle, id: &str) {
    match id {
        "open" => show_window(app),
        ON_TOP_KEY => toggle_on_top(app),
        "quit" => app.exit(0),
        _ => {
            if let Some(worktree_id) = id.strip_prefix("wt:") {
                show_window(app);
                let _ = app.emit("select-worktree", SelectWorktree { worktree_id: worktree_id.to_owned() });
            }
        }
    }
}

/// 창의 "항상 위에"를 뒤집고 저장한다. 설정이 아직 열리지 않았으면 이번 실행에만 적용된다
fn toggle_on_top(app: &AppHandle) {
    let Some(w) = app.get_webview_window(WINDOW) else { return };
    let on = !w.is_always_on_top().unwrap_or(false);
    let _ = w.set_always_on_top(on);
    if let Some(settings) = app.state::<crate::AppState>().settings.get() {
        if let Err(e) = settings.set(ON_TOP_KEY, if on { "1" } else { "0" }) {
            eprintln!("[wtboard] 항상 위에 설정을 저장할 수 없음: {e}");
        }
    }
    // check 표시를 창 상태에 맞춘다
    crate::request_tray(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wt(id: &str, unviewed: usize) -> TrayWorktree {
        TrayWorktree { id: format!("/w/{id}"), repo: "r".into(), branch: id.into(), unviewed }
    }

    #[test]
    fn title_is_total_or_none() {
        let m = model(&[wt("a", 3), wt("b", 0), wt("c", 4)], &HashSet::new());
        assert_eq!(m.title.as_deref(), Some("7"));
        assert_eq!(m.header, "미확인 7개");
        let m = model(&[wt("a", 0), wt("b", 0)], &HashSet::new());
        assert_eq!(m.title, None);
        assert_eq!(m.header, "미확인 0개");
    }

    #[test]
    fn items_unviewed_or_active() {
        let active = HashSet::from([PathBuf::from("/w/b")]);
        let m = model(&[wt("a", 3), wt("b", 0), wt("c", 0)], &active);
        let ids: Vec<&str> = m.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["/w/a", "/w/b"]);
        assert_eq!(m.items[0].label, "  r · a");
        assert_eq!(m.items[0].count, 3);
        assert!(!m.items[0].active);
        assert!(m.items[1].label.starts_with('●'), "{}", m.items[1].label);
        assert!(m.items[1].active);
    }
}
