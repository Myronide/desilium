use super::*;
use crate::resize::Edges;

fn sz(w: i32, h: i32) -> Size {
    Size { w, h }
}

fn wm() -> WindowManager {
    let mut wm = WindowManager::new(Rect::new(0, 0, 1000, 800));
    wm.set_work_area(Rect::new(0, 0, 1000, 760));
    wm
}

/// Opens a 200x100 window and moves it to (100, 100).
fn placed(wm: &mut WindowManager) -> WindowId {
    let id = wm.open_window(sz(200, 100));
    wm.move_to(id, Point { x: 100, y: 100 });
    id
}

fn left_edge() -> Edges {
    Edges {
        left: true,
        ..Edges::default()
    }
}

#[test]
fn maximize_fills_work_area_and_unmaximize_restores_exact_rect() {
    let mut wm = wm();
    let id = placed(&mut wm);
    assert!(wm.maximize(id));
    assert_eq!(wm.rect(id), Some(Rect::new(0, 0, 1000, 760)));
    assert_eq!(wm.window_state(id), Some(WindowState::Maximized));
    assert_eq!(wm.restore_rect(id), Some(Rect::new(100, 100, 200, 100)));
    assert!(wm.unmaximize(id));
    assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 200, 100)));
    assert_eq!(wm.window_state(id), Some(WindowState::Normal));
    assert_eq!(wm.restore_rect(id), None);
}

#[test]
fn unknown_window_state_calls_return_false_or_none() {
    let mut wm = wm();
    let stray = WindowManager::new(Rect::new(0, 0, 1, 1)).open_window(sz(1, 1));
    assert!(!wm.maximize(stray));
    assert!(!wm.unmaximize(stray));
    assert!(!wm.toggle_maximize(stray));
    assert!(!wm.set_fullscreen(stray, true));
    assert!(!wm.minimize(stray));
    assert!(!wm.restore(stray));
    assert_eq!(wm.window_state(stray), None);
    assert_eq!(wm.restore_rect(stray), None);
}

#[test]
fn toggle_maximize_flips_state() {
    let mut wm = wm();
    let id = placed(&mut wm);
    assert!(wm.toggle_maximize(id));
    assert_eq!(wm.window_state(id), Some(WindowState::Maximized));
    assert!(wm.toggle_maximize(id));
    assert_eq!(wm.window_state(id), Some(WindowState::Normal));
    assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 200, 100)));
}

#[test]
fn fullscreen_covers_area_and_restores_on_exit() {
    let mut wm = wm();
    let id = placed(&mut wm);
    assert!(wm.set_fullscreen(id, true));
    assert_eq!(wm.rect(id), Some(Rect::new(0, 0, 1000, 800)));
    assert_eq!(wm.window_state(id), Some(WindowState::Fullscreen));
    assert!(wm.set_fullscreen(id, false));
    assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 200, 100)));
}

#[test]
fn maximize_then_fullscreen_keeps_original_restore_rect() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.maximize(id);
    wm.set_fullscreen(id, true);
    assert_eq!(wm.restore_rect(id), Some(Rect::new(100, 100, 200, 100)));
    wm.set_fullscreen(id, false);
    assert_eq!(wm.window_state(id), Some(WindowState::Normal));
    assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 200, 100)));
}

#[test]
fn set_fullscreen_false_leaves_maximized_window_alone() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.maximize(id);
    assert!(wm.set_fullscreen(id, false));
    assert_eq!(wm.window_state(id), Some(WindowState::Maximized));
}

#[test]
fn maximized_window_ignores_resize_and_commit_does_not_reanchor() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.maximize(id);
    assert_eq!(wm.begin_resize(id, left_edge()), None);
    assert!(wm.on_commit(id, sz(500, 500)));
    assert_eq!(wm.rect(id), Some(Rect::new(0, 0, 500, 500)));
}

#[test]
fn maximize_during_resize_cancels_the_resize() {
    let mut wm = wm();
    let id = placed(&mut wm);
    assert!(wm.begin_resize(id, left_edge()).is_some());
    wm.maximize(id);
    wm.unmaximize(id);
    assert!(wm.on_commit(id, sz(150, 100)));
    assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 150, 100)));
}

#[test]
fn move_to_on_maximized_window_restores_size_first() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.maximize(id);
    assert!(wm.move_to(id, Point { x: 300, y: 300 }));
    assert_eq!(wm.rect(id), Some(Rect::new(300, 300, 200, 100)));
    assert_eq!(wm.window_state(id), Some(WindowState::Normal));
}

#[test]
fn begin_move_keeps_pointer_at_same_relative_x() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.maximize(id);
    // Halfway across the maximized 1000px width.
    let loc = wm.begin_move(id, Point { x: 500, y: 380 });
    assert_eq!(loc, Some(Point { x: 400, y: 281 }));
    assert_eq!(wm.rect(id), Some(Rect::new(400, 281, 200, 100)));
    assert_eq!(wm.window_state(id), Some(WindowState::Normal));
}

#[test]
fn begin_move_on_snapped_window_scales_relative_x() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.snap_window(id, crate::SnapZone::Right);
    // Snapped rect is (500, 0, 500, 760); the pointer sits mid-width, so it stays mid-width.
    let loc = wm.begin_move(id, Point { x: 750, y: 10 });
    assert_eq!(loc, Some(Point { x: 650, y: 0 }));
    assert_eq!(wm.rect(id), Some(Rect::new(650, 0, 200, 100)));
}

#[test]
fn begin_move_on_floating_window_returns_current_loc() {
    let mut wm = wm();
    let id = placed(&mut wm);
    assert_eq!(
        wm.begin_move(id, Point { x: 5, y: 5 }),
        Some(Point { x: 100, y: 100 })
    );
    assert_eq!(wm.rect(id), Some(Rect::new(100, 100, 200, 100)));
}

#[test]
fn begin_move_rejects_minimized_and_unknown_windows() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.minimize(id);
    assert_eq!(wm.begin_move(id, Point::default()), None);
    let stray = WindowManager::new(Rect::new(0, 0, 1, 1)).open_window(sz(1, 1));
    assert_eq!(wm.begin_move(stray, Point::default()), None);
}

#[test]
fn minimize_hides_window_from_placements_and_pointer() {
    let mut wm = wm();
    let id = placed(&mut wm);
    assert!(wm.minimize(id));
    assert!(wm.placements().is_empty());
    assert_eq!(wm.window_at(Point { x: 150, y: 150 }), None);
    assert_eq!(wm.minimized_windows(), vec![id]);
    assert_eq!(wm.window_state(id), Some(WindowState::Minimized));
}

#[test]
fn minimize_moves_focus_to_topmost_visible_window() {
    let mut wm = wm();
    let bottom = wm.open_window(sz(10, 10));
    let middle = wm.open_window(sz(10, 10));
    let top = wm.open_window(sz(10, 10));
    wm.minimize(top);
    assert_eq!(wm.focused(), Some(middle));
    wm.minimize(middle);
    assert_eq!(wm.focused(), Some(bottom));
    wm.minimize(bottom);
    assert_eq!(wm.focused(), None);
}

#[test]
fn minimizing_unfocused_window_keeps_focus() {
    let mut wm = wm();
    let first = wm.open_window(sz(10, 10));
    let second = wm.open_window(sz(10, 10));
    wm.minimize(first);
    assert_eq!(wm.focused(), Some(second));
}

#[test]
fn restore_unminimizes_raises_and_focuses() {
    let mut wm = wm();
    let first = wm.open_window(sz(10, 10));
    let second = wm.open_window(sz(10, 10));
    wm.minimize(first);
    assert!(wm.restore(first));
    assert_eq!(wm.focused(), Some(first));
    assert_eq!(wm.placements().last().map(|p| p.id), Some(first));
    assert!(wm.minimized_windows().is_empty());
    assert_eq!(wm.placements().len(), 2);
    assert_eq!(wm.focused(), Some(first));
    assert_ne!(wm.focused(), Some(second));
}

#[test]
fn restore_keeps_maximized_state() {
    let mut wm = wm();
    let id = placed(&mut wm);
    wm.maximize(id);
    wm.minimize(id);
    wm.restore(id);
    assert_eq!(wm.window_state(id), Some(WindowState::Maximized));
    assert_eq!(wm.rect(id), Some(Rect::new(0, 0, 1000, 760)));
}

#[test]
fn focus_refuses_minimized_window() {
    let mut wm = wm();
    let id = wm.open_window(sz(10, 10));
    wm.minimize(id);
    assert!(!wm.focus(id));
    assert_ne!(wm.focused(), Some(id));
}

#[test]
fn work_area_drives_maximize_and_fullscreen_targets() {
    let mut wm = WindowManager::new(Rect::new(0, 0, 1000, 800));
    let id = placed(&mut wm);
    wm.set_work_area(Rect::new(0, 30, 1000, 770));
    wm.maximize(id);
    assert_eq!(wm.rect(id), Some(Rect::new(0, 30, 1000, 770)));
    wm.set_fullscreen(id, true);
    assert_eq!(wm.rect(id), Some(Rect::new(0, 0, 1000, 800)));
}

#[test]
fn set_area_keeps_work_area_in_sync_only_when_equal() {
    let mut wm = WindowManager::new(Rect::new(0, 0, 1000, 800));
    wm.set_area(Rect::new(0, 0, 1920, 1080));
    assert_eq!(wm.work_area(), Rect::new(0, 0, 1920, 1080));
    wm.set_work_area(Rect::new(0, 0, 1920, 1000));
    wm.set_area(Rect::new(0, 0, 800, 600));
    assert_eq!(wm.work_area(), Rect::new(0, 0, 1920, 1000));
}

#[test]
fn area_change_refits_maximized_but_not_floating_windows() {
    let mut wm = wm();
    let floating = placed(&mut wm);
    let maximized = wm.open_window(sz(10, 10));
    wm.maximize(maximized);
    wm.set_work_area(Rect::new(0, 0, 500, 500));
    assert_eq!(wm.rect(maximized), Some(Rect::new(0, 0, 500, 500)));
    assert_eq!(wm.rect(floating), Some(Rect::new(100, 100, 200, 100)));
}
