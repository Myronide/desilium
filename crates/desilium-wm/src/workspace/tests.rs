use super::*;

fn id(raw: u64) -> WindowId {
    WindowId::new(raw)
}

fn edges_left() -> Edges {
    Edges {
        left: true,
        ..Edges::default()
    }
}

#[test]
fn open_places_on_top_and_focuses() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(0, 0, 10, 10));
    ws.open(id(2), Rect::new(0, 0, 10, 10));
    assert_eq!(ws.focused(), Some(id(2)));
    assert_eq!(ws.placements().last().map(|p| p.0), Some(id(2)));
}

#[test]
fn focus_raises_window() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(0, 0, 10, 10));
    ws.open(id(2), Rect::new(0, 0, 10, 10));
    assert!(ws.focus(id(1)));
    assert_eq!(ws.placements().last().map(|p| p.0), Some(id(1)));
    assert_eq!(ws.focused(), Some(id(1)));
}

#[test]
fn remove_focused_falls_back_to_topmost_remaining() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(0, 0, 10, 10));
    ws.open(id(2), Rect::new(0, 0, 10, 10));
    ws.open(id(3), Rect::new(0, 0, 10, 10));
    ws.focus(id(1));
    assert!(ws.remove(id(1)));
    assert_eq!(ws.focused(), Some(id(3)));
}

#[test]
fn remove_last_window_clears_focus() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(0, 0, 10, 10));
    assert!(ws.remove(id(1)));
    assert_eq!(ws.focused(), None);
    assert!(ws.is_empty());
}

#[test]
fn remove_unknown_returns_false() {
    let mut ws = Workspace::default();
    assert!(!ws.remove(id(9)));
}

#[test]
fn window_at_returns_topmost_overlap() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(0, 0, 100, 100));
    ws.open(id(2), Rect::new(50, 50, 100, 100));
    assert_eq!(ws.window_at(Point { x: 60, y: 60 }), Some(id(2)));
    assert_eq!(ws.window_at(Point { x: 10, y: 10 }), Some(id(1)));
    assert_eq!(ws.window_at(Point { x: 500, y: 500 }), None);
}

#[test]
fn commit_during_active_resize_anchors_location() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(100, 100, 200, 200));
    ws.begin_resize(id(1), edges_left());
    assert!(ws.on_commit(id(1), Size { w: 150, h: 200 }));
    assert_eq!(ws.rect(id(1)), Some(Rect::new(150, 100, 150, 200)));
}

#[test]
fn settling_commit_anchors_once_then_returns_to_idle() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(100, 100, 200, 200));
    ws.begin_resize(id(1), edges_left());
    ws.end_resize(id(1));
    assert!(ws.on_commit(id(1), Size { w: 150, h: 200 }));
    assert_eq!(ws.rect(id(1)), Some(Rect::new(150, 100, 150, 200)));
    assert!(ws.on_commit(id(1), Size { w: 120, h: 200 }));
    assert_eq!(ws.rect(id(1)), Some(Rect::new(150, 100, 120, 200)));
}

#[test]
fn commit_while_idle_only_sets_size() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(100, 100, 200, 200));
    assert!(ws.on_commit(id(1), Size { w: 50, h: 50 }));
    assert_eq!(ws.rect(id(1)), Some(Rect::new(100, 100, 50, 50)));
    assert!(!ws.on_commit(id(1), Size { w: 50, h: 50 }));
}

#[test]
fn commit_unknown_returns_false() {
    let mut ws = Workspace::default();
    assert!(!ws.on_commit(id(4), Size { w: 1, h: 1 }));
}

#[test]
fn move_to_unknown_returns_false() {
    let mut ws = Workspace::default();
    assert!(!ws.move_to(id(4), Point::default()));
}

#[test]
fn begin_resize_unknown_returns_none() {
    let mut ws = Workspace::default();
    assert_eq!(ws.begin_resize(id(4), edges_left()), None);
}

#[test]
fn minimized_window_is_hidden_and_cannot_take_focus() {
    let mut ws = Workspace::default();
    ws.open(id(1), Rect::new(0, 0, 100, 100));
    ws.open(id(2), Rect::new(0, 0, 100, 100));
    if let Some(shape) = ws.shape_mut(id(2)) {
        shape.minimized = true;
    }
    assert_eq!(ws.placements().len(), 1);
    assert_eq!(ws.window_at(Point { x: 1, y: 1 }), Some(id(1)));
    assert!(!ws.focus(id(2)));
}

#[test]
fn detach_then_attach_keeps_rect_and_shape() {
    let mut source = Workspace::default();
    source.open(id(1), Rect::new(7, 8, 9, 10));
    let window = source.detach(id(1)).expect("window exists");
    let mut target = Workspace::default();
    target.attach(window);
    assert_eq!(target.rect(id(1)), Some(Rect::new(7, 8, 9, 10)));
    assert_eq!(target.focused(), Some(id(1)));
    assert!(source.is_empty());
}
