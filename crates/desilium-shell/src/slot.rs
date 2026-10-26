//! A layer surface that exists on the compositor only while it is shown.
//!
//! The document stays in memory while the surface is gone ("parked"), so showing it
//! again costs a surface creation, not a document build. Re-creating the surface is
//! the protocol-clean way to hide an overlay: an unmapped layer surface would have to
//! wait for a new configure, and not every compositor sends one.

use desilium_ui::{
    UiError, UiSurface,
    wayland::{LayerConfig, LayerHost, SurfaceId},
};

/// Placement and document of a hidden surface, or the id of the live one.
pub struct Slot {
    live: Option<SurfaceId>,
    parked: Option<(LayerConfig, UiSurface)>,
}

impl Slot {
    /// A hidden slot holding `ui` with `config` for when it is shown.
    pub fn parked(config: LayerConfig, ui: UiSurface) -> Self {
        Self {
            live: None,
            parked: Some((config, ui)),
        }
    }

    /// The compositor-side surface, while shown.
    pub fn id(&self) -> Option<SurfaceId> {
        self.live
    }

    /// True while the surface exists on the compositor.
    pub fn is_shown(&self) -> bool {
        self.live.is_some()
    }

    /// True when `id` is this slot's live surface.
    pub fn owns(&self, id: SurfaceId) -> bool {
        self.live == Some(id)
    }

    /// The document, shown or not.
    pub fn ui_mut<'a>(&'a mut self, host: &'a mut LayerHost) -> Option<&'a mut UiSurface> {
        match (&mut self.parked, self.live) {
            (Some((_, ui)), _) => Some(ui),
            (None, Some(id)) => host.surface_mut(id),
            (None, None) => None,
        }
    }

    /// Changes the placement of the shown or parked surface.
    pub fn reconfigure(
        &mut self,
        host: &mut LayerHost,
        change: impl FnOnce(&mut LayerConfig),
    ) -> Result<(), UiError> {
        match (&mut self.parked, self.live) {
            (Some((config, _)), _) => {
                change(config);
                Ok(())
            }
            (None, Some(id)) => host.reconfigure(id, change),
            (None, None) => Ok(()),
        }
    }

    /// Creates the layer surface from the parked document; returns its new id.
    ///
    /// On failure the document is gone (the host consumed it) and the slot stays hidden.
    pub fn show(&mut self, host: &mut LayerHost) -> Result<Option<SurfaceId>, UiError> {
        let Some((config, ui)) = self.parked.take() else {
            return Ok(self.live);
        };
        let id = host.add_surface(config, ui)?;
        self.live = Some(id);
        Ok(Some(id))
    }

    /// Destroys the layer surface and keeps the document.
    pub fn hide(&mut self, host: &mut LayerHost) {
        let Some(id) = self.live.take() else {
            return;
        };
        self.parked = host.take_surface(id);
    }
}
