//! Public configuration and event types of the layer-shell host.

use smithay_client_toolkit::shell::wlr_layer as sctk;

use crate::{HoverTarget, UiEvent};

/// Stacking layer of a layer-shell surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Below everything, above the wallpaper.
    Background,
    /// Below windows.
    Bottom,
    /// Above windows (bars, docks).
    Top,
    /// Above everything including fullscreen (lock screen, switcher).
    Overlay,
}

impl Layer {
    pub(super) fn to_sctk(self) -> sctk::Layer {
        match self {
            Layer::Background => sctk::Layer::Background,
            Layer::Bottom => sctk::Layer::Bottom,
            Layer::Top => sctk::Layer::Top,
            Layer::Overlay => sctk::Layer::Overlay,
        }
    }
}

/// Screen edges a surface is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Anchor {
    /// Attached to the top edge.
    pub top: bool,
    /// Attached to the bottom edge.
    pub bottom: bool,
    /// Attached to the left edge.
    pub left: bool,
    /// Attached to the right edge.
    pub right: bool,
}

impl Anchor {
    /// Top bar: top, left and right.
    pub const TOP_BAR: Anchor = Anchor {
        top: true,
        bottom: false,
        left: true,
        right: true,
    };
    /// Centred at the bottom edge (dock).
    pub const BOTTOM_CENTER: Anchor = Anchor {
        top: false,
        bottom: true,
        left: false,
        right: false,
    };
    /// Not attached anywhere: centred on the output.
    pub const CENTER: Anchor = Anchor {
        top: false,
        bottom: false,
        left: false,
        right: false,
    };

    pub(super) fn to_sctk(self) -> sctk::Anchor {
        let mut out = sctk::Anchor::empty();
        out.set(sctk::Anchor::TOP, self.top);
        out.set(sctk::Anchor::BOTTOM, self.bottom);
        out.set(sctk::Anchor::LEFT, self.left);
        out.set(sctk::Anchor::RIGHT, self.right);
        out
    }
}

/// Distance from the anchored edges in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Margins {
    /// Top margin.
    pub top: i32,
    /// Right margin.
    pub right: i32,
    /// Bottom margin.
    pub bottom: i32,
    /// Left margin.
    pub left: i32,
}

/// Whether and how the surface receives keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KeyboardMode {
    /// Never takes focus.
    #[default]
    None,
    /// Takes focus when clicked.
    OnDemand,
    /// Grabs focus while mapped (launcher, lock screen).
    Exclusive,
}

impl KeyboardMode {
    pub(super) fn to_sctk(self) -> sctk::KeyboardInteractivity {
        match self {
            KeyboardMode::None => sctk::KeyboardInteractivity::None,
            KeyboardMode::OnDemand => sctk::KeyboardInteractivity::OnDemand,
            KeyboardMode::Exclusive => sctk::KeyboardInteractivity::Exclusive,
        }
    }
}

/// Everything the compositor needs to place a layer surface.
#[derive(Debug, Clone, PartialEq)]
pub struct LayerConfig {
    /// Namespace string, e.g. `"desilium-bar"`.
    pub namespace: String,
    /// Stacking layer.
    pub layer: Layer,
    /// Attached edges.
    pub anchor: Anchor,
    /// Margins from the anchored edges.
    pub margins: Margins,
    /// Space other windows must keep free: positive reserves, 0 none, -1 ignores others' zones.
    pub exclusive_zone: i32,
    /// Requested logical size; 0 on an axis stretches to the anchored edges.
    pub size: (u32, u32),
    /// Keyboard focus policy.
    pub keyboard: KeyboardMode,
    /// Output name (e.g. `"HDMI-A-1"`); `None` lets the compositor choose.
    pub output: Option<String>,
    /// Report key presses as [`HostEvent::Key`] and keep navigation keys (Esc, Tab,
    /// arrows, Enter, paging) and Alt/Logo chords away from the document, so the
    /// application can drive selection while text still reaches focused inputs.
    pub report_keys: bool,
}

impl LayerConfig {
    /// A fixed-size surface on the Top layer with no anchors and no keyboard focus.
    pub fn new(namespace: &str, size: (u32, u32)) -> Self {
        Self {
            namespace: namespace.to_owned(),
            layer: Layer::Top,
            anchor: Anchor::CENTER,
            margins: Margins::default(),
            exclusive_zone: 0,
            size,
            keyboard: KeyboardMode::None,
            output: None,
            report_keys: false,
        }
    }
}

/// Handle of a surface managed by a [`LayerHost`](crate::wayland::LayerHost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub(super) u32);

/// Something the host wants the application to know.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    /// A document event (click action, text change, submit).
    Ui {
        /// Surface it happened on.
        surface: SurfaceId,
        /// The event.
        event: UiEvent,
    },
    /// The hovered tooltip/action element of a surface changed (`None` = left it).
    Hover {
        /// Surface it happened on.
        surface: SurfaceId,
        /// New hover target.
        target: Option<HoverTarget>,
    },
    /// A key was pressed on a surface created with `report_keys` (including repeats).
    Key {
        /// Surface that has keyboard focus.
        surface: SurfaceId,
        /// The key.
        event: crate::KeyEvent,
    },
    /// Keyboard focus entered (`true`) or left (`false`) a surface.
    KeyboardFocus {
        /// Surface concerned.
        surface: SurfaceId,
        /// Whether it has focus now.
        focused: bool,
    },
    /// The compositor closed the surface (e.g. its output went away); it is removed.
    Closed(SurfaceId),
}
