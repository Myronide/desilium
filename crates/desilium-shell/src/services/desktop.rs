//! The desktop folder on disk: reading it, and the few changes the desktop makes to it.
//!
//! Every change takes a file *name* inside the desktop folder, never a path, so nothing outside
//! it can be touched. Removing goes through `gio trash`, with fixed arguments and a time limit.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read, Write},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use desilium_apps::{
    DesktopEntry, DesktopId,
    desktop::ini::{self, IniFile},
};
use serde::{Deserialize, Serialize};

use crate::model::desktop::{self, DeskItem, Kind, MAX_ITEMS, NameError};

const USER_DIRS_FILE: &str = "user-dirs.dirs";
const DESKTOP_KEY: &str = "XDG_DESKTOP_DIR";
const DEFAULT_DESKTOP_NAME: &str = "Desktop";
const NEW_FOLDER: &str = "New Folder";
const NEW_FILE: &str = "New File";
const NEW_FILE_EXTENSION: &str = ".txt";
const EXECUTABLE_BITS: u32 = 0o111;
const OWNER_EXECUTE: u32 = 0o100;
const LAUNCHER_MODE: u32 = 0o755;
const TRASH_TIMEOUT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(20);
/// How much of `gio`'s error output goes into a message.
const STDERR_LIMIT: u64 = 300;

/// Why a change to the desktop folder failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DeskError {
    /// The new name is not usable.
    #[error(transparent)]
    Name(#[from] NameError),
    /// Something with that name is already there.
    #[error("\"{0}\" already exists")]
    Exists(String),
    /// The item is gone or was never in the desktop folder.
    #[error("\"{0}\" is not on the desktop")]
    Missing(String),
    /// `gio` is not installed, so nothing can be moved to the trash.
    #[error("the trash needs `gio` (glib), which is not installed")]
    NoTrash,
    /// `gio trash` failed.
    #[error("moving to the trash failed: {0}")]
    Trash(String),
    /// The source of a launcher is not a `.desktop` file.
    #[error("only .desktop files can be added to the desktop")]
    NotALauncher,
    /// A file system call failed.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The desktop folder: `XDG_DESKTOP_DIR` of `user-dirs.dirs`, else `~/Desktop`. `None` when the
/// user turned the desktop folder off by pointing it at the home directory.
pub(crate) fn desktop_dir(home: &Path, config_home: &Path) -> Option<PathBuf> {
    match fs::read_to_string(config_home.join(USER_DIRS_FILE)) {
        Ok(text) => parse_user_dirs(&text, home),
        Err(_) => Some(home.join(DEFAULT_DESKTOP_NAME)),
    }
}

fn parse_user_dirs(text: &str, home: &Path) -> Option<PathBuf> {
    let value = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix(DESKTOP_KEY))
        .find_map(|rest| rest.trim_start().strip_prefix('='))
        .map(|value| value.trim().trim_matches('"'));
    let Some(value) = value else {
        return Some(home.join(DEFAULT_DESKTOP_NAME));
    };
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    // The specification: a desktop directory equal to the home directory means "no desktop".
    (path.is_absolute() && path != home).then_some(path)
}

/// Reads the application launcher at `path`; `None` for anything that is not one.
pub(crate) fn load_launcher(path: &Path, locales: &[String]) -> Option<DesktopEntry> {
    let id = DesktopId::new(path.file_name()?.to_str()?)?;
    let text = fs::read(path).ok()?;
    let file: IniFile = ini::parse(&String::from_utf8_lossy(&text)).ok()?;
    DesktopEntry::from_ini(id, path.to_path_buf(), &file, locales).ok()
}

/// The icons on the desktop: folders, launchers and files, hidden ones left out, in display order.
pub(crate) fn scan(dir: &Path, locales: &[String]) -> Vec<DeskItem> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut items: Vec<DeskItem> = entries
        .flatten()
        .filter_map(|entry| item_for(&entry.path(), locales))
        .take(MAX_ITEMS)
        .collect();
    desktop::sort(&mut items);
    items
}

fn item_for(path: &Path, locales: &[String]) -> Option<DeskItem> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    if file_name.starts_with('.') {
        return None;
    }
    let metadata = fs::metadata(path).ok()?;
    let plain = DeskItem {
        path: path.to_path_buf(),
        name: file_name.clone(),
        file_name,
        kind: if metadata.is_dir() {
            Kind::Folder
        } else {
            Kind::File
        },
        icon: None,
    };
    if metadata.is_dir() || !plain.file_name.ends_with(".desktop") {
        return Some(plain);
    }
    let Some(entry) = load_launcher(path, locales) else {
        return Some(plain);
    };
    if entry.hidden {
        return None;
    }
    Some(DeskItem {
        name: entry.name,
        kind: Kind::Launcher {
            trusted: metadata.permissions().mode() & EXECUTABLE_BITS != 0,
        },
        icon: entry.icon,
        ..plain
    })
}

/// Checks that `file_name` names something directly inside `dir` and returns its path.
fn inside(dir: &Path, file_name: &str) -> Result<PathBuf, DeskError> {
    let name = desktop::validate_name(file_name)?;
    let path = dir.join(name);
    // `symlink_metadata`: a dangling symlink is still an item that can be renamed or trashed.
    if fs::symlink_metadata(&path).is_err() {
        return Err(DeskError::Missing(name.to_owned()));
    }
    Ok(path)
}

fn exists(dir: &Path, name: &str) -> bool {
    fs::symlink_metadata(dir.join(name)).is_ok()
}

/// Creates an empty folder and returns its name. The folder of the desktop itself is created
/// when it is missing.
pub(crate) fn create_folder(dir: &Path) -> Result<String, DeskError> {
    fs::create_dir_all(dir)?;
    for _ in 0..=MAX_ITEMS {
        let name = desktop::unique_name(NEW_FOLDER, "", |name| exists(dir, name));
        match fs::create_dir(dir.join(&name)) {
            Ok(()) => return Ok(name),
            // Someone else created it between the check and the call: look for the next name.
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(DeskError::Exists(NEW_FOLDER.to_owned()))
}

/// Creates an empty text file and returns its name.
pub(crate) fn create_file(dir: &Path) -> Result<String, DeskError> {
    fs::create_dir_all(dir)?;
    for _ in 0..=MAX_ITEMS {
        let name = desktop::unique_name(NEW_FILE, NEW_FILE_EXTENSION, |name| exists(dir, name));
        let created = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dir.join(&name));
        match created {
            Ok(_) => return Ok(name),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(DeskError::Exists(NEW_FILE.to_owned()))
}

/// Renames `from` to `to` inside the desktop folder and returns the new name. Never replaces
/// something that exists.
pub(crate) fn rename(dir: &Path, from: &str, to: &str) -> Result<String, DeskError> {
    let source = inside(dir, from)?;
    let name = desktop::validate_name(to)?.to_owned();
    if name == from {
        return Ok(name);
    }
    let target = dir.join(&name);
    if fs::symlink_metadata(&target).is_ok() {
        return Err(DeskError::Exists(name));
    }
    fs::rename(source, target)?;
    Ok(name)
}

/// Marks a launcher as one the user trusts, which is what lets a double click start it.
pub(crate) fn allow_launching(dir: &Path, file_name: &str) -> Result<(), DeskError> {
    let path = inside(dir, file_name)?;
    let mode = fs::metadata(&path)?.permissions().mode();
    fs::set_permissions(&path, fs::Permissions::from_mode(mode | OWNER_EXECUTE))?;
    Ok(())
}

/// Copies the launcher `source` onto the desktop, trusted, and returns its file name there. An
/// existing launcher of that name is left as it is.
pub(crate) fn add_launcher(dir: &Path, source: &Path) -> Result<String, DeskError> {
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.ends_with(".desktop"))
        .ok_or(DeskError::NotALauncher)?
        .to_owned();
    fs::create_dir_all(dir)?;
    let target = dir.join(&name);
    if fs::symlink_metadata(&target).is_ok() {
        return Ok(name);
    }
    fs::copy(source, &target)?;
    fs::set_permissions(&target, fs::Permissions::from_mode(LAUNCHER_MODE))?;
    Ok(name)
}

/// File name of the saved icon positions below `$XDG_STATE_HOME/desilium`.
const LAYOUT_FILE: &str = "desktop-layout.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
struct LayoutFile {
    #[serde(default)]
    cells: BTreeMap<String, (u32, u32)>,
}

/// Where the icon positions are kept for the given state home.
pub(crate) fn layout_path(state_home: &Path) -> PathBuf {
    state_home.join("desilium").join(LAYOUT_FILE)
}

/// The saved cell of every icon; a missing or unreadable file gives an empty layout.
pub(crate) fn load_layout(path: &Path) -> BTreeMap<String, (u32, u32)> {
    let Ok(text) = fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    match toml::from_str::<LayoutFile>(&text) {
        Ok(file) => file.cells.into_iter().take(MAX_ITEMS * 2).collect(),
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "ignoring unreadable desktop layout");
            BTreeMap::new()
        }
    }
}

/// Writes the icon positions atomically (temporary file, then rename).
pub(crate) fn save_layout(path: &Path, cells: &BTreeMap<String, (u32, u32)>) -> io::Result<()> {
    let text = toml::to_string_pretty(&LayoutFile {
        cells: cells.clone(),
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("toml.tmp");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(temporary, path)
}

/// Moves an item to the trash with `gio trash`.
pub(crate) fn trash(dir: &Path, file_name: &str) -> Result<(), DeskError> {
    let path = inside(dir, file_name)?;
    let mut child = Command::new("gio")
        .arg("trash")
        .arg("--")
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| match error.kind() {
            io::ErrorKind::NotFound => DeskError::NoTrash,
            _ => DeskError::Io(error),
        })?;
    let started = Instant::now();
    loop {
        match child.try_wait()? {
            Some(status) if status.success() => return Ok(()),
            Some(_) => return Err(DeskError::Trash(read_stderr(&mut child))),
            None if started.elapsed() >= TRASH_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(DeskError::Trash("gio did not answer in time".to_owned()));
            }
            None => thread::sleep(POLL),
        }
    }
}

fn read_stderr(child: &mut std::process::Child) -> String {
    let mut text = String::new();
    if let Some(stderr) = child.stderr.as_mut() {
        let _ = stderr.take(STDERR_LIMIT).read_to_string(&mut text);
    }
    text.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAUNCHER: &str =
        "[Desktop Entry]\nType=Application\nName=Notes\nExec=notes %U\nIcon=notes\n";

    fn locales() -> Vec<String> {
        vec!["en".to_owned()]
    }

    fn write(dir: &Path, name: &str, text: &str, mode: u32) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, text).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("chmod");
        path
    }

    #[test]
    fn the_desktop_folder_follows_user_dirs_and_falls_back_to_desktop() {
        let home = Path::new("/home/u");
        assert_eq!(
            parse_user_dirs("XDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\n", home),
            Some(PathBuf::from("/home/u/Schreibtisch"))
        );
        assert_eq!(
            parse_user_dirs("XDG_DOWNLOAD_DIR=\"$HOME/Downloads\"\n", home),
            Some(PathBuf::from("/home/u/Desktop")),
            "no entry means the default"
        );
        assert_eq!(
            parse_user_dirs("XDG_DESKTOP_DIR=\"/data/desk\"\n", home),
            Some(PathBuf::from("/data/desk"))
        );
        assert_eq!(
            parse_user_dirs("XDG_DESKTOP_DIR=\"$HOME/\"\n", home),
            None,
            "pointing at the home directory turns the desktop folder off"
        );
        assert_eq!(
            parse_user_dirs("XDG_DESKTOP_DIR=\"relative\"\n", home),
            None
        );
    }

    #[test]
    fn scanning_lists_folders_launchers_and_files_but_no_hidden_ones() {
        let dir = tempfile::tempdir().expect("dir");
        fs::create_dir(dir.path().join("Photos")).expect("mkdir");
        fs::create_dir(dir.path().join(".cache")).expect("mkdir");
        write(dir.path(), "notes.desktop", LAUNCHER, 0o755);
        write(dir.path(), "readme.txt", "x", 0o644);
        write(dir.path(), ".secret", "x", 0o644);
        let items = scan(dir.path(), &locales());
        let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Photos", "Notes", "readme.txt"]);
        assert_eq!(items[0].kind, Kind::Folder);
        assert_eq!(items[1].kind, Kind::Launcher { trusted: true });
        assert_eq!(items[1].icon.as_deref(), Some("notes"));
        assert_eq!(items[1].file_name, "notes.desktop");
        assert_eq!(items[2].kind, Kind::File);
    }

    #[test]
    fn a_launcher_without_the_executable_bit_is_untrusted() {
        let dir = tempfile::tempdir().expect("dir");
        write(dir.path(), "downloaded.desktop", LAUNCHER, 0o644);
        let items = scan(dir.path(), &locales());
        assert_eq!(items[0].kind, Kind::Launcher { trusted: false });
    }

    #[test]
    fn broken_and_hidden_launchers_do_not_break_the_scan() {
        let dir = tempfile::tempdir().expect("dir");
        write(
            dir.path(),
            "broken.desktop",
            "this is not an ini file",
            0o755,
        );
        write(
            dir.path(),
            "gone.desktop",
            &format!("{LAUNCHER}Hidden=true\n"),
            0o755,
        );
        let items = scan(dir.path(), &locales());
        assert_eq!(items.len(), 1, "the hidden launcher is dropped");
        assert_eq!(
            items[0].kind,
            Kind::File,
            "an unreadable one shows as a plain file"
        );
        assert!(scan(Path::new("/nonexistent/desktop"), &locales()).is_empty());
    }

    #[test]
    fn new_folders_and_files_get_free_names() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(create_folder(dir.path()).expect("folder"), "New Folder");
        assert_eq!(create_folder(dir.path()).expect("folder"), "New Folder 2");
        assert_eq!(create_file(dir.path()).expect("file"), "New File.txt");
        assert_eq!(create_file(dir.path()).expect("file"), "New File 2.txt");
        assert!(dir.path().join("New Folder 2").is_dir());
        assert_eq!(
            fs::metadata(dir.path().join("New File.txt"))
                .expect("meta")
                .len(),
            0
        );
    }

    #[test]
    fn a_missing_desktop_folder_is_created_for_new_items() {
        let dir = tempfile::tempdir().expect("dir");
        let desktop = dir.path().join("Desktop");
        assert_eq!(create_folder(&desktop).expect("folder"), "New Folder");
        assert!(desktop.is_dir());
    }

    #[test]
    fn renaming_stays_inside_the_folder_and_never_overwrites() {
        let dir = tempfile::tempdir().expect("dir");
        write(dir.path(), "a.txt", "a", 0o644);
        write(dir.path(), "b.txt", "b", 0o644);
        assert_eq!(
            rename(dir.path(), "a.txt", "  c.txt ").expect("rename"),
            "c.txt"
        );
        assert!(dir.path().join("c.txt").is_file() && !dir.path().join("a.txt").exists());
        assert!(matches!(
            rename(dir.path(), "c.txt", "b.txt"),
            Err(DeskError::Exists(name)) if name == "b.txt"
        ));
        assert_eq!(
            fs::read_to_string(dir.path().join("b.txt")).expect("read"),
            "b"
        );
        assert!(matches!(
            rename(dir.path(), "c.txt", "../c.txt"),
            Err(DeskError::Name(NameError::Slash))
        ));
        assert!(matches!(
            rename(dir.path(), "../a.txt", "x"),
            Err(DeskError::Name(NameError::Slash))
        ));
        assert!(matches!(
            rename(dir.path(), "gone", "x"),
            Err(DeskError::Missing(_))
        ));
        assert_eq!(
            rename(dir.path(), "c.txt", "c.txt").expect("same name"),
            "c.txt"
        );
    }

    #[test]
    fn allowing_a_launcher_sets_the_owner_execute_bit_only() {
        let dir = tempfile::tempdir().expect("dir");
        write(dir.path(), "x.desktop", LAUNCHER, 0o644);
        allow_launching(dir.path(), "x.desktop").expect("allow");
        let mode = fs::metadata(dir.path().join("x.desktop"))
            .expect("meta")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o744);
        assert!(allow_launching(dir.path(), "../x.desktop").is_err());
    }

    #[test]
    fn adding_a_launcher_copies_it_trusted_and_keeps_an_existing_one() {
        let source_dir = tempfile::tempdir().expect("dir");
        let desktop = tempfile::tempdir().expect("dir");
        let source = write(source_dir.path(), "notes.desktop", LAUNCHER, 0o644);
        assert_eq!(
            add_launcher(desktop.path(), &source).expect("add"),
            "notes.desktop"
        );
        let copy = desktop.path().join("notes.desktop");
        assert_eq!(fs::read_to_string(&copy).expect("read"), LAUNCHER);
        assert_eq!(
            fs::metadata(&copy).expect("meta").permissions().mode() & 0o777,
            0o755
        );
        fs::write(&copy, "edited").expect("edit");
        add_launcher(desktop.path(), &source).expect("again");
        assert_eq!(fs::read_to_string(&copy).expect("read"), "edited");
        let text = write(source_dir.path(), "notes.txt", "x", 0o644);
        assert!(matches!(
            add_launcher(desktop.path(), &text),
            Err(DeskError::NotALauncher)
        ));
    }

    #[test]
    fn the_layout_survives_a_round_trip_and_bad_files_are_ignored() {
        let dir = tempfile::tempdir().expect("dir");
        let path = layout_path(dir.path());
        assert!(load_layout(&path).is_empty(), "no file yet");
        let cells = BTreeMap::from([
            ("a.txt".to_owned(), (3, 1)),
            ("Photos \"final\"".to_owned(), (0, 4)),
        ]);
        save_layout(&path, &cells).expect("save");
        assert_eq!(load_layout(&path), cells);
        assert!(
            !path.with_extension("toml.tmp").exists(),
            "the temporary file is renamed away"
        );
        fs::write(&path, "cells = 12 this is not toml [[").expect("write");
        assert!(load_layout(&path).is_empty());
    }

    #[test]
    fn trashing_refuses_names_outside_the_folder_before_running_anything() {
        let dir = tempfile::tempdir().expect("dir");
        assert!(matches!(trash(dir.path(), "../x"), Err(DeskError::Name(_))));
        assert!(matches!(
            trash(dir.path(), "missing"),
            Err(DeskError::Missing(_))
        ));
    }
}
