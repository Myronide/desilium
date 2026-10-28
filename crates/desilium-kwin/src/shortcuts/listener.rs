//! Receives the presses of Desilium's global shortcuts.
//!
//! `kglobalacceld` broadcasts `globalShortcutPressed` on the component's object. The listener
//! subscribes by interface and member only, so it does not depend on how the daemon spells the
//! object path, and keeps the signals whose component is `desilium`.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

use zbus::{
    MatchRule,
    blocking::{Connection, MessageIterator},
    message::Type,
};

use super::COMPONENT;
use crate::error::KwinError;

const INTERFACE: &str = "org.kde.kglobalaccel.Component";
const MEMBER: &str = "globalShortcutPressed";
const THREAD_NAME: &str = "desilium-shortcuts";

/// A Desilium shortcut was pressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutEvent {
    /// Unique action name, one of the constants in [`crate::shortcuts`].
    pub action: String,
}

/// A background thread that calls a callback for every press of a Desilium shortcut.
pub struct ShortcutListener {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

impl ShortcutListener {
    /// Subscribes on the session bus and starts the thread.
    ///
    /// The thread blocks on the bus, so [`shutdown`](Self::shutdown) takes effect at the next
    /// signal at the latest; the process ending also ends it.
    pub fn spawn(on_event: impl Fn(ShortcutEvent) + Send + 'static) -> Result<Self, KwinError> {
        let connection = Connection::session()?;
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .interface(INTERFACE)?
            .member(MEMBER)?
            .build();
        let messages = MessageIterator::for_match_rule(rule, &connection, None)?;
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                // The connection must outlive the iterator.
                let _connection = connection;
                for message in messages {
                    if flag.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(message) = message else {
                        continue;
                    };
                    let body = message.body();
                    let Ok((component, action, _timestamp)) =
                        body.deserialize::<(String, String, i64)>()
                    else {
                        continue;
                    };
                    if component == COMPONENT {
                        on_event(ShortcutEvent { action });
                    }
                }
            })
            .map_err(KwinError::Io)?;
        Ok(Self { stop, thread })
    }

    /// Stops delivering events. Does not wait for the thread, which may sit in a blocking read.
    pub fn shutdown(self) {
        self.stop.store(true, Ordering::Relaxed);
        drop(self.thread);
    }
}
