//! All popup coordinates are output-local and clamped before requesting a layer surface.
use desilium_ui::Rect;
#[derive(Debug, Clone, Copy)]
pub enum Origin {
    Top,
    Bottom,
    App { button: Rect, below: bool },
}
/// Space taken from the top and bottom screen edges by the bar and the dock.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Insets {
    pub top: u32,
    pub bottom: u32,
}
/// Gap between a panel and a popup that hangs off it.
const GAP: f64 = 8.0;
pub fn bounds(output: (i32, i32), origin: Origin, desired_height: u32, insets: Insets) -> Rect {
    let width = f64::from(output.0.max(1));
    let height = f64::from(output.1.max(1));
    let margin = 12.0f64.min((width.min(height) - 1.0).max(0.0) / 2.0);
    let popup_width = 410.0f64.min((width - 2.0 * margin).max(1.0));
    let desired = f64::from(desired_height);
    let panel_inset = insets.bottom;
    let below_bar = (f64::from(insets.top) + GAP).min(height - margin - 1.0);
    let (left, top, available) = match origin {
        Origin::Top => (
            width - popup_width - margin,
            below_bar,
            height - below_bar - f64::from(insets.bottom) - margin,
        ),
        Origin::Bottom => (
            width - popup_width - margin,
            margin,
            height - f64::from(panel_inset) - margin - GAP,
        ),
        Origin::App { button, below } => {
            let above = (button.y - 8.0 - margin).max(0.0);
            let after = button.y + button.height + 8.0;
            let below_space = (height - margin - after).max(0.0);
            let below = if below {
                below_space >= 180.0 || below_space >= above
            } else {
                above < 180.0 && below_space > above
            };
            let available = if below { below_space } else { above };
            let popup_height = desired.min(available).max(1.0);
            let top = if below {
                after
            } else {
                button.y - 8.0 - popup_height
            };
            (
                button.x + button.width / 2.0 - popup_width / 2.0,
                top,
                available,
            )
        }
    };
    let popup_height = desired
        .min(available.max(1.0))
        .min((height - 2.0 * margin).max(1.0));
    let top = if matches!(origin, Origin::Bottom) {
        height - f64::from(panel_inset) - 8.0 - popup_height
    } else {
        top
    };
    Rect {
        x: left.clamp(margin, (width - margin - popup_width).max(margin)),
        y: top.clamp(margin, (height - margin - popup_height).max(margin)),
        width: popup_width,
        height: popup_height,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn inside(rect: Rect, size: (i32, i32)) {
        assert!(rect.x >= 0.0 && rect.y >= 0.0 && rect.width > 0.0 && rect.height > 0.0);
        assert!(rect.x + rect.width <= f64::from(size.0) + 0.1);
        assert!(rect.y + rect.height <= f64::from(size.1) + 0.1);
    }
    #[test]
    fn popups_fit_small_and_large_outputs_and_both_edges() {
        for size in [(1280, 800), (800, 480), (320, 240), (80, 60)] {
            for origin in [
                Origin::Top,
                Origin::Bottom,
                Origin::App {
                    button: Rect {
                        x: 2.0,
                        y: 40.0,
                        width: 48.0,
                        height: 48.0,
                    },
                    below: true,
                },
                Origin::App {
                    button: Rect {
                        x: 1100.0,
                        y: 700.0,
                        width: 48.0,
                        height: 48.0,
                    },
                    below: false,
                },
            ] {
                let insets = Insets {
                    top: 32,
                    bottom: 90,
                };
                inside(bounds(size, origin, 620, insets), size);
            }
        }
    }
    #[test]
    fn top_popup_hangs_below_the_bar_and_follows_its_height() {
        let tall = bounds((1280, 800), Origin::Top, 300, Insets { top: 32, bottom: 0 });
        let none = bounds((1280, 800), Origin::Top, 300, Insets::default());
        assert_eq!(tall.y, 40.0);
        assert_eq!(
            none.y, 12.0,
            "without a bar the popup keeps the screen margin"
        );
        assert_eq!(tall.x + tall.width, 1268.0);
    }
    #[test]
    fn bottom_popup_sits_above_taskbar_and_app_menu_follows_its_icon() {
        let insets = Insets { top: 0, bottom: 78 };
        let popup = bounds((1280, 800), Origin::Bottom, 620, insets);
        assert!(popup.y + popup.height <= 714.0);
        let button = Rect {
            x: 300.0,
            y: 722.0,
            width: 48.0,
            height: 48.0,
        };
        let popup = bounds(
            (1280, 800),
            Origin::App {
                button,
                below: false,
            },
            300,
            insets,
        );
        assert_eq!(popup.x + popup.width / 2.0, 324.0);
        assert_eq!(popup.y + popup.height, 714.0);
    }
}
