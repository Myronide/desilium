//! Frame layout: where the title bar, buttons and client surface sit inside a window frame.

use super::metrics::{Chrome, Metrics, TITLE_GAP, TITLE_INSET};
use desilium_wm::{Point, Rect, Size};

/// The three title bar buttons in layout order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Button {
    Minimize,
    Maximize,
    Close,
}

impl Button {
    /// Layout order, left to right.
    pub const ALL: [Button; 3] = [Button::Minimize, Button::Maximize, Button::Close];

    /// Position in layout order.
    pub fn index(self) -> usize {
        match self {
            Button::Minimize => 0,
            Button::Maximize => 1,
            Button::Close => 2,
        }
    }
}

/// Space taken by chrome around the client surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Insets {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Insets {
    /// No chrome.
    pub const ZERO: Insets = Insets {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };

    /// Left plus right.
    pub fn horizontal(self) -> i32 {
        self.left.saturating_add(self.right)
    }

    /// Top plus bottom.
    pub fn vertical(self) -> i32 {
        self.top.saturating_add(self.bottom)
    }

    /// Size of the client surface inside a frame of `frame` (never below 1x1).
    pub fn client_size(self, frame: Size) -> Size {
        Size {
            w: frame.w.saturating_sub(self.horizontal()).max(1),
            h: frame.h.saturating_sub(self.vertical()).max(1),
        }
    }

    /// Size of the frame around a client surface of `client`.
    pub fn frame_size(self, client: Size) -> Size {
        Size {
            w: client.w.saturating_add(self.horizontal()),
            h: client.h.saturating_add(self.vertical()),
        }
    }
}

/// Every rectangle of one window frame, in compositor coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub chrome: Chrome,
    /// The rectangle the window manager manages (title bar and border included).
    pub outer: Rect,
    /// Empty without chrome.
    pub title_bar: Rect,
    /// Where the client surface is mapped.
    pub client: Rect,
    /// Button hit areas in [`Button::ALL`] order; empty without chrome.
    pub buttons: [Rect; 3],
    /// Area available for the title text; empty without chrome.
    pub title_text: Rect,
    /// Border width actually drawn.
    pub border: i32,
}

const EMPTY: Rect = Rect::new(0, 0, 0, 0);

impl Frame {
    /// Lays out `outer` with the chrome `chrome` wears.
    pub fn new(metrics: &Metrics, chrome: Chrome, outer: Rect) -> Frame {
        let insets = metrics.insets(chrome);
        let client_size = insets.client_size(Size {
            w: outer.size.w,
            h: outer.size.h,
        });
        let client = Rect {
            loc: Point {
                x: outer.loc.x.saturating_add(insets.left),
                y: outer.loc.y.saturating_add(insets.top),
            },
            size: client_size,
        };
        if chrome == Chrome::None {
            return Frame {
                chrome,
                outer,
                title_bar: EMPTY,
                client: outer,
                buttons: [EMPTY; 3],
                title_text: EMPTY,
                border: 0,
            };
        }
        let title_bar = Rect {
            loc: outer.loc,
            size: Size {
                w: outer.size.w,
                h: metrics.title_height,
            },
        };
        let buttons = button_rects(metrics, title_bar);
        Frame {
            chrome,
            outer,
            title_bar,
            client,
            buttons,
            title_text: title_text_area(metrics, title_bar),
            border: insets.left,
        }
    }

    /// Hit area of `button`.
    pub fn button_rect(&self, button: Button) -> Rect {
        self.buttons[button.index()]
    }

    /// Glyph box (centred in the button hit area).
    pub fn glyph_rect(&self, metrics: &Metrics, button: Button) -> Rect {
        let area = self.button_rect(button);
        let edge = metrics.glyph_size;
        Rect::new(
            area.loc.x + (area.size.w - edge) / 2,
            area.loc.y + (area.size.h - edge) / 2,
            edge,
            edge,
        )
    }
}

fn button_rects(metrics: &Metrics, title_bar: Rect) -> [Rect; 3] {
    let group = metrics.button_group_width();
    let group_left = if metrics.buttons_left {
        title_bar.loc.x + metrics.button_side_inset
    } else {
        title_bar.right() - metrics.button_side_inset - group
    };
    let top = title_bar.loc.y + (title_bar.size.h - metrics.button_height) / 2;
    let mut rects = [EMPTY; 3];
    for button in Button::ALL {
        let step = metrics.button_width + metrics.button_spacing;
        let index = i32::try_from(button.index()).unwrap_or(0);
        rects[button.index()] = Rect::new(
            group_left + index * step,
            top,
            metrics.button_width,
            metrics.button_height,
        );
    }
    rects
}

fn title_text_area(metrics: &Metrics, title_bar: Rect) -> Rect {
    let group = metrics.button_group_width() + metrics.button_side_inset;
    let (left, right) = if metrics.buttons_left {
        (
            title_bar.loc.x + group + TITLE_GAP,
            title_bar.right() - TITLE_INSET,
        )
    } else {
        (
            title_bar.loc.x + TITLE_INSET,
            title_bar.right() - group - TITLE_GAP,
        )
    };
    Rect::new(
        left,
        title_bar.loc.y,
        (right - left).max(0),
        title_bar.size.h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use desilium_theme::Theme;

    fn metrics() -> Metrics {
        Metrics::from_theme(&Theme::dark())
    }

    #[test]
    fn windowed_frame_layout() {
        let frame = Frame::new(&metrics(), Chrome::Windowed, Rect::new(100, 50, 400, 300));
        assert_eq!(frame.title_bar, Rect::new(100, 50, 400, 36));
        assert_eq!(frame.client, Rect::new(101, 86, 398, 263));
        assert_eq!(frame.border, 1);
    }

    #[test]
    fn buttons_follow_the_theme_convention() {
        let frame = Frame::new(&metrics(), Chrome::Windowed, Rect::new(0, 0, 400, 300));
        // group_left = 400 - 4 - 100 = 296; y = (36 - 28) / 2 = 4
        assert_eq!(
            frame.button_rect(Button::Minimize),
            Rect::new(296, 4, 32, 28)
        );
        assert_eq!(
            frame.button_rect(Button::Maximize),
            Rect::new(330, 4, 32, 28)
        );
        assert_eq!(frame.button_rect(Button::Close), Rect::new(364, 4, 32, 28));
    }

    #[test]
    fn glyph_box_is_centred_in_the_button() {
        let metrics = metrics();
        let frame = Frame::new(&metrics, Chrome::Windowed, Rect::new(0, 0, 400, 300));
        assert_eq!(
            frame.glyph_rect(&metrics, Button::Close),
            Rect::new(372, 10, 16, 16)
        );
    }

    #[test]
    fn left_aligned_buttons_move_the_title_right() {
        let mut theme = Theme::dark();
        theme.window.buttons.alignment = desilium_theme::ButtonAlignment::Left;
        let metrics = Metrics::from_theme(&theme);
        let frame = Frame::new(&metrics, Chrome::Windowed, Rect::new(0, 0, 400, 300));
        assert_eq!(frame.button_rect(Button::Minimize).loc.x, 4);
        assert!(frame.title_text.loc.x > frame.button_rect(Button::Close).right());
    }

    #[test]
    fn title_text_stays_clear_of_buttons() {
        let frame = Frame::new(&metrics(), Chrome::Windowed, Rect::new(0, 0, 400, 300));
        assert_eq!(frame.title_text.loc.x, TITLE_INSET);
        assert!(frame.title_text.right() + TITLE_GAP <= frame.button_rect(Button::Minimize).loc.x);
    }

    #[test]
    fn maximized_has_no_border_but_a_title_bar() {
        let frame = Frame::new(&metrics(), Chrome::Maximized, Rect::new(0, 0, 800, 600));
        assert_eq!(frame.client, Rect::new(0, 36, 800, 564));
        assert_eq!(frame.border, 0);
    }

    #[test]
    fn no_chrome_means_client_fills_the_frame() {
        let frame = Frame::new(&metrics(), Chrome::None, Rect::new(5, 6, 100, 80));
        assert_eq!(frame.client, frame.outer);
        assert_eq!(frame.title_bar.size, Size { w: 0, h: 0 });
    }

    #[test]
    fn client_and_frame_sizes_round_trip() {
        let insets = metrics().insets(Chrome::Windowed);
        let client = Size { w: 640, h: 480 };
        assert_eq!(insets.client_size(insets.frame_size(client)), client);
        assert_eq!(insets.frame_size(client), Size { w: 642, h: 517 });
    }

    #[test]
    fn client_size_never_collapses() {
        let insets = metrics().insets(Chrome::Windowed);
        assert_eq!(
            insets.client_size(Size { w: 0, h: 10 }),
            Size { w: 1, h: 1 }
        );
    }
}
