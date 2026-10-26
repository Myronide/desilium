//! zwp_linux_dmabuf_v1: clients share GPU buffers (video, games, Vulkan, browsers).
//!
//! Imports need the renderer, which lives in the backend. The handler therefore queues each
//! buffer and the backend drains the queue at the start of every frame.

use super::protocols::PendingDmabufImport;
use crate::state::DesiliumState;
use smithay::backend::allocator::Format;
use smithay::backend::allocator::dmabuf::Dmabuf;
use smithay::backend::renderer::ImportDma;
use smithay::delegate_dmabuf;
use smithay::wayland::dmabuf::{DmabufGlobal, DmabufHandler, DmabufState, ImportNotifier};
use std::fmt::Display;
use tracing::warn;

impl DesiliumState {
    /// Advertises the renderer's buffer formats to clients. Call once after the renderer exists.
    pub fn init_dmabuf_global(&mut self, formats: impl IntoIterator<Item = Format>) {
        let global = self
            .protocols
            .dmabuf
            .create_global::<Self>(&self.display_handle, formats);
        self.protocols.dmabuf_global = Some(global);
    }

    /// Imports the buffers clients queued since the last frame; failures notify the client.
    pub fn import_pending_dmabufs<R>(&mut self, renderer: &mut R)
    where
        R: ImportDma,
        R::Error: Display,
    {
        let pending = std::mem::take(&mut self.protocols.pending_dmabufs);
        for PendingDmabufImport { dmabuf, notifier } in pending {
            match renderer.import_dmabuf(&dmabuf, None) {
                Ok(_) => {
                    // The client already holds the buffer; a dead client is not our error.
                    let _ = notifier.successful::<Self>();
                }
                Err(error) => {
                    warn!(%error, "dmabuf import failed");
                    notifier.failed();
                }
            }
        }
    }
}

impl DmabufHandler for DesiliumState {
    fn dmabuf_state(&mut self) -> &mut DmabufState {
        &mut self.protocols.dmabuf
    }

    fn dmabuf_imported(
        &mut self,
        _global: &DmabufGlobal,
        dmabuf: Dmabuf,
        notifier: ImportNotifier,
    ) {
        self.protocols
            .pending_dmabufs
            .push(PendingDmabufImport { dmabuf, notifier });
        self.request_redraw();
    }
}

delegate_dmabuf!(DesiliumState);
