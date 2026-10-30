use crate::{
    location::{Location, valid_name},
    model::Entry,
};
use gio::prelude::*;

const ATTRIBUTES: &str = "standard::name,standard::display-name,standard::type,standard::size,standard::is-hidden,time::modified,standard::content-type,standard::target-uri";

pub fn list(
    location: &Location,
    cancel: &gio::Cancellable,
) -> Result<Vec<Entry>, gio::glib::Error> {
    let folder = gio::File::for_uri(&location.0);
    let children =
        folder.enumerate_children(ATTRIBUTES, gio::FileQueryInfoFlags::NONE, Some(cancel))?;
    let mut entries = Vec::new();
    
    while let Some(info) = children.next_file(Some(cancel))? {
        let child = folder.child(info.name());
        entries.push(Entry {
            name: info.display_name().into(),
            location: Location(
                info.attribute_string("standard::target-uri")
                    .map_or_else(|| child.uri().into(), Into::into),
            ),
            directory: matches!(
                info.file_type(),
                gio::FileType::Directory | gio::FileType::Mountable | gio::FileType::Shortcut
            ),
            hidden: info.is_hidden(),
            size: info.size().max(0) as u64,
            modified: info.attribute_uint64("time::modified"),
            kind: info.content_type().map_or_else(
                || "File".into(),
                |kind| gio::content_type_get_description(&kind).into(),
            ),
        });
    }
    
    if let (_, Some(error)) = children.close(Some(cancel)) {
        return Err(error);
    }
    
    Ok(entries)
}

pub fn create_folder(
    location: &Location,
    name: &str,
    cancel: &gio::Cancellable,
) -> Result<(), String> {
    valid_name(name)?;
    gio::File::for_uri(&location.0)
        .child(name)
        .make_directory(Some(cancel))
        .map_err(|error| error.to_string())
}

pub fn rename(location: &Location, name: &str, cancel: &gio::Cancellable) -> Result<(), String> {
    valid_name(name)?;
    gio::File::for_uri(&location.0)
        .set_display_name(name, Some(cancel))
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// No overwrite and no symlink traversal. A partial copy is retained and reported on failure.
pub fn transfer(
    source: &Location,
    folder: &Location,
    cut: bool,
    cancel: &gio::Cancellable,
) -> Result<(), String> {
    let source = gio::File::for_uri(&source.0);
    let target_folder = gio::File::for_uri(&folder.0);
    if source == target_folder || target_folder.has_prefix(&source) {
        return Err("A folder cannot be copied into itself.".into());
    }
    let name = source.basename().ok_or("This location has no file name.")?;
    let target = target_folder.child(name);
    
    if cut {
        return source
            .move_(
                &target,
                gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                Some(cancel),
                None,
            )
            .map_err(|error| error.to_string());
    }
    
    copy_tree(&source, &target, cancel, 0)
        .map_err(|error| format!("Copy stopped: {error}. Files already copied were retained."))
}
fn copy_tree(
    source: &gio::File,
    target: &gio::File,
    cancel: &gio::Cancellable,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 {
        return Err("The folder tree is too deep.".into());
    }
    
    let kind = source.query_file_type(gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, Some(cancel));
    if kind != gio::FileType::Directory {
        return source
            .copy(
                target,
                gio::FileCopyFlags::NOFOLLOW_SYMLINKS,
                Some(cancel),
                None,
            )
            .map_err(|error| error.to_string());
    }
    
    target
        .make_directory(Some(cancel))
        .map_err(|error| error.to_string())?;
    let children = source
        .enumerate_children(
            "standard::name",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            Some(cancel),
        )
        .map_err(|error| error.to_string())?;
    
    while let Some(info) = children
        .next_file(Some(cancel))
        .map_err(|error| error.to_string())?
    {
        copy_tree(
            &source.child(info.name()),
            &target.child(info.name()),
            cancel,
            depth + 1,
        )?;
    }
    
    if let (_, Some(error)) = children.close(Some(cancel)) {
        return Err(error.to_string());
    }
    
    Ok(())
}
