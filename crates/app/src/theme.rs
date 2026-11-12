//! theme: 저장된 값 읽기, 창에 적용, 설정창 command
use serde::Serialize;
use tauri::window::Color;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindowBuilder};
use wtboard_core::store::settings::{SettingsStore, Theme, THEME_KEY};

/// `get_settings` 응답
#[derive(Serialize)]
pub struct SettingsDto {
    theme: Theme,
}

/// `theme-changed` event
#[derive(Clone, Serialize)]
struct ThemeChanged {
    theme: Theme,
}

/// 저장된 theme. 읽지 못하면 dark
pub fn current(settings: &SettingsStore) -> Theme {
    Theme::from_setting(settings.get(THEME_KEY).ok().flatten().as_deref())
}

/// 문서보다 먼저 돈다. `index.html`의 inline script가 이 값으로 `<html data-theme>`을 정해 첫 화면부터 그 theme으로 그린다
pub fn init_script(theme: Theme) -> String {
    format!("window.__WTBOARD_THEME__ = \"{}\";", theme.as_str())
}

/// 새 창에 theme(title bar, 뜰 때 배경색, 첫 화면)을 입힌다
pub fn style<'a, R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'a, R, M>,
    theme: Theme,
) -> WebviewWindowBuilder<'a, R, M> {
    builder.theme(Some(native(theme))).background_color(background(theme)).initialization_script(init_script(theme))
}

/// 열린 창 모두에 theme을 바꾸고 화면에 알린다
pub fn apply(app: &AppHandle, theme: Theme) {
    for window in app.webview_windows().values() {
        let _ = window.set_theme(Some(native(theme)));
        let _ = window.set_background_color(Some(background(theme)));
    }
    let _ = app.emit("theme-changed", ThemeChanged { theme });
}

fn native(theme: Theme) -> tauri::Theme {
    match theme {
        Theme::Dark => tauri::Theme::Dark,
        Theme::Light => tauri::Theme::Light,
    }
}

fn background(theme: Theme) -> Color {
    let (r, g, b) = theme.background();
    Color(r, g, b, 0xff)
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Result<SettingsDto, String> {
    Ok(SettingsDto { theme: current(&app.state::<crate::AppState>().settings) })
}

/// 저장하고 모든 창에 적용한다
#[tauri::command]
pub fn set_theme(app: AppHandle, theme: Theme) -> Result<(), String> {
    app.state::<crate::AppState>().settings.set(THEME_KEY, theme.as_str()).map_err(|e| e.to_string())?;
    apply(&app, theme);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_script_sets_global() {
        assert_eq!(init_script(Theme::Light), r#"window.__WTBOARD_THEME__ = "light";"#);
    }
}
