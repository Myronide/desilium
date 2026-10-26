//! Backend-neutral input and output types of a [`UiSurface`](crate::UiSurface).
//!
//! Nothing in here names a Blitz type, so the rendering backend can be swapped.

use keyboard_types::{Key as BlitzKey, Modifiers as BlitzModifiers, NamedKey};

/// Pointer button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    /// Usually the left button.
    Primary,
    /// Usually the right button.
    Secondary,
    /// Usually the wheel button.
    Middle,
}

/// Keyboard modifier state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    /// Shift is held.
    pub shift: bool,
    /// Control is held.
    pub ctrl: bool,
    /// Alt is held.
    pub alt: bool,
    /// The logo/super key is held.
    pub logo: bool,
}

impl Modifiers {
    pub(crate) fn to_backend(self) -> BlitzModifiers {
        let mut out = BlitzModifiers::empty();
        out.set(BlitzModifiers::SHIFT, self.shift);
        out.set(BlitzModifiers::CONTROL, self.ctrl);
        out.set(BlitzModifiers::ALT, self.alt);
        out.set(BlitzModifiers::META, self.logo);
        out
    }
}

/// Logical key, as resolved by xkb (layout and modifiers already applied).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// A printable key; the string is what it types.
    Character(String),
    /// Enter / Return.
    Enter,
    /// Backspace.
    Backspace,
    /// Forward delete.
    Delete,
    /// Tab.
    Tab,
    /// Escape.
    Escape,
    /// Cursor left.
    ArrowLeft,
    /// Cursor right.
    ArrowRight,
    /// Cursor up.
    ArrowUp,
    /// Cursor down.
    ArrowDown,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Anything the UI does not interpret.
    Other,
}

impl Key {
    pub(crate) fn to_backend(&self) -> BlitzKey {
        let named = match self {
            Key::Character(text) => return BlitzKey::Character(text.clone()),
            Key::Enter => NamedKey::Enter,
            Key::Backspace => NamedKey::Backspace,
            Key::Delete => NamedKey::Delete,
            Key::Tab => NamedKey::Tab,
            Key::Escape => NamedKey::Escape,
            Key::ArrowLeft => NamedKey::ArrowLeft,
            Key::ArrowRight => NamedKey::ArrowRight,
            Key::ArrowUp => NamedKey::ArrowUp,
            Key::ArrowDown => NamedKey::ArrowDown,
            Key::Home => NamedKey::Home,
            Key::End => NamedKey::End,
            Key::PageUp => NamedKey::PageUp,
            Key::PageDown => NamedKey::PageDown,
            Key::Other => NamedKey::Unidentified,
        };
        BlitzKey::Named(named)
    }
}

/// One keyboard event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    /// Logical key.
    pub key: Key,
    /// Modifiers held when the key was pressed.
    pub modifiers: Modifiers,
    /// Text the key produces, if any (used for typing into inputs).
    pub text: Option<String>,
    /// True for synthesized key-repeat events.
    pub repeat: bool,
}

impl KeyEvent {
    /// A plain character key press without modifiers.
    pub fn character(text: &str) -> Self {
        Self {
            key: Key::Character(text.to_owned()),
            modifiers: Modifiers::default(),
            text: Some(text.to_owned()),
            repeat: false,
        }
    }

    /// A key press without text and modifiers.
    pub fn named(key: Key) -> Self {
        Self {
            key,
            modifiers: Modifiers::default(),
            text: None,
            repeat: false,
        }
    }
}

/// Scroll amount of a wheel event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WheelDelta {
    /// Whole lines (mouse wheel notches).
    Lines {
        /// Horizontal lines, positive to the right.
        x: f64,
        /// Vertical lines, positive downwards.
        y: f64,
    },
    /// Logical pixels (touchpads).
    Pixels {
        /// Horizontal pixels, positive to the right.
        x: f64,
        /// Vertical pixels, positive downwards.
        y: f64,
    },
}

/// Cursor shape the surface wants while the pointer is over it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorHint {
    /// Arrow.
    #[default]
    Default,
    /// Hand, for clickable things.
    Pointer,
    /// I-beam, for text.
    Text,
}

/// Axis-aligned rectangle in logical (CSS) pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// Integer rectangle in buffer (physical) pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelRect {
    /// Left edge.
    pub x: u32,
    /// Top edge.
    pub y: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
}

/// A click on an element carrying `data-action="name"` (and optionally `data-arg`).
#[derive(Debug, Clone, PartialEq)]
pub struct UiAction {
    /// Value of `data-action`.
    pub action: String,
    /// Value of `data-arg`, if present.
    pub arg: Option<String>,
    /// The element's `id` attribute, if it has one.
    pub element_id: Option<String>,
    /// The element's border box in logical pixels.
    pub rect: Rect,
}

/// What the pointer currently hovers, for tooltips.
#[derive(Debug, Clone, PartialEq)]
pub struct HoverTarget {
    /// Value of the nearest `data-tooltip` attribute.
    pub tooltip: Option<String>,
    /// The element carrying it (or the hovered action element).
    pub rect: Rect,
    /// `id` attribute of that element.
    pub element_id: Option<String>,
    /// `data-action` of the hovered element, if any.
    pub action: Option<String>,
}

/// Something observable that happened inside the document besides plain clicks.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    /// A click on `data-action`, `data-middle-action` or `data-secondary-action`.
    Action(UiAction),
    /// The value of a text input changed (typing, deleting, paste).
    TextChanged {
        /// `id` of the input, if it has one.
        element_id: Option<String>,
        /// `data-key` of the input, if it has one.
        key: Option<String>,
        /// New full value.
        value: String,
    },
    /// Enter was pressed in an input carrying `data-submit="name"`.
    Submit {
        /// Value of `data-submit`.
        action: String,
        /// Current text of the input.
        value: String,
    },
}

/// Statistics of one [`UiSurface::render`](crate::UiSurface::render) call.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RenderStats {
    /// False when nothing was dirty and the buffer was left untouched.
    pub rendered: bool,
    /// Region that differs from the previous frame in buffer pixels (`None` = identical).
    pub damage: Option<PixelRect>,
    /// Style resolution plus layout.
    pub resolve: std::time::Duration,
    /// Painting and rasterisation.
    pub paint: std::time::Duration,
    /// Conversion into the destination buffer, including the damage diff.
    pub convert: std::time::Duration,
}

impl RenderStats {
    /// Total time spent in the call.
    pub fn total(&self) -> std::time::Duration {
        self.resolve + self.paint + self.convert
    }
}
