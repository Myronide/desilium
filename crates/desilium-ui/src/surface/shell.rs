//! Callbacks Blitz uses to talk back to its host, collected for our own use.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};

use blitz_traits::{
    net::{Bytes, NetHandler, NetProvider, Request},
    shell::{ClipboardError, ShellProvider},
};

/// State shared between the document and the [`UiSurface`](crate::UiSurface).
#[derive(Default)]
pub(crate) struct SharedState {
    redraw: AtomicBool,
    ime_enabled: AtomicBool,
    clipboard: Mutex<String>,
}

impl SharedState {
    pub(crate) fn redraw_requested(&self) -> bool {
        self.redraw.load(Ordering::Relaxed)
    }

    pub(crate) fn clear_redraw(&self) {
        self.redraw.store(false, Ordering::Relaxed);
    }

    pub(crate) fn mark_redraw(&self) {
        self.redraw.store(true, Ordering::Relaxed);
    }

    pub(crate) fn ime_enabled(&self) -> bool {
        self.ime_enabled.load(Ordering::Relaxed)
    }

    pub(crate) fn clipboard_text(&self) -> String {
        self.clipboard
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default()
    }
}

impl ShellProvider for SharedState {
    fn request_redraw(&self) {
        self.mark_redraw();
    }

    fn set_ime_enabled(&self, is_enabled: bool) {
        self.ime_enabled.store(is_enabled, Ordering::Relaxed);
    }

    fn get_clipboard_text(&self) -> Result<String, ClipboardError> {
        self.clipboard
            .lock()
            .map(|text| text.clone())
            .map_err(|_| ClipboardError)
    }

    fn set_clipboard_text(&self, text: String) -> Result<(), ClipboardError> {
        let mut slot = self.clipboard.lock().map_err(|_| ClipboardError)?;
        *slot = text;
        Ok(())
    }
}

/// Serves `file://` resources (images, fonts) synchronously from disk.
pub(crate) struct FileProvider;

impl NetProvider for FileProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let Ok(path) = request.url.to_file_path() else {
            tracing::debug!(url = %request.url, "unsupported resource url");
            return;
        };
        match std::fs::read(&path) {
            Ok(bytes) => handler.bytes(request.url.to_string(), Bytes::from(bytes)),
            Err(error) => tracing::warn!(path = %path.display(), %error, "resource read failed"),
        }
    }
}
