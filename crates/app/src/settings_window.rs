//! 설정창. 하나만 띄운다. 닫기 = 숨기기(`on_window_event`)라서 다시 열면 숨은 창을 앞으로 가져온다
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{theme, window};

pub const SETTINGS: &str = "settings";

/// 설정창만 보인다. main 창이 숨어 있어도 같이 보이지 않는다
pub fn open(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        window::bring_to_front(&w);
        return;
    }
    let current = theme::current(&app.state::<crate::AppState>().settings);
    let builder = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::default())
        .title("Settings")
        .inner_size(560.0, 520.0)
        .resizable(false);
    if let Err(e) = theme::style(builder, current).build() {
        eprintln!("[wtboard] 설정창을 열 수 없음: {e}");
    }
}

/// main 창의 빈 화면 버튼이 부른다. 창을 만드는 일이라 async(main thread 밖)로 둔다
#[tauri::command]
pub async fn open_settings(app: AppHandle) {
    open(&app);
}
