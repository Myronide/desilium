//! Logical cursor shapes and their name mappings (CSS, cursor-shape-v1, XCursor).

/// Number of cursor kinds; sizes the per-kind caches.
pub(crate) const KIND_COUNT: usize = 20;

/// Logical cursor shape requested by a client or by the compositor itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CursorKind {
    Default,
    Text,
    Pointer,
    Grab,
    Grabbing,
    Wait,
    Progress,
    NotAllowed,
    Crosshair,
    Move,
    ResizeN,
    ResizeS,
    ResizeE,
    ResizeW,
    ResizeNe,
    ResizeNw,
    ResizeSe,
    ResizeSw,
    ResizeEw,
    ResizeNs,
}

impl CursorKind {
    /// Every kind in declaration order; `index` depends on this order.
    pub const ALL: [CursorKind; KIND_COUNT] = [
        CursorKind::Default,
        CursorKind::Text,
        CursorKind::Pointer,
        CursorKind::Grab,
        CursorKind::Grabbing,
        CursorKind::Wait,
        CursorKind::Progress,
        CursorKind::NotAllowed,
        CursorKind::Crosshair,
        CursorKind::Move,
        CursorKind::ResizeN,
        CursorKind::ResizeS,
        CursorKind::ResizeE,
        CursorKind::ResizeW,
        CursorKind::ResizeNe,
        CursorKind::ResizeNw,
        CursorKind::ResizeSe,
        CursorKind::ResizeSw,
        CursorKind::ResizeEw,
        CursorKind::ResizeNs,
    ];

    /// Dense slot of this kind, valid because `ALL` is in declaration order.
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// Canonical CSS / cursor-shape-v1 name of this kind.
    pub fn css_name(self) -> &'static str {
        match self {
            CursorKind::Default => "default",
            CursorKind::Text => "text",
            CursorKind::Pointer => "pointer",
            CursorKind::Grab => "grab",
            CursorKind::Grabbing => "grabbing",
            CursorKind::Wait => "wait",
            CursorKind::Progress => "progress",
            CursorKind::NotAllowed => "not-allowed",
            CursorKind::Crosshair => "crosshair",
            CursorKind::Move => "move",
            CursorKind::ResizeN => "n-resize",
            CursorKind::ResizeS => "s-resize",
            CursorKind::ResizeE => "e-resize",
            CursorKind::ResizeW => "w-resize",
            CursorKind::ResizeNe => "ne-resize",
            CursorKind::ResizeNw => "nw-resize",
            CursorKind::ResizeSe => "se-resize",
            CursorKind::ResizeSw => "sw-resize",
            CursorKind::ResizeEw => "ew-resize",
            CursorKind::ResizeNs => "ns-resize",
        }
    }

    /// XCursor file names to try, in priority order (first existing file wins).
    pub fn xcursor_names(self) -> &'static [&'static str] {
        match self {
            CursorKind::Default => &["default", "left_ptr", "arrow"],
            CursorKind::Text => &["text", "xterm", "ibeam"],
            CursorKind::Pointer => &["pointer", "hand2", "hand1", "left_ptr"],
            CursorKind::Grab => &["grab", "openhand", "hand1", "left_ptr"],
            CursorKind::Grabbing => &["grabbing", "closedhand", "hand2", "left_ptr"],
            CursorKind::Wait => &["wait", "watch", "left_ptr_watch"],
            CursorKind::Progress => &["progress", "left_ptr_watch", "half-busy", "watch"],
            CursorKind::NotAllowed => &["not-allowed", "crossed_circle", "no-drop", "forbidden"],
            CursorKind::Crosshair => &["crosshair", "cross", "tcross"],
            CursorKind::Move => &["move", "fleur", "all-scroll", "size_all"],
            CursorKind::ResizeN => &["n-resize", "top_side", "size_ver"],
            CursorKind::ResizeS => &["s-resize", "bottom_side", "size_ver"],
            CursorKind::ResizeE => &["e-resize", "right_side", "size_hor"],
            CursorKind::ResizeW => &["w-resize", "left_side", "size_hor"],
            CursorKind::ResizeNe => &["ne-resize", "top_right_corner", "size_bdiag"],
            CursorKind::ResizeNw => &["nw-resize", "top_left_corner", "size_fdiag"],
            CursorKind::ResizeSe => &["se-resize", "bottom_right_corner", "size_fdiag"],
            CursorKind::ResizeSw => &["sw-resize", "bottom_left_corner", "size_bdiag"],
            CursorKind::ResizeEw => &["ew-resize", "col-resize", "sb_h_double_arrow", "size_hor"],
            CursorKind::ResizeNs => &["ns-resize", "row-resize", "sb_v_double_arrow", "size_ver"],
        }
    }

    /// Parses a CSS / cursor-shape name, including common aliases. Unknown names give `None`.
    pub fn from_css_name(name: &str) -> Option<CursorKind> {
        CursorKind::ALL
            .into_iter()
            .find(|kind| kind.css_name() == name)
            .or_else(|| css_alias(name))
    }
}

/// Aliases that CSS and toolkits use for the same shapes.
fn css_alias(name: &str) -> Option<CursorKind> {
    match name {
        "nesw-resize" => Some(CursorKind::ResizeNe),
        "nwse-resize" => Some(CursorKind::ResizeNw),
        "col-resize" => Some(CursorKind::ResizeEw),
        "row-resize" => Some(CursorKind::ResizeNs),
        "all-scroll" => Some(CursorKind::Move),
        "no-drop" => Some(CursorKind::NotAllowed),
        "hand" => Some(CursorKind::Pointer),
        "xterm" => Some(CursorKind::Text),
        "half-busy" => Some(CursorKind::Progress),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_is_in_declaration_order() {
        for (position, kind) in CursorKind::ALL.iter().enumerate() {
            assert_eq!(kind.index(), position);
        }
    }

    #[test]
    fn css_names_round_trip() {
        for kind in CursorKind::ALL {
            assert_eq!(CursorKind::from_css_name(kind.css_name()), Some(kind));
        }
    }

    #[test]
    fn css_names_are_unique() {
        let mut names: Vec<&str> = CursorKind::ALL.iter().map(|k| k.css_name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), KIND_COUNT);
    }

    #[test]
    fn parses_shape_and_resize_names() {
        assert_eq!(
            CursorKind::from_css_name("default"),
            Some(CursorKind::Default)
        );
        assert_eq!(
            CursorKind::from_css_name("pointer"),
            Some(CursorKind::Pointer)
        );
        assert_eq!(
            CursorKind::from_css_name("ew-resize"),
            Some(CursorKind::ResizeEw)
        );
        assert_eq!(
            CursorKind::from_css_name("not-allowed"),
            Some(CursorKind::NotAllowed)
        );
    }

    #[test]
    fn parses_diagonal_and_alias_names() {
        assert_eq!(
            CursorKind::from_css_name("nesw-resize"),
            Some(CursorKind::ResizeNe)
        );
        assert_eq!(
            CursorKind::from_css_name("nwse-resize"),
            Some(CursorKind::ResizeNw)
        );
        assert_eq!(
            CursorKind::from_css_name("col-resize"),
            Some(CursorKind::ResizeEw)
        );
        assert_eq!(
            CursorKind::from_css_name("all-scroll"),
            Some(CursorKind::Move)
        );
    }

    #[test]
    fn rejects_unknown_names() {
        assert_eq!(CursorKind::from_css_name(""), None);
        assert_eq!(CursorKind::from_css_name("Pointer"), None);
        assert_eq!(CursorKind::from_css_name("sparkle"), None);
    }

    #[test]
    fn every_kind_has_xcursor_names() {
        for kind in CursorKind::ALL {
            assert!(!kind.xcursor_names().is_empty());
        }
    }
}
