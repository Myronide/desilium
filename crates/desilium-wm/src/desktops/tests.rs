use super::*;

fn area() -> Rect {
    Rect::new(0, 0, 1000, 800)
}

fn sz(w: i32, h: i32) -> Size {
    Size { w, h }
}

#[test]
fn count_is_clamped_to_supported_range() {
    assert_eq!(Desktops::new(area(), 0).count(), 1);
    assert_eq!(Desktops::new(area(), 100).count(), MAX_DESKTOPS);
    assert_eq!(Desktops::new(area(), DEFAULT_DESKTOPS).count(), 4);
}

#[test]
fn next_and_prev_wrap_and_return_new_index() {
    let mut desks = Desktops::new(area(), 3);
    assert_eq!(desks.next(), 1);
    assert_eq!(desks.next(), 2);
    assert_eq!(desks.next(), 0);
    assert_eq!(desks.prev(), 2);
    assert_eq!(desks.active(), 2);
}

#[test]
fn switch_to_rejects_out_of_range_index() {
    let mut desks = Desktops::new(area(), 2);
    assert!(desks.switch_to(1));
    assert_eq!(desks.active(), 1);
    assert!(!desks.switch_to(2));
    assert_eq!(desks.active(), 1);
}

#[test]
fn single_desktop_next_and_prev_stay_put() {
    let mut desks = Desktops::new(area(), 1);
    assert_eq!(desks.next(), 0);
    assert_eq!(desks.prev(), 0);
}

#[test]
fn open_window_goes_to_active_desktop() {
    let mut desks = Desktops::new(area(), 2);
    desks.switch_to(1);
    let id = desks.open_window(sz(100, 100));
    assert_eq!(desks.desktop_of(id), Some(1));
    assert!(desks.active_manager().rect(id).is_some());
    assert_eq!(desks.manager(0).and_then(|m| m.rect(id)), None);
}

#[test]
fn ids_are_unique_across_desktops() {
    let mut desks = Desktops::new(area(), 3);
    let mut seen = Vec::new();
    for index in 0..3 {
        desks.switch_to(index);
        seen.push(desks.open_window(sz(10, 10)));
        seen.push(desks.open_window(sz(10, 10)));
    }
    let mut unique = seen.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), seen.len());
}

#[test]
fn move_window_keeps_rect_and_updates_desktop() {
    let mut desks = Desktops::new(area(), 2);
    let id = desks.open_window(sz(120, 80));
    let before = desks.manager(0).and_then(|m| m.rect(id));
    assert!(desks.move_window_to(id, 1));
    assert_eq!(desks.desktop_of(id), Some(1));
    assert_eq!(desks.manager(1).and_then(|m| m.rect(id)), before);
    assert_eq!(desks.manager(0).and_then(|m| m.rect(id)), None);
}

#[test]
fn move_window_fixes_focus_on_both_desktops() {
    let mut desks = Desktops::new(area(), 2);
    let kept = desks.open_window(sz(10, 10));
    let moved = desks.open_window(sz(10, 10));
    assert!(desks.move_window_to(moved, 1));
    assert_eq!(desks.manager(0).and_then(|m| m.focused()), Some(kept));
    assert_eq!(desks.manager(1).and_then(|m| m.focused()), Some(moved));
}

#[test]
fn move_window_to_busy_desktop_keeps_its_focus() {
    let mut desks = Desktops::new(area(), 2);
    let moved = desks.open_window(sz(10, 10));
    desks.switch_to(1);
    let resident = desks.open_window(sz(10, 10));
    assert!(desks.move_window_to(moved, 1));
    assert_eq!(desks.manager(1).and_then(|m| m.focused()), Some(resident));
}

#[test]
fn move_window_rejects_bad_targets_and_unknown_ids() {
    let mut desks = Desktops::new(area(), 2);
    let id = desks.open_window(sz(10, 10));
    assert!(desks.move_window_to(id, 0));
    assert!(!desks.move_window_to(id, 2));
    assert!(!desks.move_window_to(WindowId::new(999), 1));
}

#[test]
fn close_window_works_on_any_desktop() {
    let mut desks = Desktops::new(area(), 2);
    desks.switch_to(1);
    let id = desks.open_window(sz(10, 10));
    desks.switch_to(0);
    assert!(desks.close_window(id));
    assert_eq!(desks.desktop_of(id), None);
    assert!(!desks.close_window(id));
}

#[test]
fn set_count_shrink_moves_windows_to_last_remaining_desktop() {
    let mut desks = Desktops::new(area(), 4);
    desks.switch_to(3);
    let id = desks.open_window(sz(50, 50));
    desks.set_count(2);
    assert_eq!(desks.count(), 2);
    assert_eq!(desks.desktop_of(id), Some(1));
    assert_eq!(desks.active(), 1);
}

#[test]
fn set_count_shrink_keeps_rect() {
    let mut desks = Desktops::new(area(), 3);
    desks.switch_to(2);
    let id = desks.open_window(sz(50, 50));
    let before = desks.manager(2).and_then(|m| m.rect(id));
    desks.set_count(1);
    assert_eq!(desks.manager(0).and_then(|m| m.rect(id)), before);
}

#[test]
fn set_count_grow_adds_empty_desktops() {
    let mut desks = Desktops::new(area(), 1);
    desks.set_count(3);
    assert_eq!(desks.count(), 3);
    assert!(desks.manager(2).is_some_and(|m| m.is_empty()));
    assert!(desks.manager(3).is_none());
}

#[test]
fn set_count_clamps_to_at_least_one() {
    let mut desks = Desktops::new(area(), 3);
    desks.set_count(0);
    assert_eq!(desks.count(), 1);
}

#[test]
fn all_windows_lists_every_desktop_in_order() {
    let mut desks = Desktops::new(area(), 2);
    let first = desks.open_window(sz(10, 10));
    desks.switch_to(1);
    let second = desks.open_window(sz(10, 10));
    assert_eq!(desks.all_windows(), vec![(first, 0), (second, 1)]);
}

#[test]
fn manager_of_mut_returns_owning_desktop() {
    let mut desks = Desktops::new(area(), 2);
    desks.switch_to(1);
    let id = desks.open_window(sz(10, 10));
    let owner = desks.manager_of_mut(id);
    assert!(owner.is_some_and(|m| m.rect(id).is_some()));
    assert!(desks.manager_of_mut(WindowId::new(999)).is_none());
}

#[test]
fn area_changes_reach_every_desktop() {
    let mut desks = Desktops::new(area(), 3);
    let wider = Rect::new(0, 0, 1920, 1080);
    desks.set_area(wider);
    assert_eq!(desks.area(), wider);
    assert_eq!(desks.work_area(), wider);
    assert!(desks.manager(2).is_some_and(|m| m.area() == wider));
}

#[test]
fn work_area_applies_to_new_desktops_and_maximize() {
    let mut desks = Desktops::new(area(), 1);
    desks.set_work_area(Rect::new(0, 30, 1000, 770));
    desks.set_count(2);
    let id = desks.open_window(sz(10, 10));
    desks.switch_to(1);
    let other = desks.open_window(sz(10, 10));
    assert!(
        desks
            .manager_of_mut(other)
            .is_some_and(|m| m.maximize(other))
    );
    assert_eq!(
        desks.manager(1).and_then(|m| m.rect(other)),
        Some(Rect::new(0, 30, 1000, 770))
    );
    assert!(desks.manager(0).is_some_and(|m| m.rect(id).is_some()));
}

#[test]
fn deref_reads_only_the_active_desktop() {
    let mut desks = Desktops::new(area(), 2);
    let id = desks.open_window(sz(10, 10));
    assert_eq!(desks.len(), 1);
    assert_eq!(desks.focused(), Some(id));
    desks.switch_to(1);
    assert!(desks.is_empty());
    assert_eq!(desks.focused(), None);
    assert!(desks.placements().is_empty());
}

#[test]
fn deref_mut_focus_ignores_windows_on_other_desktops() {
    let mut desks = Desktops::new(area(), 2);
    let hidden = desks.open_window(sz(10, 10));
    desks.switch_to(1);
    let shown = desks.open_window(sz(10, 10));
    assert!(!desks.focus(hidden));
    assert_eq!(desks.focused(), Some(shown));
    desks.clear_focus();
    assert_eq!(desks.focused(), None);
    assert_eq!(desks.manager(0).and_then(|m| m.focused()), Some(hidden));
}

#[test]
fn each_desktop_keeps_its_own_focus_across_switches() {
    let mut desks = Desktops::new(area(), 2);
    let first = desks.open_window(sz(10, 10));
    desks.switch_to(1);
    let second = desks.open_window(sz(10, 10));
    desks.switch_to(0);
    assert_eq!(desks.focused(), Some(first));
    desks.switch_to(1);
    assert_eq!(desks.focused(), Some(second));
}

#[test]
fn rect_and_commit_reach_windows_on_hidden_desktops() {
    let mut desks = Desktops::new(area(), 2);
    let id = desks.open_window(sz(100, 100));
    desks.switch_to(1);
    assert!(desks.rect(id).is_some());
    assert!(desks.on_commit(id, sz(300, 200)));
    assert_eq!(desks.rect(id).map(|rect| rect.size), Some(sz(300, 200)));
    assert!(!desks.on_commit(WindowId::new(999), sz(1, 1)));
}

#[test]
fn close_and_commit_of_unknown_window_are_harmless() {
    let mut desks = Desktops::new(area(), 3);
    let id = desks.open_window(sz(10, 10));
    assert!(desks.move_window_to(id, 2));
    desks.switch_to(1);
    assert!(desks.close_window(id));
    assert!(!desks.on_commit(id, sz(5, 5)));
    assert_eq!(desks.rect(id), None);
}

#[test]
fn windows_moved_away_are_hidden_from_the_source_desktop() {
    let mut desks = Desktops::new(area(), 2);
    let id = desks.open_window(sz(10, 10));
    assert!(desks.move_window_to(id, 1));
    assert!(desks.is_empty());
    desks.switch_to(1);
    assert_eq!(desks.focused(), Some(id));
}

#[test]
fn switch_and_move_to_the_same_desktop_are_no_ops() {
    let mut desks = Desktops::new(area(), 2);
    let id = desks.open_window(sz(10, 10));
    assert!(desks.move_window_to(id, 0));
    assert!(desks.switch_to(0));
    assert_eq!(desks.focused(), Some(id));
    assert_eq!(desks.all_windows(), vec![(id, 0)]);
}
