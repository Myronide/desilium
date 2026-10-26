use super::*;

fn big_area() -> Rect {
    Rect::new(0, 0, 1000, 800)
}

fn sz(w: i32, h: i32) -> Size {
    Size { w, h }
}

#[test]
fn first_window_cascades_from_inset() {
    let mut wm = WindowManager::new(big_area());
    let id = wm.open_window(sz(100, 100));
    assert_eq!(wm.rect(id), Some(Rect::new(48, 48, 100, 100)));
}

#[test]
fn cascade_wraps_after_ten_slots() {
    let mut wm = WindowManager::new(big_area());
    let first = wm.open_window(sz(10, 10));
    for _ in 0..9 {
        wm.open_window(sz(10, 10));
    }
    let eleventh = wm.open_window(sz(10, 10));
    assert_eq!(
        wm.rect(first).map(|r| r.loc),
        wm.rect(eleventh).map(|r| r.loc)
    );
}

#[test]
fn cascade_steps_by_32_per_index() {
    let mut wm = WindowManager::new(big_area());
    wm.open_window(sz(10, 10));
    let second = wm.open_window(sz(10, 10));
    assert_eq!(wm.rect(second).map(|r| r.loc), Some(Point { x: 80, y: 80 }));
}

#[test]
fn cascade_index_counts_opens_not_current_windows() {
    let mut wm = WindowManager::new(big_area());
    let first = wm.open_window(sz(10, 10));
    wm.close_window(first);
    let second = wm.open_window(sz(10, 10));
    assert_eq!(wm.rect(second).map(|r| r.loc), Some(Point { x: 80, y: 80 }));
}

#[test]
fn cascade_clamps_into_small_area() {
    let mut wm = WindowManager::new(Rect::new(0, 0, 100, 100));
    let id = wm.open_window(sz(80, 80));
    assert_eq!(wm.rect(id).map(|r| r.loc), Some(Point { x: 20, y: 20 }));
}

#[test]
fn oversized_window_pins_to_area_origin() {
    let mut wm = WindowManager::new(Rect::new(10, 10, 100, 100));
    let id = wm.open_window(sz(300, 300));
    assert_eq!(wm.rect(id).map(|r| r.loc), Some(Point { x: 10, y: 10 }));
}

#[test]
fn ids_are_monotonic_from_one() {
    let mut wm = WindowManager::new(big_area());
    let first = wm.open_window(sz(1, 1));
    let second = wm.open_window(sz(1, 1));
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 2);
}

#[test]
fn open_places_on_top_and_focuses() {
    let mut wm = WindowManager::new(big_area());
    let first = wm.open_window(sz(10, 10));
    let second = wm.open_window(sz(10, 10));
    assert_eq!(wm.focused(), Some(second));
    assert_eq!(wm.placements().last().map(|p| p.id), Some(second));
    assert_eq!(wm.placements().first().map(|p| p.id), Some(first));
}

#[test]
fn focus_raises_and_focuses() {
    let mut wm = WindowManager::new(big_area());
    let first = wm.open_window(sz(10, 10));
    wm.open_window(sz(10, 10));
    assert!(wm.focus(first));
    assert_eq!(wm.focused(), Some(first));
    assert_eq!(wm.placements().last().map(|p| p.id), Some(first));
}

#[test]
fn focus_unknown_returns_false() {
    let mut wm = WindowManager::new(big_area());
    let other = WindowManager::new(big_area()).open_window(sz(1, 1));
    assert!(!wm.focus(other));
}

#[test]
fn clear_focus_removes_focus() {
    let mut wm = WindowManager::new(big_area());
    wm.open_window(sz(10, 10));
    wm.clear_focus();
    assert_eq!(wm.focused(), None);
}

#[test]
fn closing_focused_falls_back_to_topmost_remaining() {
    let mut wm = WindowManager::new(big_area());
    let bottom = wm.open_window(sz(10, 10));
    let middle = wm.open_window(sz(10, 10));
    let top = wm.open_window(sz(10, 10));
    wm.focus(top);
    assert!(wm.close_window(top));
    assert_eq!(wm.focused(), Some(middle));
    assert!(wm.close_window(middle));
    assert_eq!(wm.focused(), Some(bottom));
    assert!(wm.close_window(bottom));
    assert_eq!(wm.focused(), None);
    assert!(wm.is_empty());
}

#[test]
fn closing_unknown_returns_false() {
    let mut wm = WindowManager::new(big_area());
    let stray = WindowManager::new(big_area()).open_window(sz(1, 1));
    assert!(!wm.close_window(stray));
}

#[test]
fn window_at_returns_topmost_overlap() {
    let mut wm = WindowManager::new(big_area());
    let below = wm.open_window(sz(200, 200));
    let above = wm.open_window(sz(200, 200));
    assert!(wm.move_to(below, Point { x: 0, y: 0 }));
    assert!(wm.move_to(above, Point { x: 100, y: 100 }));
    assert_eq!(wm.window_at(Point { x: 150, y: 150 }), Some(above));
    assert_eq!(wm.window_at(Point { x: 50, y: 50 }), Some(below));
    assert_eq!(wm.window_at(Point { x: 900, y: 700 }), None);
}

#[test]
fn move_to_updates_location_only() {
    let mut wm = WindowManager::new(big_area());
    let id = wm.open_window(sz(30, 40));
    assert!(wm.move_to(id, Point { x: 5, y: 6 }));
    assert_eq!(wm.rect(id), Some(Rect::new(5, 6, 30, 40)));
}

#[test]
fn move_to_unknown_returns_false() {
    let mut wm = WindowManager::new(big_area());
    let stray = WindowManager::new(big_area()).open_window(sz(1, 1));
    assert!(!wm.move_to(stray, Point::default()));
}

#[test]
fn set_area_does_not_move_existing_windows() {
    let mut wm = WindowManager::new(big_area());
    let id = wm.open_window(sz(100, 100));
    let before = wm.rect(id);
    wm.set_area(Rect::new(0, 0, 50, 50));
    assert_eq!(wm.rect(id), before);
    assert_eq!(wm.area(), Rect::new(0, 0, 50, 50));
}

#[test]
fn resize_flow_through_manager_anchors_left_edge() {
    let mut wm = WindowManager::new(big_area());
    let id = wm.open_window(sz(200, 100));
    wm.move_to(id, Point { x: 100, y: 100 });
    let left = Edges {
        left: true,
        ..Edges::default()
    };
    assert_eq!(
        wm.begin_resize(id, left),
        Some(Rect::new(100, 100, 200, 100))
    );
    assert!(wm.on_commit(id, sz(150, 100)));
    assert_eq!(wm.rect(id), Some(Rect::new(150, 100, 150, 100)));
    wm.end_resize(id);
    assert!(wm.on_commit(id, sz(160, 100)));
    assert_eq!(wm.rect(id), Some(Rect::new(140, 100, 160, 100)));
}
