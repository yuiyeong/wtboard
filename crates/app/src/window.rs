//! main 창: 저장된 크기·위치로 만들고, 사용자가 옮기거나 크기를 바꾸면 저장한다
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Manager, Monitor, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use wtboard_core::store::settings::{SettingsStore, WINDOW_KEY};
use wtboard_core::window_fit::{decode, encode, fit, save_action, Rect, SaveAction, Screen, WindowState};

pub const MAIN: &str = "main";
/// 옮기거나 크기를 바꾸는 동안은 저장하지 않는다. 마지막 event 뒤 이만큼 지나면 저장한다
const SAVE_DELAY: Duration = Duration::from_millis(500);

/// physical work area → logical Screen
pub fn to_screen(x: i32, y: i32, width: u32, height: u32, scale: f64, primary: bool) -> Screen {
    Screen {
        area: Rect { x: x as f64 / scale, y: y as f64 / scale, width: width as f64 / scale, height: height as f64 / scale },
        primary,
    }
}

/// 저장된 rect·theme으로 main 창을 만든다. 지금 화면에 들어오지 않으면 줄이거나 옮긴다
pub fn create_main(app: &AppHandle, settings: &SettingsStore) -> tauri::Result<WebviewWindow> {
    let saved = settings.get(WINDOW_KEY).ok().flatten();
    let placement = fit(decode(saved.as_deref()), &screens(app));
    let mut builder = WebviewWindowBuilder::new(app, MAIN, WebviewUrl::default())
        .title("wtboard")
        .inner_size(placement.width, placement.height);
    if let Some((x, y)) = placement.position {
        builder = builder.position(x, y);
    }
    crate::theme::style(builder, crate::theme::current(settings)).build()
}

/// 숨었거나 최소화된 창을 보이고 앞으로 가져온다
pub fn bring_to_front(window: &WebviewWindow) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_focus();
}

fn screens(app: &AppHandle) -> Vec<Screen> {
    let primary = app.primary_monitor().ok().flatten();
    let is_primary = |m: &Monitor| primary.as_ref().is_some_and(|p| p.name() == m.name() && p.position() == m.position());
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let area = m.work_area();
            to_screen(area.position.x, area.position.y, area.size.width, area.size.height, m.scale_factor(), is_primary(m))
        })
        .collect()
}

/// 창의 지금 rect (logical). 위치는 title bar를 포함한 바깥, 크기는 안쪽
pub fn rect(window: &WebviewWindow) -> Option<Rect> {
    let scale = window.scale_factor().ok()?;
    let position = window.outer_position().ok()?.to_logical::<f64>(scale);
    let size = window.inner_size().ok()?.to_logical::<f64>(scale);
    Some(Rect { x: position.x, y: position.y, width: size.width, height: size.height })
}

/// 창 크기·위치 저장. 복원 직후 rect(`baseline`)에서 달라졌을 때만 저장한다.
/// 앱이 화면에 맞춰 줄이거나 옮긴 rect, 모니터 구성이 바뀌어 OS가 바꾼 rect는 저장하지 않는다
pub struct GeometrySaver {
    tx: mpsc::Sender<()>,
    saver: Arc<Saver>,
}

struct Saver {
    app: AppHandle,
    /// 비교와 저장을 이 lock 안에서 해서 두 저장이 엇갈리지 않게 한다
    baseline: Mutex<Baseline>,
}

/// 마지막으로 저장한(또는 복원한) rect와 그때의 모니터 구성
struct Baseline {
    rect: Rect,
    screens: Vec<Screen>,
}

impl GeometrySaver {
    pub fn spawn(app: AppHandle, baseline: Rect) -> Self {
        let baseline = Baseline { rect: baseline, screens: screens(&app) };
        let saver = Arc::new(Saver { app, baseline: Mutex::new(baseline) });
        let (tx, rx) = mpsc::channel();
        let worker = saver.clone();
        std::thread::spawn(move || {
            // 요청이 오면 SAVE_DELAY 동안 조용해질 때까지 기다린 뒤 저장한다
            while rx.recv().is_ok() {
                while rx.recv_timeout(SAVE_DELAY).is_ok() {}
                worker.save();
            }
        });
        Self { tx, saver }
    }

    /// `Moved`·`Resized`마다 부른다
    pub fn request(&self) {
        let _ = self.tx.send(());
    }

    /// 종료나 창 닫기 직전에 부른다. 기다리지 않고 바로 저장한다
    pub fn flush(&self) {
        self.saver.save();
    }
}

impl Saver {
    fn save(&self) {
        let Some(window) = self.app.get_webview_window(MAIN) else { return };
        // 모를 때는 사용자가 정한 창 크기가 아닌 것으로 본다
        let state = WindowState {
            fullscreen: window.is_fullscreen().unwrap_or(true),
            minimized: window.is_minimized().unwrap_or(true),
            visible: window.is_visible().unwrap_or(false),
        };
        let Some(current) = rect(&window) else { return };
        let now = screens(&self.app);
        let mut baseline = self.baseline.lock().unwrap();
        // 모니터를 뽑거나 꽂으면 OS가 창을 줄이거나 옮긴다. 그 rect를 저장하면 다음 실행이 작게 뜬다
        match save_action(state, now != baseline.screens, &baseline.rect, &current) {
            SaveAction::Nothing => {}
            SaveAction::Rebaseline => *baseline = Baseline { rect: current, screens: now },
            SaveAction::Save => {
                let settings = &self.app.state::<crate::AppState>().settings;
                match settings.set(WINDOW_KEY, &encode(&current)) {
                    Ok(()) => baseline.rect = current,
                    Err(e) => eprintln!("[wtboard] 창 크기·위치를 저장할 수 없음: {e}"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_is_logical_work_area() {
        assert_eq!(
            to_screen(0, 50, 2880, 1750, 2.0, true),
            Screen { area: Rect { x: 0.0, y: 25.0, width: 1440.0, height: 875.0 }, primary: true }
        );
        assert_eq!(to_screen(-2560, 0, 2560, 1440, 1.0, false).area.x, -2560.0);
    }
}
