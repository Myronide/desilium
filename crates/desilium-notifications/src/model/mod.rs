//! Pure notification model: types, state transitions and policy.
//!
//! Nothing in this module touches D-Bus. Time enters as [`Timestamp`]
//! arguments, so every transition is deterministic and unit-testable.

pub mod hints;
pub mod image;
pub mod limits;
pub mod markup;
pub mod policy;
pub mod store;
pub mod view;

use std::num::NonZeroU32;

pub use hints::{HintValue, Hints};
pub use image::{ImageError, RawImage, Rgba};
pub use markup::{Span, Style};
pub use policy::RateLimit;
pub use store::{Config, Model};

/// Identifier of a notification. Zero is reserved by the freedesktop spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NotificationId(NonZeroU32);

impl NotificationId {
    /// Builds an id from a wire value; `None` for the reserved zero.
    pub fn new(raw: u32) -> Option<Self> {
        NonZeroU32::new(raw).map(Self)
    }

    /// The raw wire value (never zero).
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// Urgency levels from the freedesktop spec, ordered from least to most urgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Urgency {
    Low,
    Normal,
    Critical,
}

impl Urgency {
    /// Maps the wire byte; unknown values fall back to `Normal` as the spec asks.
    pub fn from_byte(raw: u8) -> Self {
        match raw {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }

    /// The wire byte for this level.
    pub fn as_byte(self) -> u8 {
        match self {
            Self::Low => 0,
            Self::Normal => 1,
            Self::Critical => 2,
        }
    }
}

/// Why a notification left the screen, using the freedesktop reason codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseReason {
    Expired,
    Dismissed,
    Closed,
    Undefined,
}

impl CloseReason {
    /// The `reason` value of `NotificationClosed`.
    pub fn code(self) -> u32 {
        match self {
            Self::Expired => 1,
            Self::Dismissed => 2,
            Self::Closed => 3,
            Self::Undefined => 4,
        }
    }
}

/// A point in time, passed in by the caller.
///
/// `monotonic_ms` drives expiry and rate limiting; `unix_ms` is only for display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Timestamp {
    pub monotonic_ms: u64,
    pub unix_ms: u64,
}

/// The image shown next to a notification.
#[derive(Debug, Clone, PartialEq)]
pub enum Icon {
    None,
    /// A themed icon name, resolved by the shell.
    Name(String),
    /// A file path or URI-free path to an image.
    Path(String),
    /// Decoded pixels from the `image-data` hint.
    Rgba(Rgba),
}

/// One button of a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

/// A notify request after D-Bus conversion, before sanitising.
#[derive(Debug, Clone, PartialEq)]
pub struct Incoming {
    pub app_name: String,
    pub replaces_id: u32,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    /// Flattened `key, label, key, label, ...` pairs as sent by the client.
    pub actions: Vec<String>,
    pub hints: Hints,
    pub expire_timeout: i32,
}

/// A stored notification in the history.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub id: NotificationId,
    pub app_name: String,
    /// Grouping and rate-limit key: the desktop entry when present, else the app name.
    pub app_key: String,
    /// Plain-text summary with control characters removed.
    pub summary: String,
    pub body: Vec<Span>,
    pub icon: Icon,
    pub actions: Vec<Action>,
    pub urgency: Urgency,
    pub category: Option<String>,
    pub transient: bool,
    pub resident: bool,
    /// Progress in percent, when the client sent the `value` hint.
    pub progress: Option<u8>,
    /// Client-chosen key that makes a notification replace its predecessor.
    pub sync_key: Option<String>,
    pub received: Timestamp,
    /// Monotonic deadline of the toast; `None` when it never auto-expires.
    pub expires_at: Option<u64>,
    pub read: bool,
    /// Whether the notification is currently shown as a toast.
    pub toast: bool,
}

/// A state change the D-Bus layer must publish.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A new history entry was created.
    Added {
        id: NotificationId,
        toast: bool,
    },
    /// An existing entry was replaced or changed (for example marked read).
    Updated {
        id: NotificationId,
        toast: bool,
    },
    /// The entry left the history entirely.
    Removed {
        id: NotificationId,
        reason: CloseReason,
    },
    /// A toast ended but the entry stays in the history (expired toast).
    ToastClosed {
        id: NotificationId,
        reason: CloseReason,
    },
    /// A toast was hidden without ending (stack overflow or do-not-disturb).
    ToastHidden {
        id: NotificationId,
    },
    /// The user invoked one of the notification's actions.
    ActionInvoked {
        id: NotificationId,
        key: String,
    },
    DndChanged(bool),
}

/// Result of a `Notify` call.
#[derive(Debug, Clone, PartialEq)]
pub struct Notified {
    /// The id to return to the client. Throttled notifications still get one.
    pub id: NotificationId,
    pub events: Vec<Event>,
}

/// Errors for requests that name a missing notification or action.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("no notification with id {0}")]
    UnknownNotification(u32),
    #[error("notification {0} has no action named {1:?}")]
    UnknownAction(u32, String),
}
