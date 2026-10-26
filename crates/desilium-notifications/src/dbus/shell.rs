//! `org.desilium.Notifications1`, the interface the shell uses for toasts and the center.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Type;
use zbus::{fdo, interface};

use super::service::Shared;
use crate::model::view::group_by_app;
use crate::model::{CloseReason, Icon, Notification, NotificationId, Urgency};

pub const SERVICE: &str = "org.desilium.Notifications1";
pub const PATH: &str = "/org/desilium/Notifications1";

/// A styled run of body text.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WireSpan {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Empty when the span is not a link.
    pub link: String,
}

/// One button of a notification.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WireAction {
    pub key: String,
    pub label: String,
}

/// A history entry as the shell renders it.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WireNotification {
    pub id: u32,
    pub app_key: String,
    pub app_name: String,
    pub summary: String,
    pub body: Vec<WireSpan>,
    /// `none`, `name:<theme name>`, `path:<file path>` or `rgba` (fetch with `IconPixels`).
    pub icon: String,
    pub actions: Vec<WireAction>,
    /// 0 low, 1 normal, 2 critical.
    pub urgency: u8,
    /// Unix time in milliseconds.
    pub received_unix_ms: u64,
    /// Percent, or -1 when the client sent no progress.
    pub progress: i32,
    pub read: bool,
    pub toast: bool,
}

/// Notifications of one application, newest first.
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct WireGroup {
    pub app_key: String,
    pub app_name: String,
    pub unread: u32,
    pub notifications: Vec<WireNotification>,
}

/// The shell-facing object.
pub struct Shell {
    shared: Arc<Shared>,
}

impl Shell {
    pub fn new(shared: Arc<Shared>) -> Self {
        Self { shared }
    }
}

#[interface(name = "org.desilium.Notifications1")]
impl Shell {
    /// Do-not-disturb: non-critical notifications go to the history without a toast.
    #[zbus(property)]
    fn do_not_disturb(&self) -> bool {
        self.shared.with_model(|model| model.do_not_disturb())
    }

    #[zbus(property)]
    fn set_do_not_disturb(&self, value: bool) {
        let events = self
            .shared
            .with_model(|model| model.set_do_not_disturb(value));
        self.shared.submit(events);
    }

    /// The whole history, newest first.
    fn list(&self) -> Vec<WireNotification> {
        self.shared
            .with_model(|model| model.history().map(wire_notification).collect())
    }

    /// The history grouped by application, most recently active application first.
    fn list_groups(&self) -> Vec<WireGroup> {
        self.shared.with_model(|model| {
            group_by_app(model.history())
                .into_iter()
                .map(|group| WireGroup {
                    app_key: group.app_key.to_string(),
                    app_name: group.app_name.to_string(),
                    unread: u32::try_from(group.unread).unwrap_or(u32::MAX),
                    notifications: group.items.into_iter().map(wire_notification).collect(),
                })
                .collect()
        })
    }

    /// The toast stack, newest first; the shell renders these and nothing else.
    fn active_toasts(&self) -> Vec<WireNotification> {
        self.shared.with_model(|model| {
            model
                .active_toasts()
                .into_iter()
                .map(wire_notification)
                .collect()
        })
    }

    fn dismiss(&self, id: u32) {
        let Some(id) = NotificationId::new(id) else {
            return;
        };
        let events = self
            .shared
            .with_model(|model| model.close(id, CloseReason::Dismissed));
        self.shared.submit(events);
    }

    fn dismiss_all(&self) {
        let events = self.shared.with_model(|model| model.dismiss_all());
        self.shared.submit(events);
    }

    fn invoke_action(&self, id: u32, key: &str) -> fdo::Result<()> {
        let id = NotificationId::new(id)
            .ok_or_else(|| fdo::Error::InvalidArgs("notification id must be non-zero".into()))?;
        let events = self
            .shared
            .with_model(|model| model.invoke_action(id, key))
            .map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;
        self.shared.submit(events);
        Ok(())
    }

    fn mark_read(&self, id: u32) {
        let Some(id) = NotificationId::new(id) else {
            return;
        };
        let events = self.shared.with_model(|model| model.mark_read(id));
        self.shared.submit(events);
    }

    /// Decoded RGBA pixels for an entry whose icon reference is `rgba`: width, height, pixels.
    fn icon_pixels(&self, id: u32) -> fdo::Result<(u32, u32, Vec<u8>)> {
        let found = NotificationId::new(id).and_then(|id| {
            self.shared
                .with_model(|model| model.get(id).map(|item| item.icon.clone()))
        });
        match found {
            Some(Icon::Rgba(rgba)) => Ok((rgba.width, rgba.height, rgba.pixels)),
            _ => Err(fdo::Error::InvalidArgs(format!(
                "notification {id} has no inline image"
            ))),
        }
    }

    /// A notification entered the history.
    #[zbus(signal)]
    pub async fn added(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    /// A notification was replaced or changed (including read state).
    #[zbus(signal)]
    pub async fn updated(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    /// A notification left the history; drop it from the toast stack too.
    #[zbus(signal)]
    pub async fn removed(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    /// A toast ended (expired) but the notification stays in the history.
    #[zbus(signal)]
    pub async fn toast_closed(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    /// A toast was hidden: stack overflow or do-not-disturb. The entry stays in the history.
    #[zbus(signal)]
    pub async fn toast_hidden(emitter: &SignalEmitter<'_>, id: u32) -> zbus::Result<()>;

    /// Do-not-disturb was switched on or off.
    #[zbus(signal)]
    pub async fn dnd_changed(emitter: &SignalEmitter<'_>, enabled: bool) -> zbus::Result<()>;
}

fn wire_notification(item: &Notification) -> WireNotification {
    WireNotification {
        id: item.id.get(),
        app_key: item.app_key.clone(),
        app_name: item.app_name.clone(),
        summary: item.summary.clone(),
        body: item
            .body
            .iter()
            .map(|span| WireSpan {
                text: span.text.clone(),
                bold: span.style.bold,
                italic: span.style.italic,
                underline: span.style.underline,
                link: span.link.clone().unwrap_or_default(),
            })
            .collect(),
        icon: icon_ref(&item.icon),
        actions: item
            .actions
            .iter()
            .map(|action| WireAction {
                key: action.key.clone(),
                label: action.label.clone(),
            })
            .collect(),
        urgency: match item.urgency {
            Urgency::Low => 0,
            Urgency::Normal => 1,
            Urgency::Critical => 2,
        },
        received_unix_ms: item.received.unix_ms,
        progress: item.progress.map_or(-1, i32::from),
        read: item.read,
        toast: item.toast,
    }
}

fn icon_ref(icon: &Icon) -> String {
    match icon {
        Icon::None => "none".to_string(),
        Icon::Name(name) => format!("name:{name}"),
        Icon::Path(path) => format!("path:{path}"),
        Icon::Rgba(_) => "rgba".to_string(),
    }
}
