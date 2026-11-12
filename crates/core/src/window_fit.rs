//! 저장된 창 rect를 지금 연결된 모니터에 맞춘다. 좌표는 모두 logical point.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// 모니터의 work area (logical point)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Screen {
    pub area: Rect,
    pub primary: bool,
}

/// position이 None이면 OS 기본 위치
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub width: f64,
    pub height: f64,
    pub position: Option<(f64, f64)>,
}

pub const DEFAULT_SIZE: (f64, f64) = (1400.0, 900.0);
/// 저장된 가로나 세로가 이보다 작으면 저장 값이 없는 것으로 본다
pub const MIN_SAVED: f64 = 200.0;
/// 이보다 작은 차이는 사용자가 옮긴 것으로 보지 않는다 (좌표 반올림 오차)
const MOVE_TOLERANCE: f64 = 1.0;

/// `window_main` setting 값 → rect. 깨졌거나 너무 작으면 None
pub fn decode(value: Option<&str>) -> Option<Rect> {
    let rect: Rect = serde_json::from_str(value?).ok()?;
    let finite = [rect.x, rect.y, rect.width, rect.height].iter().all(|v| v.is_finite());
    (finite && rect.width >= MIN_SAVED && rect.height >= MIN_SAVED).then_some(rect)
}

pub fn encode(rect: &Rect) -> String {
    serde_json::to_string(rect).expect("Rect는 항상 JSON이 된다")
}

/// 저장된 rect(없으면 기본 크기)를 화면 안에 들어오게 한 크기와 위치
pub fn fit(saved: Option<Rect>, screens: &[Screen]) -> Placement {
    let primary = screens.iter().find(|s| s.primary).or(screens.first());
    let Some(saved) = saved else {
        let (width, height) = DEFAULT_SIZE;
        return match primary {
            Some(s) => Placement {
                width: width.min(s.area.width),
                height: height.min(s.area.height),
                position: None,
            },
            None => Placement { width, height, position: None },
        };
    };
    let most_overlap = screens
        .iter()
        .map(|s| (s, overlap(&saved, &s.area)))
        .filter(|(_, o)| *o > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(s, _)| s);
    let Some(screen) = most_overlap.or(primary) else {
        return Placement {
            width: saved.width,
            height: saved.height,
            position: Some((saved.x, saved.y)),
        };
    };
    let area = screen.area;
    let width = saved.width.min(area.width);
    let height = saved.height.min(area.height);
    Placement {
        width,
        height,
        position: Some((
            saved.x.min(area.x + area.width - width).max(area.x),
            saved.y.min(area.y + area.height - height).max(area.y),
        )),
    }
}

/// 복원 뒤 창 rect(`baseline`)에서 `current`가 사용자가 움직이거나 크기를 바꾼 것인가
pub fn moved_by_user(baseline: &Rect, current: &Rect) -> bool {
    [
        (baseline.x, current.x),
        (baseline.y, current.y),
        (baseline.width, current.width),
        (baseline.height, current.height),
    ]
    .iter()
    .any(|(a, b)| (a - b).abs() > MOVE_TOLERANCE)
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}
