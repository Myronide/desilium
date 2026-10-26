use smithay::reexports::calloop;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error("event loop failure: {0}")]
    EventLoop(#[from] calloop::Error),
    #[error("i/o failure: {0}")]
    Io(#[from] std::io::Error),
    #[error("cannot open the Wayland socket: {0}")]
    Socket(#[from] smithay::reexports::wayland_server::BindError),
    #[error("cannot create the Wayland display: {0}")]
    Display(#[from] smithay::reexports::wayland_server::backend::InitError),
    #[error("winit backend failure: {0}")]
    Winit(#[from] smithay::backend::winit::Error),
    #[error("headless GPU setup failed: {0}")]
    Gpu(String),
    #[error("cannot write the screenshot: {0}")]
    Screenshot(String),
    #[error("rendering failed: {0}")]
    Render(String),
    #[error("keyboard setup failed: {0}")]
    Keyboard(String),
    #[error("{0}")]
    Usage(String),
}
