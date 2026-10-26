//! xdg-activation-v1: a client may raise another window only with evidence of user intent.

use crate::state::DesiliumState;
use smithay::delegate_xdg_activation;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ClientId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::xdg_activation::{
    XdgActivationHandler, XdgActivationState, XdgActivationToken, XdgActivationTokenData,
};
use std::time::Duration;
use tracing::info;

/// How old a token backed by an input serial may be and still activate a window.
const INPUT_TOKEN_MAX_AGE: Duration = Duration::from_secs(10);

/// Whether a window activation request is honoured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationVerdict {
    Activate,
    Ignore,
}

/// The facts the focus-stealing policy decides on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActivationEvidence {
    /// The token was created with a seat serial from an input or focus event.
    pub has_input_serial: bool,
    /// Time since the token was created.
    pub token_age: Duration,
    /// The requesting client currently holds keyboard focus.
    pub from_focused_client: bool,
}

/// Honours requests from the focused application and recent user input; ignores the rest.
pub fn judge_activation(evidence: ActivationEvidence) -> ActivationVerdict {
    if evidence.from_focused_client {
        return ActivationVerdict::Activate;
    }
    let fresh_input = evidence.has_input_serial && evidence.token_age <= INPUT_TOKEN_MAX_AGE;
    if fresh_input {
        ActivationVerdict::Activate
    } else {
        ActivationVerdict::Ignore
    }
}

impl XdgActivationHandler for DesiliumState {
    fn activation_state(&mut self) -> &mut XdgActivationState {
        &mut self.protocols.activation
    }

    fn request_activation(
        &mut self,
        token: XdgActivationToken,
        token_data: XdgActivationTokenData,
        surface: WlSurface,
    ) {
        // Tokens are single-use; drop it whatever the verdict.
        self.protocols.activation.remove_token(&token);
        let evidence = self.activation_evidence(&token_data);
        if judge_activation(evidence) == ActivationVerdict::Ignore {
            info!("window activation ignored: no recent user input");
            return;
        }
        self.activate_surface(&surface);
    }
}

impl DesiliumState {
    fn activation_evidence(&self, token: &XdgActivationTokenData) -> ActivationEvidence {
        ActivationEvidence {
            has_input_serial: token.serial.is_some(),
            token_age: token.timestamp.elapsed(),
            from_focused_client: self.is_keyboard_focused_client(token.client_id.as_ref()),
        }
    }

    fn is_keyboard_focused_client(&self, requester: Option<&ClientId>) -> bool {
        let Some(requester) = requester else {
            return false;
        };
        let Some(keyboard) = self.seat.get_keyboard() else {
            return false;
        };
        let Some(focus) = keyboard.current_focus() else {
            return false;
        };
        self.display_handle
            .get_client(focus.id())
            .is_ok_and(|client| client.id() == *requester)
    }

    /// Focuses and raises the window owning `surface`, the same way a click does.
    fn activate_surface(&mut self, surface: &WlSurface) {
        let Some(id) = self.window_id_for_surface(surface) else {
            return;
        };
        if self.wm.focus(id) {
            self.sync_space();
        }
        self.apply_focus();
    }
}

delegate_xdg_activation!(DesiliumState);

#[cfg(test)]
mod tests {
    use super::*;

    fn evidence(serial: bool, age_secs: u64, focused: bool) -> ActivationEvidence {
        ActivationEvidence {
            has_input_serial: serial,
            token_age: Duration::from_secs(age_secs),
            from_focused_client: focused,
        }
    }

    #[test]
    fn focused_client_is_always_honoured() {
        assert_eq!(
            judge_activation(evidence(false, 3600, true)),
            ActivationVerdict::Activate
        );
    }

    #[test]
    fn fresh_input_serial_is_honoured() {
        assert_eq!(
            judge_activation(evidence(true, 1, false)),
            ActivationVerdict::Activate
        );
    }

    #[test]
    fn input_serial_at_the_age_limit_is_still_honoured() {
        let limit = INPUT_TOKEN_MAX_AGE.as_secs();
        assert_eq!(
            judge_activation(evidence(true, limit, false)),
            ActivationVerdict::Activate
        );
    }

    #[test]
    fn stale_input_serial_is_ignored() {
        let stale = INPUT_TOKEN_MAX_AGE.as_secs() + 1;
        assert_eq!(
            judge_activation(evidence(true, stale, false)),
            ActivationVerdict::Ignore
        );
    }

    #[test]
    fn token_without_input_or_focus_is_ignored() {
        assert_eq!(
            judge_activation(evidence(false, 0, false)),
            ActivationVerdict::Ignore
        );
    }
}
