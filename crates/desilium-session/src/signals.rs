//! Turns process signals into `SupervisorEvent`s, so the supervisor can block instead of polling.

use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};

use signal_hook::consts::{SIGCHLD, SIGINT, SIGTERM};
use signal_hook::iterator::{Handle, Signals};

use crate::error::SessionError;
use crate::supervisor::SupervisorEvent;

const LISTENER_THREAD_NAME: &str = "session-signals";

/// Forwards SIGCHLD and termination signals into a channel until dropped.
pub struct SignalListener {
    handle: Handle,
    thread: Option<JoinHandle<()>>,
}

impl SignalListener {
    /// Installs the handlers and starts the forwarding thread.
    pub fn start(events: Sender<SupervisorEvent>) -> Result<Self, SessionError> {
        let mut signals =
            Signals::new([SIGCHLD, SIGTERM, SIGINT]).map_err(SessionError::Signals)?;
        let handle = signals.handle();
        let thread = thread::Builder::new()
            .name(LISTENER_THREAD_NAME.to_owned())
            .spawn(move || forward(&mut signals, &events))
            .map_err(SessionError::Signals)?;
        Ok(Self {
            handle,
            thread: Some(thread),
        })
    }
}

fn forward(signals: &mut Signals, events: &Sender<SupervisorEvent>) {
    for signal in signals.forever() {
        let event = match signal {
            SIGCHLD => SupervisorEvent::ChildChanged,
            _ => SupervisorEvent::ShutdownRequested,
        };
        if events.send(event).is_err() {
            break;
        }
    }
}

impl Drop for SignalListener {
    fn drop(&mut self) {
        self.handle.close();
        // The thread only forwards events; a panic in it has no recovery path during drop.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
