//! Protocol globals that only need bookkeeping: primary selection, viewporter, fractional scale
//! and the KDE server-decoration legacy protocol. Their states are grouped in [`ProtocolStates`]
//! so the core state only holds one field for all of them.

use crate::state::DesiliumState;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::delegate_fractional_scale;
use smithay::delegate_kde_decoration;
use smithay::delegate_primary_selection;
use smithay::delegate_viewporter;
use smithay::desktop::Window;
use smithay::reexports::calloop::LoopHandle;
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration::{
    Mode as KdeMode, OrgKdeKwinServerDecoration,
};
use smithay::reexports::wayland_protocols_misc::server_decoration::server::org_kde_kwin_server_decoration_manager::Mode as KdeDefaultMode;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{DisplayHandle, WEnum};
use smithay::wayland::compositor::with_states;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufState, ImportNotifier};
use smithay::wayland::fractional_scale::{
    FractionalScaleHandler, FractionalScaleManagerState, with_fractional_scale,
};
use smithay::wayland::idle_inhibit::IdleInhibitManagerState;
use smithay::wayland::idle_notify::IdleNotifierState;
use smithay::wayland::selection::primary_selection::{PrimarySelectionHandler, PrimarySelectionState};
use smithay::wayland::shell::kde::decoration::{KdeDecorationHandler, KdeDecorationState};
use smithay::wayland::viewporter::ViewporterState;
use smithay::wayland::xdg_activation::XdgActivationState;

/// Lowest fractional scale a client may be told about.
const MIN_OUTPUT_SCALE: f64 = 0.25;
/// Highest fractional scale a client may be told about.
const MAX_OUTPUT_SCALE: f64 = 8.0;
/// Scale used when the output reports nothing usable.
const FALLBACK_OUTPUT_SCALE: f64 = 1.0;

/// A client buffer waiting for import; imported on the render thread that owns the GPU.
pub struct PendingDmabufImport {
    pub dmabuf: Dmabuf,
    pub notifier: ImportNotifier,
}

/// Globals and state of the bookkeeping protocols, owned by [`DesiliumState`].
pub struct ProtocolStates {
    pub(super) primary_selection: PrimarySelectionState,
    // Held only to keep the globals alive.
    _viewporter: ViewporterState,
    _fractional_scale: FractionalScaleManagerState,
    pub(super) activation: XdgActivationState,
    pub(super) kde_decoration: KdeDecorationState,
    pub(super) dmabuf: DmabufState,
    pub(super) dmabuf_global: Option<DmabufGlobal>,
    pub(super) pending_dmabufs: Vec<PendingDmabufImport>,
    pub(super) idle_notifier: IdleNotifierState<DesiliumState>,
    _idle_inhibit: IdleInhibitManagerState,
    pub(super) idle_inhibitors: Vec<WlSurface>,
}

impl ProtocolStates {
    /// Creates every global; the dmabuf global waits for the renderer's formats.
    pub fn new(handle: &LoopHandle<'static, DesiliumState>, dh: &DisplayHandle) -> Self {
        Self {
            primary_selection: PrimarySelectionState::new::<DesiliumState>(dh),
            _viewporter: ViewporterState::new::<DesiliumState>(dh),
            _fractional_scale: FractionalScaleManagerState::new::<DesiliumState>(dh),
            activation: XdgActivationState::new::<DesiliumState>(dh),
            kde_decoration: KdeDecorationState::new::<DesiliumState>(dh, KdeDefaultMode::Server),
            dmabuf: DmabufState::new(),
            dmabuf_global: None,
            pending_dmabufs: Vec::new(),
            idle_notifier: IdleNotifierState::new(dh, handle.clone()),
            _idle_inhibit: IdleInhibitManagerState::new::<DesiliumState>(dh),
            idle_inhibitors: Vec::new(),
        }
    }
}

/// Clamps a fractional scale into the range a client can use; NaN becomes the fallback.
pub fn sanitize_scale(scale: f64) -> f64 {
    if scale.is_nan() {
        return FALLBACK_OUTPUT_SCALE;
    }
    scale.clamp(MIN_OUTPUT_SCALE, MAX_OUTPUT_SCALE)
}

impl DesiliumState {
    /// The fractional scale of the first output, which every new surface is told about.
    pub fn output_scale(&self) -> f64 {
        self.space
            .outputs()
            .next()
            .map_or(FALLBACK_OUTPUT_SCALE, |output| {
                sanitize_scale(output.current_scale().fractional_scale())
            })
    }

    /// Pushes the current output scale to every window's fractional-scale object.
    ///
    /// Called when the output scale changes; popups and subsurfaces keep the value they got
    /// when they were created.
    #[allow(dead_code)] // wired up when the output scale can change at runtime
    pub fn broadcast_output_scale(&mut self) {
        let scale = self.output_scale();
        let surfaces: Vec<WlSurface> = self
            .windows
            .values()
            .filter_map(|window| window.toplevel().map(|top| top.wl_surface().clone()))
            .collect();
        for surface in &surfaces {
            set_preferred_scale(surface, scale);
        }
        self.request_redraw();
    }

    /// Makes the KDE legacy decoration visible to the window manager's chrome logic.
    fn announce_kde_decoration(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
    ) {
        decoration.mode(KdeMode::Server);
        let Some(toplevel) = self.toplevel_of_surface(surface) else {
            return;
        };
        self.set_server_decorated(&toplevel);
        self.sync_space();
    }

    fn toplevel_of_surface(
        &self,
        surface: &WlSurface,
    ) -> Option<smithay::wayland::shell::xdg::ToplevelSurface> {
        let id = self.window_id_for_surface(surface)?;
        self.windows.get(&id).and_then(Window::toplevel).cloned()
    }
}

fn set_preferred_scale(surface: &WlSurface, scale: f64) {
    with_states(surface, |states| {
        with_fractional_scale(states, |fractional| fractional.set_preferred_scale(scale));
    });
}

impl PrimarySelectionHandler for DesiliumState {
    fn primary_selection_state(&self) -> &PrimarySelectionState {
        &self.protocols.primary_selection
    }
}

impl FractionalScaleHandler for DesiliumState {
    fn new_fractional_scale(&mut self, surface: WlSurface) {
        set_preferred_scale(&surface, self.output_scale());
    }
}

impl KdeDecorationHandler for DesiliumState {
    fn kde_decoration_state(&self) -> &KdeDecorationState {
        &self.protocols.kde_decoration
    }

    fn new_decoration(&mut self, surface: &WlSurface, decoration: &OrgKdeKwinServerDecoration) {
        self.announce_kde_decoration(surface, decoration);
    }

    fn request_mode(
        &mut self,
        surface: &WlSurface,
        decoration: &OrgKdeKwinServerDecoration,
        _mode: WEnum<KdeMode>,
    ) {
        // Like xdg-decoration: the compositor always draws the chrome.
        self.announce_kde_decoration(surface, decoration);
    }
}

delegate_primary_selection!(DesiliumState);
delegate_viewporter!(DesiliumState);
delegate_fractional_scale!(DesiliumState);
delegate_kde_decoration!(DesiliumState);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_inside_range_is_kept() {
        assert_eq!(sanitize_scale(1.5), 1.5);
        assert_eq!(sanitize_scale(2.0), 2.0);
    }

    #[test]
    fn scale_outside_range_is_clamped() {
        assert_eq!(sanitize_scale(0.0), MIN_OUTPUT_SCALE);
        assert_eq!(sanitize_scale(-3.0), MIN_OUTPUT_SCALE);
        assert_eq!(sanitize_scale(100.0), MAX_OUTPUT_SCALE);
    }

    #[test]
    fn nan_scale_falls_back() {
        assert_eq!(sanitize_scale(f64::NAN), FALLBACK_OUTPUT_SCALE);
    }
}
