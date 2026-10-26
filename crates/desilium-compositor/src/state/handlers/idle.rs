//! Idle detection for clients: ext-idle-notify and idle inhibition (video players, presentations).

use crate::state::DesiliumState;
use smithay::delegate_idle_inhibit;
use smithay::delegate_idle_notify;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::idle_inhibit::IdleInhibitHandler;
use smithay::wayland::idle_notify::{IdleNotifierHandler, IdleNotifierState};

impl DesiliumState {
    /// Records user input so idle timers restart. Called for every input event.
    pub fn note_activity(&mut self) {
        let seat = self.seat.clone();
        self.protocols.idle_notifier.notify_activity(&seat);
    }

    /// Idle timers pause while any surface holds an inhibitor.
    fn sync_idle_inhibition(&mut self) {
        let inhibited = !self.protocols.idle_inhibitors.is_empty();
        self.protocols.idle_notifier.set_is_inhibited(inhibited);
    }
}

impl IdleNotifierHandler for DesiliumState {
    fn idle_notifier_state(&mut self) -> &mut IdleNotifierState<Self> {
        &mut self.protocols.idle_notifier
    }
}

impl IdleInhibitHandler for DesiliumState {
    fn inhibit(&mut self, surface: WlSurface) {
        self.protocols.idle_inhibitors.push(surface);
        self.sync_idle_inhibition();
    }

    fn uninhibit(&mut self, surface: WlSurface) {
        self.protocols
            .idle_inhibitors
            .retain(|held| *held != surface);
        self.sync_idle_inhibition();
    }
}

delegate_idle_notify!(DesiliumState);
delegate_idle_inhibit!(DesiliumState);
