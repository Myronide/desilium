//! Pure stacking and focus rules for layer surfaces.
//!
//! Kept free of Smithay state so the ordering decisions can be unit tested.

use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, Layer};

/// Layers stacked above every window, listed top-down (overlay first).
pub const ABOVE_WINDOWS: [Layer; 2] = [Layer::Overlay, Layer::Top];

/// Layers stacked below every window, listed top-down (bottom first).
pub const BELOW_WINDOWS: [Layer; 2] = [Layer::Bottom, Layer::Background];

/// Every layer in ascending stacking order.
const ASCENDING: [Layer; 4] = [Layer::Background, Layer::Bottom, Layer::Top, Layer::Overlay];

/// Whether surfaces on this layer are drawn above application windows.
pub fn is_above_windows(layer: Layer) -> bool {
    ABOVE_WINDOWS.contains(&layer)
}

/// Picks the surface that takes exclusive keyboard focus.
///
/// Exclusive focus is only honoured above the window layer. When several surfaces ask
/// for it, the highest layer wins and, within that layer, the most recently mapped one.
/// `entries` must be in mapping order (oldest first).
pub fn pick_exclusive<T>(
    entries: impl IntoIterator<Item = (Layer, KeyboardInteractivity, T)>,
) -> Option<T> {
    entries
        .into_iter()
        .filter(|(layer, mode, _)| {
            is_above_windows(*layer) && *mode == KeyboardInteractivity::Exclusive
        })
        .max_by_key(|(layer, _, _)| layer_rank(*layer))
        .map(|(_, _, target)| target)
}

/// Position of a layer in the ascending stack.
fn layer_rank(layer: Layer) -> usize {
    ASCENDING
        .iter()
        .position(|&candidate| candidate == layer)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(
        layer: Layer,
        mode: KeyboardInteractivity,
        name: &'static str,
    ) -> (Layer, KeyboardInteractivity, &'static str) {
        (layer, mode, name)
    }

    #[test]
    fn window_layers_split_around_windows() {
        assert!(is_above_windows(Layer::Overlay));
        assert!(is_above_windows(Layer::Top));
        assert!(!is_above_windows(Layer::Bottom));
        assert!(!is_above_windows(Layer::Background));
    }

    #[test]
    fn no_entries_means_no_exclusive_focus() {
        let picked = pick_exclusive(Vec::<(Layer, KeyboardInteractivity, u8)>::new());
        assert_eq!(picked, None);
    }

    #[test]
    fn on_demand_and_none_never_take_exclusive_focus() {
        let entries = [
            entry(Layer::Overlay, KeyboardInteractivity::OnDemand, "launcher"),
            entry(Layer::Top, KeyboardInteractivity::None, "bar"),
        ];
        assert_eq!(pick_exclusive(entries), None);
    }

    #[test]
    fn exclusive_below_the_window_layer_is_ignored() {
        let entries = [entry(
            Layer::Bottom,
            KeyboardInteractivity::Exclusive,
            "dock",
        )];
        assert_eq!(pick_exclusive(entries), None);
    }

    #[test]
    fn overlay_beats_top_even_when_mapped_first() {
        let entries = [
            entry(Layer::Overlay, KeyboardInteractivity::Exclusive, "lock"),
            entry(Layer::Top, KeyboardInteractivity::Exclusive, "panel"),
        ];
        assert_eq!(pick_exclusive(entries), Some("lock"));
    }

    #[test]
    fn newest_wins_within_the_same_layer() {
        let entries = [
            entry(Layer::Top, KeyboardInteractivity::Exclusive, "old"),
            entry(Layer::Top, KeyboardInteractivity::Exclusive, "new"),
        ];
        assert_eq!(pick_exclusive(entries), Some("new"));
    }

    #[test]
    fn non_exclusive_entries_do_not_hide_an_exclusive_one() {
        let entries = [
            entry(Layer::Overlay, KeyboardInteractivity::OnDemand, "menu"),
            entry(Layer::Top, KeyboardInteractivity::Exclusive, "launcher"),
        ];
        assert_eq!(pick_exclusive(entries), Some("launcher"));
    }
}
