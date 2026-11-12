use wtboard_core::window_fit::{decode, encode, fit, moved_by_user, save_action, Placement, Rect, SaveAction, Screen, WindowState};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect { x, y, width, height }
}

fn mac() -> Screen {
    Screen { area: rect(0.0, 25.0, 1440.0, 875.0), primary: true }
}

fn ext() -> Screen {
    Screen { area: rect(1440.0, 0.0, 2560.0, 1440.0), primary: false }
}

fn placed(width: f64, height: f64, position: Option<(f64, f64)>) -> Placement {
    Placement { width, height, position }
}

#[test]
fn fit_default_when_nothing_saved() {
    let only_ext = Screen { primary: true, ..ext() };
    assert_eq!(fit(None, &[only_ext]), placed(1400.0, 900.0, None));
}

#[test]
fn fit_default_shrinks_to_screen() {
    assert_eq!(fit(None, &[mac()]), placed(1400.0, 875.0, None));
}

#[test]
fn fit_keeps_rect_inside_screen() {
    let saved = rect(100.0, 100.0, 1200.0, 800.0);
    assert_eq!(fit(Some(saved), &[mac()]), placed(1200.0, 800.0, Some((100.0, 100.0))));
}

#[test]
fn fit_shrinks_both_axes() {
    let saved = rect(1440.0, 0.0, 2560.0, 1440.0);
    assert_eq!(fit(Some(saved), &[mac()]), placed(1440.0, 875.0, Some((0.0, 25.0))));
}

#[test]
fn fit_shrinks_width_only() {
    let saved = rect(200.0, 100.0, 2000.0, 600.0);
    assert_eq!(fit(Some(saved), &[mac()]), placed(1440.0, 600.0, Some((0.0, 100.0))));
}

#[test]
fn fit_moves_to_primary_when_monitor_is_gone() {
    let saved = rect(-2460.0, 200.0, 1800.0, 1000.0);
    assert_eq!(fit(Some(saved), &[mac()]), placed(1440.0, 875.0, Some((0.0, 25.0))));
}

#[test]
fn fit_pulls_partly_offscreen_rect_in() {
    let saved = rect(1000.0, 500.0, 800.0, 600.0);
    assert_eq!(fit(Some(saved), &[mac()]), placed(800.0, 600.0, Some((640.0, 300.0))));
}

#[test]
fn fit_picks_screen_with_most_overlap() {
    let saved = rect(1500.0, 100.0, 2000.0, 1200.0);
    assert_eq!(
        fit(Some(saved), &[mac(), ext()]),
        placed(2000.0, 1200.0, Some((1500.0, 100.0)))
    );
}

#[test]
fn fit_straddling_rect_goes_to_screen_with_more_area_not_primary() {
    // mac 위로 240pt, ext 위로 1200pt 걸친다. ext 쪽 넓이가 훨씬 크다
    let saved = rect(1200.0, 100.0, 1440.0, 800.0);
    assert_eq!(fit(Some(saved), &[mac(), ext()]), placed(1440.0, 800.0, Some((1440.0, 100.0))));
}

#[test]
fn fit_without_screens_returns_saved() {
    let saved = rect(10.0, 10.0, 900.0, 700.0);
    assert_eq!(fit(Some(saved), &[]), placed(900.0, 700.0, Some((10.0, 10.0))));
}

#[test]
fn decode_roundtrip() {
    let r = rect(-12.5, 40.0, 1280.0, 777.25);
    assert_eq!(decode(Some(&encode(&r))), Some(r));
}

#[test]
fn decode_rejects_garbage_and_tiny() {
    assert_eq!(decode(None), None);
    assert_eq!(decode(Some("x")), None);
    assert_eq!(decode(Some(r#"{"x":0,"y":0,"width":150,"height":900}"#)), None);
    assert_eq!(decode(Some(r#"{"x":0,"y":0,"width":900,"height":199}"#)), None);
    assert_eq!(decode(Some(r#"{"x":0,"y":0,"width":null,"height":900}"#)), None);
}

#[test]
fn moved_by_user_ignores_subpoint_noise() {
    let base = rect(100.0, 100.0, 1200.0, 800.0);
    assert!(!moved_by_user(&base, &base));
    assert!(!moved_by_user(&base, &rect(100.0, 100.5, 1200.0, 800.0)));
    assert!(moved_by_user(&base, &rect(100.0, 100.0, 1210.0, 800.0)));
}

const SHOWN: WindowState = WindowState { fullscreen: false, minimized: false, visible: true };

#[test]
fn save_action_skips_when_rect_is_not_users_choice() {
    let base = rect(100.0, 100.0, 1200.0, 800.0);
    let moved = rect(300.0, 100.0, 1200.0, 800.0);
    for state in [
        WindowState { fullscreen: true, ..SHOWN },
        WindowState { minimized: true, ..SHOWN },
        WindowState { visible: false, ..SHOWN },
    ] {
        assert_eq!(save_action(state, false, &base, &moved), SaveAction::Nothing, "{state:?}");
        assert_eq!(save_action(state, true, &base, &moved), SaveAction::Nothing, "{state:?}");
    }
}

#[test]
fn save_action_rebaselines_when_screens_changed() {
    let base = rect(1500.0, 100.0, 2000.0, 1200.0);
    // 외부 모니터를 뽑아 OS가 줄이고 옮긴 rect
    let shrunk = rect(0.0, 25.0, 1440.0, 875.0);
    assert_eq!(save_action(SHOWN, true, &base, &shrunk), SaveAction::Rebaseline);
    assert_eq!(save_action(SHOWN, true, &base, &base), SaveAction::Rebaseline);
}

#[test]
fn save_action_saves_only_users_move_on_same_screens() {
    let base = rect(100.0, 100.0, 1200.0, 800.0);
    assert_eq!(save_action(SHOWN, false, &base, &rect(300.0, 100.0, 1200.0, 800.0)), SaveAction::Save);
    assert_eq!(save_action(SHOWN, false, &base, &rect(100.0, 100.5, 1200.0, 800.0)), SaveAction::Nothing);
}
