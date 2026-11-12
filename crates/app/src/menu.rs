//! app menu: Tauri 기본 menu에 `Settings…`(⌘,)를 더한다. Edit 등 나머지는 그대로 둔다 (Edit이 빠지면 webview에서 ⌘C·⌘V가 안 된다)
use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem};
use tauri::{AppHandle, Wry};

use crate::settings_window;

/// 설정창을 여는 menu item id. tray의 `설정…`도 같은 id다
pub const SETTINGS_ID: &str = "settings";

pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::default(app)?;
    let items = menu.items()?;
    // macOS 기본 menu의 첫 submenu가 app menu다 (About, Services, Hide, Quit). About 다음에 넣는다
    if let Some(app_menu) = items.first().and_then(|i| i.as_submenu()) {
        let settings = MenuItemBuilder::with_id(SETTINGS_ID, "Settings…").accelerator("CmdOrCtrl+,").build(app)?;
        app_menu.insert_items(&[&PredefinedMenuItem::separator(app)?, &settings], 1)?;
    }
    Ok(menu)
}

/// app menu와 tray 메뉴의 event가 모두 온다
pub fn on_event(app: &AppHandle, id: &str) {
    if id == SETTINGS_ID {
        settings_window::open(app);
    }
}
