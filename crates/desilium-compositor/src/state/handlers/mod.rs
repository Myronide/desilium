//! Wayland protocol handlers and client bookkeeping.
mod activation;
mod compositor;
mod decoration_handler;
mod dmabuf;
mod idle;
mod layer_shell;
mod listener;
mod protocols;
mod seat;
mod xdg_shell;

pub use protocols::ProtocolStates;

pub use listener::{ClientState, init as init_listener};
