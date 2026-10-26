//! Converts the layer map's free area into window-manager terms.

use desilium_wm::{Placement, Rect, WindowId};
use smithay::utils::{Logical, Rectangle};

/// The window manager's rectangle for a layer map's non-exclusive zone.
pub fn work_area_of(zone: Rectangle<i32, Logical>) -> Rect {
    Rect::new(zone.loc.x, zone.loc.y, zone.size.w, zone.size.h)
}

/// Windows whose rectangle differs between two placement snapshots (new windows included).
pub fn moved_windows(before: &[Placement], after: &[Placement]) -> Vec<WindowId> {
    after
        .iter()
        .filter(|now| {
            let previous = before.iter().find(|old| old.id == now.id);
            previous.map(|old| old.rect) != Some(now.rect)
        })
        .map(|placement| placement.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_wm::{Size, WindowManager};
    use smithay::utils::{Point, Size as SmithaySize};

    fn placement(id: WindowId, x: i32) -> Placement {
        Placement {
            id,
            rect: Rect::new(x, 0, 10, 10),
            state: desilium_wm::WindowState::Normal,
        }
    }

    #[test]
    fn zone_keeps_its_origin_and_size() {
        let zone = Rectangle::new(Point::from((0, 32)), SmithaySize::from((1280, 768)));
        assert_eq!(work_area_of(zone), Rect::new(0, 32, 1280, 768));
    }

    #[test]
    fn unchanged_placements_report_nothing() {
        let mut wm = WindowManager::new(Rect::new(0, 0, 100, 100));
        let id = wm.open_window(Size { w: 10, h: 10 });
        let snapshot = vec![placement(id, 0)];
        assert!(moved_windows(&snapshot, &snapshot).is_empty());
    }

    #[test]
    fn moved_and_new_windows_are_reported() {
        let mut wm = WindowManager::new(Rect::new(0, 0, 100, 100));
        let moved = wm.open_window(Size { w: 10, h: 10 });
        let added = wm.open_window(Size { w: 10, h: 10 });
        let before = vec![placement(moved, 0)];
        let after = vec![placement(moved, 5), placement(added, 0)];
        assert_eq!(moved_windows(&before, &after), vec![moved, added]);
    }
}
