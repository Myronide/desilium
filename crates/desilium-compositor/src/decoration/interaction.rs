//! Hover, press and double-click bookkeeping of the window chrome (time is passed in).

use super::geometry::Button;
use desilium_wm::{Point, WindowId};
use std::time::Duration;

/// Two clicks within this time on the same window count as a double click.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// Largest pointer travel between the clicks of a double click, per axis.
const DOUBLE_CLICK_SLOP: i32 = 6;

/// Visual state of one button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonState {
    #[default]
    Rest,
    Hover,
    Pressed,
}

type Target = (WindowId, Button);

#[derive(Debug, Clone, Copy)]
struct Click {
    id: WindowId,
    at: Duration,
    position: Point,
}

/// Which button is hovered or pressed, and the last title bar click.
#[derive(Debug, Default)]
pub struct Interaction {
    hover: Option<Target>,
    pressed: Option<Target>,
    last_click: Option<Click>,
}

impl Interaction {
    /// Updates the hovered button; true when the picture changes.
    pub fn set_hover(&mut self, hover: Option<Target>) -> bool {
        if self.hover == hover {
            return false;
        }
        let before = self.hover;
        self.hover = hover;
        // Moving between buttons while one is held only matters if the held one is involved.
        let Some(held) = self.pressed else {
            return true;
        };
        before == Some(held) || hover == Some(held)
    }

    /// Starts pressing a button; true when the picture changes.
    pub fn press(&mut self, target: Target) -> bool {
        self.pressed = Some(target);
        true
    }

    /// Ends a press. Returns the button to activate (released over the pressed button) and whether
    /// the picture changes.
    pub fn release(&mut self, over: Option<Target>) -> (Option<Target>, bool) {
        let Some(held) = self.pressed.take() else {
            return (None, false);
        };
        (over.filter(|target| *target == held), true)
    }

    /// Forgets everything about a closed window; true when the picture changes.
    pub fn forget(&mut self, id: WindowId) -> bool {
        let mut changed = false;
        if self.hover.is_some_and(|(window, _)| window == id) {
            self.hover = None;
            changed = true;
        }
        if self.pressed.is_some_and(|(window, _)| window == id) {
            self.pressed = None;
            changed = true;
        }
        if self.last_click.is_some_and(|click| click.id == id) {
            self.last_click = None;
        }
        changed
    }

    /// How `button` of window `id` is drawn.
    pub fn button_state(&self, id: WindowId, button: Button) -> ButtonState {
        let target = Some((id, button));
        if self.pressed == target && self.hover == target {
            return ButtonState::Pressed;
        }
        match (self.pressed, self.hover == target) {
            (None, true) => ButtonState::Hover,
            _ => ButtonState::Rest,
        }
    }

    /// Records a title bar click; true when it completes a double click.
    pub fn register_click(&mut self, id: WindowId, now: Duration, position: Point) -> bool {
        let double = self.last_click.is_some_and(|last| {
            last.id == id
                && now.saturating_sub(last.at) <= DOUBLE_CLICK
                && (position.x - last.position.x).abs() <= DOUBLE_CLICK_SLOP
                && (position.y - last.position.y).abs() <= DOUBLE_CLICK_SLOP
        });
        self.last_click = if double {
            None
        } else {
            Some(Click {
                id,
                at: now,
                position,
            })
        };
        double
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_wm::{Rect, Size, WindowManager};

    fn id(value: u64) -> WindowId {
        let mut wm = WindowManager::new(Rect::new(0, 0, 100, 100));
        let mut last = wm.open_window(Size { w: 10, h: 10 });
        while last.get() < value {
            last = wm.open_window(Size { w: 10, h: 10 });
        }
        last
    }

    const SPOT: Point = Point { x: 50, y: 10 };

    #[test]
    fn hover_reports_changes_only() {
        let mut interaction = Interaction::default();
        assert!(interaction.set_hover(Some((id(1), Button::Close))));
        assert!(!interaction.set_hover(Some((id(1), Button::Close))));
        assert_eq!(
            interaction.button_state(id(1), Button::Close),
            ButtonState::Hover
        );
        assert_eq!(
            interaction.button_state(id(1), Button::Minimize),
            ButtonState::Rest
        );
        assert_eq!(
            interaction.button_state(id(2), Button::Close),
            ButtonState::Rest
        );
        assert!(interaction.set_hover(None));
        assert_eq!(
            interaction.button_state(id(1), Button::Close),
            ButtonState::Rest
        );
    }

    #[test]
    fn press_then_release_on_the_same_button_activates() {
        let mut interaction = Interaction::default();
        let target = (id(1), Button::Maximize);
        interaction.set_hover(Some(target));
        interaction.press(target);
        assert_eq!(
            interaction.button_state(id(1), Button::Maximize),
            ButtonState::Pressed
        );
        assert_eq!(interaction.release(Some(target)), (Some(target), true));
    }

    #[test]
    fn release_elsewhere_cancels() {
        let mut interaction = Interaction::default();
        let target = (id(1), Button::Close);
        interaction.press(target);
        assert_eq!(
            interaction.release(Some((id(1), Button::Minimize))),
            (None, true)
        );
        interaction.press(target);
        assert_eq!(interaction.release(None), (None, true));
    }

    #[test]
    fn release_without_press_is_ignored() {
        let mut interaction = Interaction::default();
        assert_eq!(
            interaction.release(Some((id(1), Button::Close))),
            (None, false)
        );
    }

    #[test]
    fn dragging_off_the_pressed_button_shows_rest() {
        let mut interaction = Interaction::default();
        let target = (id(1), Button::Close);
        interaction.set_hover(Some(target));
        interaction.press(target);
        assert!(interaction.set_hover(None));
        assert_eq!(
            interaction.button_state(id(1), Button::Close),
            ButtonState::Rest
        );
        assert!(interaction.set_hover(Some(target)));
        assert_eq!(
            interaction.button_state(id(1), Button::Close),
            ButtonState::Pressed
        );
    }

    #[test]
    fn other_buttons_do_not_hover_while_one_is_held() {
        let mut interaction = Interaction::default();
        interaction.press((id(1), Button::Close));
        assert!(!interaction.set_hover(Some((id(1), Button::Minimize))));
        assert_eq!(
            interaction.button_state(id(1), Button::Minimize),
            ButtonState::Rest
        );
    }

    #[test]
    fn forget_clears_the_window() {
        let mut interaction = Interaction::default();
        interaction.set_hover(Some((id(1), Button::Close)));
        assert!(interaction.forget(id(1)));
        assert!(!interaction.forget(id(1)));
    }

    #[test]
    fn double_click_needs_time_place_and_window() {
        let mut interaction = Interaction::default();
        let ms = Duration::from_millis;
        assert!(!interaction.register_click(id(1), ms(0), SPOT));
        assert!(interaction.register_click(id(1), ms(300), Point { x: 52, y: 11 }));
        // A completed double click is consumed: the third click starts over.
        assert!(!interaction.register_click(id(1), ms(350), SPOT));
        assert!(!interaction.register_click(id(1), ms(900), SPOT));
        assert!(!interaction.register_click(id(2), ms(1000), SPOT));
        assert!(!interaction.register_click(id(2), ms(1100), Point { x: 90, y: 10 }));
    }
}
