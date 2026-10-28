//! Mount futures wired to the same cancellable as ordinary file jobs.
use gio::prelude::*;
async fn result(
    receiver: async_channel::Receiver<Result<(), gio::glib::Error>>,
) -> Result<(), String> {
    receiver
        .recv()
        .await
        .map_err(|_| "The mount service disconnected.".to_owned())?
        .map_err(super::friendly)
}
pub(super) async fn location(
    file: &gio::File,
    operation: &gio::MountOperation,
    cancel: &gio::Cancellable,
) -> Result<(), String> {
    let (sender, receiver) = async_channel::bounded(1);
    file.mount_enclosing_volume(
        gio::MountMountFlags::NONE,
        Some(operation),
        Some(cancel),
        move |value| {
            let _ = sender.try_send(value);
        },
    );
    result(receiver).await
}
pub(super) async fn volume(
    volume: &gio::Volume,
    operation: &gio::MountOperation,
    cancel: &gio::Cancellable,
) -> Result<(), String> {
    let (sender, receiver) = async_channel::bounded(1);
    volume.mount(
        gio::MountMountFlags::NONE,
        Some(operation),
        Some(cancel),
        move |value| {
            let _ = sender.try_send(value);
        },
    );
    result(receiver).await
}
pub(super) async fn unmount(
    mount: &gio::Mount,
    operation: &gio::MountOperation,
    cancel: &gio::Cancellable,
) -> Result<(), String> {
    let (sender, receiver) = async_channel::bounded(1);
    mount.unmount_with_operation(
        gio::MountUnmountFlags::NONE,
        Some(operation),
        Some(cancel),
        move |value| {
            let _ = sender.try_send(value);
        },
    );
    result(receiver).await
}
