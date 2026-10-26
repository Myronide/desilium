//! `org.freedesktop.Notifications` (spec 1.2), the interface applications talk to.

use std::collections::HashMap;
use std::sync::Arc;

use zbus::interface;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;

use super::convert;
use super::service::Shared;
use crate::model::hints;
use crate::model::{CloseReason, Incoming, NotificationId};

pub const SERVICE: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";

/// Capabilities we really implement: plain and markup bodies with links, actions,
/// bitmap icons, and history persistence.
const CAPABILITIES: [&str; 6] = [
    "actions",
    "body",
    "body-markup",
    "body-hyperlinks",
    "icon-static",
    "persistence",
];
const SERVER_NAME: &str = "Desilium";
const VENDOR: &str = "Myronide";
const SPEC_VERSION: &str = "1.2";

/// The freedesktop server object.
pub struct Freedesktop {
    shared: Arc<Shared>,
}

impl Freedesktop {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }
}

#[interface(name = "org.freedesktop.Notifications")]
impl Freedesktop {
    fn get_capabilities(&self) -> Vec<String> {
        CAPABILITIES
            .iter()
            .map(|name| (*name).to_string())
            .collect()
    }

    // The argument list is fixed by the freedesktop spec, so it cannot be grouped.
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: Vec<String>,
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> u32 {
        let incoming = Incoming {
            app_name: app_name.to_string(),
            replaces_id,
            app_icon: app_icon.to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions,
            hints: hints::parse(convert::hint_pairs(&hints)),
            expire_timeout,
        };
        let now = self.shared.now();
        let notified = self.shared.with_model(|model| model.notify(incoming, now));
        self.shared.submit(notified.events);
        notified.id.get()
    }

    fn close_notification(&self, id: u32) {
        let Some(id) = NotificationId::new(id) else {
            return;
        };
        let events = self
            .shared
            .with_model(|model| model.close(id, CloseReason::Closed));
        self.shared.submit(events);
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            SERVER_NAME.to_string(),
            VENDOR.to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            SPEC_VERSION.to_string(),
        )
    }

    /// Emitted when a notification is closed; `reason` uses the spec codes 1-4.
    #[zbus(signal)]
    pub async fn notification_closed(
        emitter: &SignalEmitter<'_>,
        id: u32,
        reason: u32,
    ) -> zbus::Result<()>;

    /// Emitted when the user activates one of the notification's actions.
    #[zbus(signal)]
    pub async fn action_invoked(
        emitter: &SignalEmitter<'_>,
        id: u32,
        action_key: &str,
    ) -> zbus::Result<()>;
}
