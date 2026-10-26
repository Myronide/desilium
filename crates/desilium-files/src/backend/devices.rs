use crate::{location::Location, model::Device};
use gio::prelude::*;

pub fn volume_id(volume: &gio::Volume) -> String {
    volume
        .identifier("unix-device")
        .or_else(|| volume.uuid())
        .map_or_else(|| format!("volume:{}", volume.name()), Into::into)
}
pub fn snapshot(monitor: &gio::VolumeMonitor) -> Vec<Device> {
    let mut devices = Vec::new();
    for volume in monitor.volumes() {
        let mount = volume.get_mount();
        devices.push(Device {
            id: volume_id(&volume),
            name: volume.name().into(),
            location: mount
                .as_ref()
                .map(|mount| Location(mount.root().uri().into())),
            space: mount.as_ref().and_then(|mount| space(&mount.root())),
            can_mount: volume.can_mount(),
            can_unmount: mount.is_some_and(|mount| mount.can_unmount()),
        });
    }
    for mount in monitor.mounts() {
        let location = Location(mount.root().uri().into());
        if mount.is_shadowed()
            || devices
                .iter()
                .any(|device| device.location.as_ref() == Some(&location))
        {
            continue;
        }
        devices.push(Device {
            space: space(&mount.root()),
            id: location.0.clone(),
            name: mount.name().into(),
            location: Some(location),
            can_mount: false,
            can_unmount: mount.can_unmount(),
        });
    }
    devices.sort_by_key(|device| device.name.to_lowercase());
    devices
}

fn space(file: &gio::File) -> Option<(u64, u64)> {
    file.path()?;
    let info = file
        .query_filesystem_info("filesystem::size,filesystem::free", gio::Cancellable::NONE)
        .ok()?;
    Some((
        info.attribute_uint64("filesystem::size"),
        info.attribute_uint64("filesystem::free"),
    ))
}
