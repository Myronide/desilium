//! Connection setup, shared state and the single timer task that publishes events.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_channel::{Receiver, Sender};
use async_io::Timer;
use futures_lite::future;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, connection::Builder};

use super::freedesktop::{self, Freedesktop};
use super::shell::{self, Shell};
use crate::ServiceError;
use crate::model::{Config, Event, Model, Timestamp};

/// State shared by the D-Bus objects and the timer task.
///
/// Requests mutate the model under a short lock, then queue the resulting events.
/// The timer task drains the queue and emits signals, so no lock is held across an await.
pub struct Shared {
    model: Mutex<Model>,
    outbox: Mutex<Vec<Event>>,
    wake: Sender<()>,
    started: Instant,
}

impl Shared {
    fn new(wake: Sender<()>) -> Self {
        Self {
            model: Mutex::new(Model::new(Config::default())),
            outbox: Mutex::new(Vec::new()),
            wake,
            started: Instant::now(),
        }
    }

    /// The current time: a monotonic clock for deadlines and a wall clock for display.
    pub fn now(&self) -> Timestamp {
        Timestamp {
            monotonic_ms: millis(self.started.elapsed().as_millis()),
            unix_ms: millis(
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis(),
            ),
        }
    }

    /// Runs `operation` against the model while holding its lock.
    pub fn with_model<T>(&self, operation: impl FnOnce(&mut Model) -> T) -> T {
        let mut guard = lock(&self.model);
        operation(&mut guard)
    }

    /// Queues events for publication and wakes the timer task.
    pub fn submit(&self, events: Vec<Event>) {
        if events.is_empty() {
            return;
        }
        lock(&self.outbox).extend(events);
        // A full channel already holds a pending wake-up, so a failed send is harmless.
        let _ = self.wake.try_send(());
    }

    fn drain(&self) -> Vec<Event> {
        std::mem::take(&mut *lock(&self.outbox))
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn millis(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// A running service: the bus connection plus the state it serves.
pub struct Service {
    connection: Connection,
    shared: Arc<Shared>,
    wake: Receiver<()>,
}

impl Service {
    /// Connects to `address`, or to the session bus when `None`, and claims both service names.
    pub async fn connect(address: Option<&str>) -> Result<Self, ServiceError> {
        let (wake_tx, wake_rx) = async_channel::bounded(1);
        let shared = Arc::new(Shared::new(wake_tx));
        let builder = match address {
            Some(address) => Builder::address(address)?,
            None => Builder::session()?,
        };
        let connection = builder
            .name(freedesktop::SERVICE)?
            .name(shell::SERVICE)?
            .serve_at(freedesktop::PATH, Freedesktop::new(shared.clone()))?
            .serve_at(shell::PATH, Shell::new(shared.clone()))?
            .build()
            .await?;
        Ok(Self {
            connection,
            shared,
            wake: wake_rx,
        })
    }

    /// The bus connection, for callers that need to talk to the bus themselves.
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Publishes queued events, ends expired toasts and sleeps until the next deadline or request.
    pub async fn run(self) {
        loop {
            let now = self.shared.now();
            let mut events = self.shared.with_model(|model| model.tick(now));
            events.extend(self.shared.drain());
            dispatch(&self.connection, &events).await;
            let deadline = self.shared.with_model(|model| model.next_expiry());
            self.sleep(deadline).await;
        }
    }

    /// Waits for a request or for the next toast deadline, whichever comes first.
    async fn sleep(&self, deadline: Option<u64>) {
        let wake = async {
            let _ = self.wake.recv().await;
        };
        let Some(at) = deadline else {
            return wake.await;
        };
        let remaining = at.saturating_sub(self.shared.now().monotonic_ms);
        let timer = async {
            let _ = Timer::after(Duration::from_millis(remaining)).await;
        };
        future::or(timer, wake).await;
    }
}

async fn dispatch(connection: &Connection, events: &[Event]) {
    for event in events {
        if let Err(error) = emit(connection, event).await {
            tracing::warn!(%error, "could not publish a notification event");
        }
    }
}

async fn emit(connection: &Connection, event: &Event) -> zbus::Result<()> {
    let shell = SignalEmitter::new(connection, shell::PATH)?;
    let freedesktop = SignalEmitter::new(connection, freedesktop::PATH)?;
    match event {
        Event::Added { id, .. } => Shell::added(&shell, id.get()).await,
        Event::Updated { id, .. } => Shell::updated(&shell, id.get()).await,
        Event::Removed { id, reason } => {
            Freedesktop::notification_closed(&freedesktop, id.get(), reason.code()).await?;
            Shell::removed(&shell, id.get()).await
        }
        Event::ToastClosed { id, reason } => {
            Freedesktop::notification_closed(&freedesktop, id.get(), reason.code()).await?;
            Shell::toast_closed(&shell, id.get()).await
        }
        Event::ToastHidden { id } => Shell::toast_hidden(&shell, id.get()).await,
        Event::ActionInvoked { id, key } => {
            Freedesktop::action_invoked(&freedesktop, id.get(), key).await
        }
        Event::DndChanged(enabled) => Shell::dnd_changed(&shell, *enabled).await,
    }
}
