pub mod auth;
pub mod devices;
pub mod files;
mod operations;
use std::sync::{Arc, Mutex};
type ActiveJob = Arc<Mutex<Option<gio::Cancellable>>>;
use crate::{
    location::Location,
    model::{Device, Entry},
};
use gio::prelude::*;
pub type EventSender = calloop::channel::Sender<Event>;

pub enum Command {
    List {
        location: Location,
        generation: u64,
    },
    Devices,
    Mount(String),
    Unmount(String),
    CreateFolder(Location, String),
    Rename(Location, String),
    Trash(Location),
    Transfer {
        source: Location,
        folder: Location,
        cut: bool,
    },
    Open(Location),
}
pub enum Event {
    Listed {
        location: Location,
        generation: u64,
        result: Result<Vec<Entry>, String>,
    },
    Devices(Vec<Device>),
    Navigate(Location),
    Finished(Result<String, String>),
    Authenticate(auth::Challenge),
    Question {
        message: String,
        choices: Vec<String>,
        reply: std::sync::mpsc::SyncSender<Option<i32>>,
    },
    Refresh,
}
pub struct Worker {
    sender: async_channel::Sender<(Command, gio::Cancellable)>,
    active: ActiveJob,
}
impl Worker {
    pub fn spawn(events: EventSender) -> Result<Self, String> {
        let (sender, receiver) = async_channel::bounded(16);
        let active: ActiveJob = Arc::new(Mutex::new(None));
        let current = active.clone();
        std::thread::Builder::new()
            .name("doria-files".into())
            .spawn(move || {
                let context = gio::glib::MainContext::new();
                let result = context.with_thread_default(|| {
                    context.block_on(run(receiver, events.clone(), current))
                });
                if let Err(error) = result {
                    let _ = events.send(Event::Finished(Err(error.to_string())));
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self { sender, active })
    }
    pub fn send(&mut self, command: Command) -> Result<(), String> {
        let cancel = gio::Cancellable::new();
        self.sender
            .try_send((command, cancel.clone()))
            .map_err(|_| {
                "The file service is busy. Try again after the current operation.".to_owned()
            })?;
        Ok(())
    }
    pub fn cancel(&self) {
        if let Ok(active) = self.active.lock()
            && let Some(cancel) = &*active
        {
            cancel.cancel();
        }
    }
}
async fn run(
    receiver: async_channel::Receiver<(Command, gio::Cancellable)>,
    events: EventSender,
    active: ActiveJob,
) {
    let monitor = gio::VolumeMonitor::get();
    watch_devices(&monitor, &events);
    let mut directory_monitor = None;
    while let Ok((command, cancel)) = receiver.recv().await {
        if let Ok(mut active) = active.lock() {
            *active = Some(cancel.clone());
        }
        if let Command::List {
            location,
            generation,
        } = command
        {
            let result = list_mounted(&location, &cancel, &events).await;
            directory_monitor = watch_directory(&location, &events);
            let _ = events.send(Event::Listed {
                location,
                generation,
                result,
            });
            if let Ok(mut active) = active.lock() {
                *active = None;
            }
            continue;
        }
        let result = execute(command, &monitor, &cancel, &events).await;
        if let Ok(mut active) = active.lock() {
            *active = None;
        }
        let _ = events.send(Event::Devices(devices::snapshot(&monitor)));
        if let Some(result) = result {
            let _ = events.send(Event::Finished(result));
        }
    }
    drop(directory_monitor);
}
async fn list_mounted(
    location: &Location,
    cancel: &gio::Cancellable,
    events: &EventSender,
) -> Result<Vec<Entry>, String> {
    match files::list(location, cancel) {
        Ok(entries) => Ok(entries),
        Err(error) if error.matches(gio::IOErrorEnum::NotMounted) => {
            let operation = auth::operation(events.clone());
            operations::location(&gio::File::for_uri(&location.0), &operation, cancel).await?;
            files::list(location, cancel).map_err(friendly)
        }
        Err(error) => Err(friendly(error)),
    }
}
fn friendly(error: gio::glib::Error) -> String {
    if error.matches(gio::IOErrorEnum::NotSupported) {
        return format!(
            "This location is not supported. Install the matching GVfs backend (SMB, SFTP or WebDAV) and try again. {error}"
        );
    }
    error.to_string()
}
async fn execute(
    command: Command,
    monitor: &gio::VolumeMonitor,
    cancel: &gio::Cancellable,
    events: &EventSender,
) -> Option<Result<String, String>> {
    let result = match command {
        Command::Devices => return None,
        Command::Mount(id) => return Some(mount(monitor, &id, events, cancel).await),
        Command::Unmount(id) => return Some(unmount(monitor, &id, events, cancel).await),
        Command::CreateFolder(location, name) => files::create_folder(&location, &name, cancel),
        Command::Rename(location, name) => files::rename(&location, &name, cancel),
        Command::Trash(location) => gio::File::for_uri(&location.0)
            .trash(Some(cancel))
            .map_err(friendly),
        Command::Transfer {
            source,
            folder,
            cut,
        } => files::transfer(&source, &folder, cut, cancel),
        Command::Open(location) => {
            gio::AppInfo::launch_default_for_uri(&location.0, gio::AppLaunchContext::NONE)
                .map_err(friendly)
        }
        Command::List { .. } => return None,
    };
    Some(result.map(|()| "Done.".into()))
}
async fn mount(
    monitor: &gio::VolumeMonitor,
    id: &str,
    events: &EventSender,
    cancel: &gio::Cancellable,
) -> Result<String, String> {
    let volume = monitor
        .volumes()
        .into_iter()
        .find(|volume| devices::volume_id(volume) == id)
        .ok_or("The drive is no longer available.")?;
    let operation = auth::operation(events.clone());
    operations::volume(&volume, &operation, cancel).await?;
    if let Some(mount) = volume.get_mount() {
        let _ = events.send(Event::Navigate(Location(mount.root().uri().into())));
    }
    Ok("Drive connected.".into())
}
async fn unmount(
    monitor: &gio::VolumeMonitor,
    id: &str,
    events: &EventSender,
    cancel: &gio::Cancellable,
) -> Result<String, String> {
    let device = devices::snapshot(monitor)
        .into_iter()
        .find(|device| device.id == id)
        .ok_or("The drive is no longer available.")?;
    let location = device.location.ok_or("The drive is not mounted.")?;
    let mount = monitor
        .mounts()
        .into_iter()
        .find(|mount| mount.root().uri().as_str() == location.0)
        .ok_or("The drive is no longer mounted.")?;
    let operation = auth::operation(events.clone());
    operations::unmount(&mount, &operation, cancel).await?;
    Ok("Drive safely unmounted.".into())
}
fn watch_directory(location: &Location, events: &EventSender) -> Option<gio::FileMonitor> {
    let monitor = gio::File::for_uri(&location.0)
        .monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE)
        .ok()?;
    let sender = events.clone();
    monitor.connect_changed(move |_, _, _, _| {
        let _ = sender.send(Event::Refresh);
    });
    Some(monitor)
}
fn watch_devices(monitor: &gio::VolumeMonitor, events: &EventSender) {
    let sender = events.clone();
    monitor.connect_volume_added(move |monitor, _| {
        let _ = sender.send(Event::Devices(devices::snapshot(monitor)));
    });
    let sender = events.clone();
    monitor.connect_volume_removed(move |monitor, _| {
        let _ = sender.send(Event::Devices(devices::snapshot(monitor)));
    });
    let sender = events.clone();
    monitor.connect_mount_added(move |monitor, _| {
        let _ = sender.send(Event::Devices(devices::snapshot(monitor)));
    });
    let sender = events.clone();
    monitor.connect_mount_removed(move |monitor, _| {
        let _ = sender.send(Event::Devices(devices::snapshot(monitor)));
    });
    let sender = events.clone();
    monitor.connect_mount_changed(move |monitor, _| {
        let _ = sender.send(Event::Devices(devices::snapshot(monitor)));
    });
}
