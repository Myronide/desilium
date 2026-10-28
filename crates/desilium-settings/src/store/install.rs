//! Unpacking a download safely and putting the theme where applications look for it.
//!
//! A download is untrusted. Before anything is written the archive is listed and refused if it
//! holds an absolute path, a `..` component, a link that leaves the archive, a hard link or a
//! device file. After unpacking, the tree is walked again (links, special files, total size)
//! before anything is moved. A theme directory goes to `~/.local/share/icons/<name>`, a colour
//! scheme to `~/.local/share/color-schemes/<name>.colors`. A directory that exists and was not
//! installed from the same store entry is never touched. Themes are data, not code; nothing is
//! executed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use super::{
    Kind, StoreDirs, StoreError,
    process::{Limits, run},
};

/// Name of the file inside an installed theme directory that records the store entry it came from.
pub const MARKER: &str = ".desilium-store";
/// Most entries an archive may list.
const MAX_ENTRIES: usize = 50_000;
/// Most bytes (and files) a download may unpack to.
const MAX_UNPACKED: u64 = 1 << 30;
/// Longest path or link target accepted.
const MAX_PATH: usize = 1024;
/// Deepest directory nesting walked.
const MAX_DEPTH: usize = 32;
/// Largest plain colour scheme file.
const MAX_SCHEME: u64 = 1 << 20;
/// Longest directory name installed.
const MAX_NAME: usize = 64;
/// How long listing or unpacking may take.
const TOOL_TIME: Duration = Duration::from_secs(180);
/// Longest archive listing read.
const LISTING_MAX: u64 = 16 << 20;
/// Scratch directories older than this are leftovers of an interrupted run.
const STALE_AFTER: Duration = Duration::from_secs(3600);

/// A scratch directory that is removed when dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    /// A new empty directory below `work`.
    pub fn new(work: &Path) -> io::Result<Self> {
        fs::create_dir_all(work)?;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |since| since.subsec_nanos());
        let path = work.join(format!("job-{}-{nanos}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }

    /// The directory.
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Removes scratch directories an earlier, interrupted run left behind.
pub fn sweep(work: &Path) {
    let Ok(entries) = fs::read_dir(work) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_some_and(|age| age > STALE_AFTER);
        if old && entry.file_name().to_string_lossy().starts_with("job-") {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// A theme or scheme directory (file) name that is safe to create: one path component, no
/// leading dot, no control characters, not too long.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.chars().count() <= MAX_NAME
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && !name.chars().any(char::is_control)
}

/// An archive entry path that stays below the extraction directory.
pub fn is_safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PATH
        && !path.starts_with('/')
        && !path.contains(['\\', '\0'])
        && path.split('/').all(|part| part != "..")
}

/// Whether the link `entry -> target` points outside the extraction directory (resolved
/// lexically, from the directory the link is in).
pub fn link_escapes(entry: &str, target: &str) -> bool {
    if target.is_empty()
        || target.len() > MAX_PATH
        || target.starts_with('/')
        || target.contains('\0')
    {
        return true;
    }
    let mut stack: Vec<&str> = entry
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    stack.pop();
    for part in target.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if stack.pop().is_none() {
                    return true;
                }
            }
            other => stack.push(other),
        }
    }
    false
}

/// What an archive lists for one entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// A regular file or a directory.
    Plain,
    /// A symbolic link and its target.
    Symlink(String),
    /// A hard link.
    Hardlink,
    /// A device, pipe or socket.
    Special,
}

/// One listed entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Path inside the archive.
    pub path: String,
    /// What it is.
    pub kind: EntryKind,
}

/// Refuses an archive listing that holds anything unsafe.
pub fn check_entries(entries: &[ArchiveEntry]) -> Result<(), StoreError> {
    if entries.len() > MAX_ENTRIES {
        return Err(StoreError::TooLarge);
    }
    for entry in entries {
        let shown = || entry.path.chars().take(80).collect::<String>();
        if !is_safe_path(&entry.path) {
            return Err(StoreError::Unsafe(shown()));
        }
        match &entry.kind {
            EntryKind::Plain => {}
            EntryKind::Symlink(target) if !link_escapes(&entry.path, target) => {}
            EntryKind::Symlink(_) | EntryKind::Hardlink | EntryKind::Special => {
                return Err(StoreError::Unsafe(shown()));
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Zip,
    Tar,
}

fn sniff(path: &Path) -> io::Result<Format> {
    let mut head = [0u8; 4];
    let read = fs::File::open(path)?.read(&mut head)?;
    Ok(if read == 4 && &head == b"PK\x03\x04" {
        Format::Zip
    } else {
        Format::Tar
    })
}

fn listing_limits() -> Limits {
    Limits {
        timeout: TOOL_TIME,
        max_stdout: LISTING_MAX,
        watch: None,
    }
}

fn os(text: &str) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(text)
}

/// Lists a tar archive twice: names exactly (one per line) and verbosely for the entry types.
/// Both listings must have the same length, or an entry with a line break in its name hides.
fn list_tar(archive: &Path) -> Result<Vec<ArchiveEntry>, StoreError> {
    let names = run("tar", &[os("-tf"), archive.as_os_str()], &listing_limits())?;
    if !names.success {
        return Err(StoreError::NotAnArchive);
    }
    let verbose = run("tar", &[os("-tvf"), archive.as_os_str()], &listing_limits())?;
    if !verbose.success {
        return Err(StoreError::NotAnArchive);
    }
    let names = String::from_utf8_lossy(&names.stdout).into_owned();
    let verbose = String::from_utf8_lossy(&verbose.stdout).into_owned();
    parse_tar_listing(&names, &verbose)
}

/// The pure part of [`list_tar`]: pairs the two listings.
pub fn parse_tar_listing(names: &str, verbose: &str) -> Result<Vec<ArchiveEntry>, StoreError> {
    let names: Vec<&str> = names.lines().collect();
    let verbose: Vec<&str> = verbose.lines().collect();
    if names.len() != verbose.len() {
        return Err(StoreError::Unsafe("a name with a line break".to_owned()));
    }
    Ok(names
        .iter()
        .zip(&verbose)
        .map(|(name, line)| {
            let kind = match line.chars().next() {
                Some('-' | 'd') => EntryKind::Plain,
                Some('l') => match line.rsplit_once(" -> ") {
                    Some((_, target)) => EntryKind::Symlink(target.to_owned()),
                    None => EntryKind::Special,
                },
                Some('h') => EntryKind::Hardlink,
                _ => EntryKind::Special,
            };
            ArchiveEntry {
                path: name.trim_end_matches('/').to_owned(),
                kind,
            }
        })
        .collect())
}

fn list_zip(archive: &Path) -> Result<Vec<ArchiveEntry>, StoreError> {
    let names = run(
        "unzip",
        &[os("-Z1"), archive.as_os_str()],
        &listing_limits(),
    )?;
    if !names.success {
        return Err(StoreError::NotAnArchive);
    }
    Ok(String::from_utf8_lossy(&names.stdout)
        .lines()
        .map(|name| ArchiveEntry {
            path: name.trim_end_matches('/').to_owned(),
            kind: EntryKind::Plain,
        })
        .collect())
}

fn extract(archive: &Path, format: Format, into: &Path) -> Result<(), StoreError> {
    let limits = Limits::quick(TOOL_TIME);
    let done = match format {
        Format::Tar => run(
            "tar",
            &[
                os("-xf"),
                archive.as_os_str(),
                os("-C"),
                into.as_os_str(),
                os("--no-same-owner"),
                os("--no-same-permissions"),
            ],
            &limits,
        )?,
        Format::Zip => run(
            "unzip",
            &[
                os("-qq"),
                os("-o"),
                archive.as_os_str(),
                os("-d"),
                into.as_os_str(),
            ],
            &limits,
        )?,
    };
    if done.success {
        Ok(())
    } else {
        Err(StoreError::NotAnArchive)
    }
}

/// Walks an unpacked tree: no special files, no link out of the tree, bounded size and depth.
/// Returns the total size of the regular files.
pub fn verify_tree(root: &Path) -> Result<u64, StoreError> {
    let mut total = 0u64;
    let mut count = 0usize;
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    while let Some((dir, relative, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(StoreError::Unsafe("nested too deep".to_owned()));
        }
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = if relative.is_empty() {
                name
            } else {
                format!("{relative}/{name}")
            };
            count += 1;
            if count > MAX_ENTRIES {
                return Err(StoreError::TooLarge);
            }
            let meta = fs::symlink_metadata(entry.path())?;
            let kind = meta.file_type();
            if kind.is_symlink() {
                let target = fs::read_link(entry.path())?;
                if link_escapes(&path, &target.to_string_lossy()) {
                    return Err(StoreError::Unsafe(path));
                }
            } else if kind.is_dir() {
                stack.push((entry.path(), path, depth + 1));
            } else if kind.is_file() {
                total += meta.len();
                if total > MAX_UNPACKED {
                    return Err(StoreError::TooLarge);
                }
            } else {
                return Err(StoreError::Unsafe(path));
            }
        }
    }
    Ok(total)
}

fn read_small(path: &Path, limit: u64) -> Option<String> {
    let mut text = String::new();
    fs::File::open(path)
        .ok()?
        .take(limit)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Whether `dir` is an icon theme (an `index.theme` that lists `Directories`).
pub fn is_icon_theme(dir: &Path) -> bool {
    read_small(&dir.join("index.theme"), 1 << 20).is_some_and(|text| {
        text.lines()
            .any(|line| line.trim_start().starts_with("Directories"))
    })
}

/// Whether `dir` is a cursor theme (it has a `cursors` directory).
pub fn is_cursor_theme(dir: &Path) -> bool {
    dir.join("cursors").is_dir()
}

fn real_dirs(parent: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.metadata().is_ok_and(|meta| meta.is_dir()))
        .map(|entry| {
            (
                entry.file_name().to_string_lossy().into_owned(),
                entry.path(),
            )
        })
        .filter(|(name, _)| valid_name(name))
        .collect();
    found.sort();
    found
}

fn scheme_files(parent: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let mut found: Vec<(String, PathBuf)> = entries
        .flatten()
        .filter(|entry| entry.metadata().is_ok_and(|meta| meta.is_file()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".colors")?.to_owned();
            valid_name(&stem).then(|| (stem, entry.path()))
        })
        .collect();
    found.sort();
    found
}

/// The themes of `kind` inside an unpacked archive: directories one level down, or two when
/// there is none at the first level (an archive that wraps everything in one folder).
pub fn find_in(root: &Path, kind: Kind) -> Vec<(String, PathBuf)> {
    if kind == Kind::Colours {
        let mut found = scheme_files(root);
        if found.is_empty() {
            for (_, dir) in real_dirs(root) {
                found.extend(scheme_files(&dir));
            }
        }
        return found;
    }
    let matches = |dir: &Path| match kind {
        Kind::Icons => is_icon_theme(dir),
        _ => is_cursor_theme(dir),
    };
    let first: Vec<_> = real_dirs(root)
        .into_iter()
        .filter(|(_, dir)| matches(dir))
        .collect();
    if !first.is_empty() {
        return first;
    }
    real_dirs(root)
        .into_iter()
        .flat_map(|(_, dir)| real_dirs(&dir))
        .filter(|(_, dir)| matches(dir))
        .collect()
}

fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        let kind = fs::symlink_metadata(entry.path())?.file_type();
        if kind.is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(entry.path())?, &target)?;
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Moves a directory; across file systems it is copied and the original removed.
fn move_tree(from: &Path, to: &Path) -> io::Result<()> {
    if fs::rename(from, to).is_ok() {
        return Ok(());
    }
    copy_tree(from, to)?;
    let _ = fs::remove_dir_all(from);
    Ok(())
}

fn owned_by(marker: &Path, id: &str) -> bool {
    read_small(marker, 64).is_some_and(|text| text.trim() == id)
}

fn install_dir(source: &Path, parent: &Path, name: &str, id: &str) -> Result<(), StoreError> {
    fs::create_dir_all(parent)?;
    let destination = parent.join(name);
    if fs::symlink_metadata(&destination).is_ok() && !owned_by(&destination.join(MARKER), id) {
        return Err(StoreError::Foreign(name.to_owned()));
    }
    // The new copy is built next to its place so that the final step is one rename.
    let staging = parent.join(format!(".desilium-new-{name}"));
    let _ = fs::remove_dir_all(&staging);
    move_tree(source, &staging)?;
    fs::write(staging.join(MARKER), id)?;
    if fs::symlink_metadata(&destination).is_ok() {
        fs::remove_dir_all(&destination)?;
    }
    fs::rename(&staging, &destination)?;
    Ok(())
}

fn scheme_marker(parent: &Path, name: &str) -> PathBuf {
    parent.join(format!(".{name}.colors{MARKER}"))
}

fn install_scheme(contents: &[u8], parent: &Path, name: &str, id: &str) -> Result<(), StoreError> {
    fs::create_dir_all(parent)?;
    let destination = parent.join(format!("{name}.colors"));
    let marker = scheme_marker(parent, name);
    if fs::symlink_metadata(&destination).is_ok() && !owned_by(&marker, id) {
        return Err(StoreError::Foreign(name.to_owned()));
    }
    let staging = parent.join(format!(".desilium-new-{name}.colors"));
    fs::write(&staging, contents)?;
    fs::write(&marker, id)?;
    fs::rename(&staging, &destination)?;
    Ok(())
}

/// The singular noun of a kind for messages (translated by [`StoreError::message`]).
fn noun(kind: Kind) -> &'static str {
    match kind {
        Kind::Icons => "icon theme",
        Kind::Cursors => "cursor theme",
        Kind::Colours => "colour scheme",
    }
}

/// A colour scheme that was downloaded as a bare file rather than in an archive.
fn plain_scheme(file: &Path) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(file)
        .ok()?
        .take(MAX_SCHEME + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    // Text only (a tar header has NUL bytes), starting with a group like every KConfig file.
    let text = std::str::from_utf8(&bytes).ok()?;
    let starts_with_group = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .is_some_and(|line| line.starts_with('['));
    (bytes.len() as u64 <= MAX_SCHEME
        && !bytes.contains(&0)
        && starts_with_group
        && text.contains("[Colors:"))
    .then_some(bytes)
}

/// Installs the downloaded `file` as theme(s) of `kind` for store entry `id`; returns the names
/// that were installed. `remote_name` and `entry_name` only name a bare colour scheme file.
pub fn install_file(
    file: &Path,
    scratch: &Scratch,
    kind: Kind,
    id: &str,
    names: (&str, &str),
    dirs: &StoreDirs,
) -> Result<Vec<String>, StoreError> {
    if !dirs.can_install() {
        return Err(StoreError::NoHome);
    }
    if kind == Kind::Colours
        && let Some(contents) = plain_scheme(file)
    {
        let (remote_name, entry_name) = names;
        let stem = remote_name
            .strip_suffix(".colors")
            .filter(|stem| valid_name(stem))
            .unwrap_or(entry_name);
        if !valid_name(stem) {
            return Err(StoreError::Unsafe(stem.to_owned()));
        }
        install_scheme(&contents, &dirs.schemes, stem, id)?;
        return Ok(vec![stem.to_owned()]);
    }
    let format = sniff(file)?;
    let entries = match format {
        Format::Tar => list_tar(file)?,
        Format::Zip => list_zip(file)?,
    };
    check_entries(&entries)?;
    let out = scratch.path().join("out");
    fs::create_dir_all(&out)?;
    extract(file, format, &out)?;
    verify_tree(&out)?;
    let found = find_in(&out, kind);
    if found.is_empty() {
        return Err(StoreError::NothingFound(noun(kind)));
    }
    let mut installed = Vec::new();
    for (name, path) in found {
        if kind == Kind::Colours {
            let contents = fs::read(&path)?;
            if contents.len() as u64 > MAX_SCHEME {
                return Err(StoreError::TooLarge);
            }
            install_scheme(&contents, &dirs.schemes, &name, id)?;
        } else {
            install_dir(&path, &dirs.icons, &name, id)?;
        }
        installed.push(name);
    }
    Ok(installed)
}

/// Names of the installed themes of `kind` in every search directory, sorted and unique.
pub fn installed_names(kind: Kind, dirs: &StoreDirs) -> Vec<String> {
    let mut names = BTreeSet::new();
    match kind {
        Kind::Colours => {
            for dir in &dirs.scheme_search {
                names.extend(scheme_files(dir).into_iter().map(|(name, _)| name));
            }
        }
        Kind::Icons | Kind::Cursors => {
            for dir in &dirs.icon_search {
                for (name, path) in real_dirs(dir) {
                    let matches = if kind == Kind::Icons {
                        is_icon_theme(&path)
                    } else {
                        is_cursor_theme(&path)
                    };
                    if matches {
                        names.insert(name);
                    }
                }
            }
        }
    }
    names.into_iter().collect()
}

/// Which theme names came from which store entry, read from the markers of earlier installs.
pub fn installed_by_id(dirs: &StoreDirs) -> BTreeMap<String, Vec<String>> {
    let mut map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, path) in real_dirs(&dirs.icons) {
        if let Some(id) = read_small(&path.join(MARKER), 64) {
            map.entry(id.trim().to_owned()).or_default().push(name);
        }
    }
    for (name, _) in scheme_files(&dirs.schemes) {
        if let Some(id) = read_small(&scheme_marker(&dirs.schemes, &name), 64) {
            map.entry(id.trim().to_owned()).or_default().push(name);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, kind: EntryKind) -> ArchiveEntry {
        ArchiveEntry {
            path: path.to_owned(),
            kind,
        }
    }

    #[test]
    fn paths_must_stay_below_the_extraction_directory() {
        for good in ["Theme", "Theme/index.theme", "./Theme/a b.png", "a/b..c/d"] {
            assert!(is_safe_path(good), "{good:?}");
        }
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../../x",
            "a/..",
            "a\\b",
            "a\0b",
        ] {
            assert!(!is_safe_path(bad), "{bad:?}");
        }
        assert!(!is_safe_path(&"a/".repeat(600)));
    }

    #[test]
    fn links_may_point_inside_the_archive_only() {
        assert!(!link_escapes("Theme/16x16", "16"));
        assert!(!link_escapes(
            "Theme/scalable/apps/a.svg",
            "../../actions/a.svg"
        ));
        assert!(
            !link_escapes("Theme/a/b", "../../Theme/c"),
            "back into the archive"
        );
        assert!(link_escapes("Theme/a", "../../../etc"));
        assert!(link_escapes("a", ".."), "the link sits at the top level");
        assert!(link_escapes("Theme/a", "/etc/passwd"));
        assert!(link_escapes("Theme/a", ""));
        assert!(!link_escapes("./Theme/a", "b"));
    }

    #[test]
    fn a_listing_with_anything_unsafe_is_refused() {
        let ok = [
            entry("Theme", EntryKind::Plain),
            entry("Theme/16x16", EntryKind::Symlink("16".into())),
        ];
        assert!(check_entries(&ok).is_ok());
        for bad in [
            entry("../evil", EntryKind::Plain),
            entry("/abs", EntryKind::Plain),
            entry("Theme/l", EntryKind::Symlink("/etc".into())),
            entry("Theme/l", EntryKind::Symlink("../../out".into())),
            entry("Theme/h", EntryKind::Hardlink),
            entry("Theme/dev", EntryKind::Special),
        ] {
            assert!(
                matches!(
                    check_entries(std::slice::from_ref(&bad)),
                    Err(StoreError::Unsafe(_))
                ),
                "{bad:?}"
            );
        }
        let many: Vec<_> = (0..MAX_ENTRIES + 1)
            .map(|n| entry(&format!("f{n}"), EntryKind::Plain))
            .collect();
        assert!(matches!(check_entries(&many), Err(StoreError::TooLarge)));
    }

    #[test]
    fn the_two_tar_listings_are_paired_and_a_mismatch_is_refused() {
        let names = "Theme/\nTheme/index.theme\nTheme/16x16\n";
        let verbose = "drwxr-xr-x u/g 0 2024-01-01 00:00 Theme/\n\
                       -rw-r--r-- u/g 5 2024-01-01 00:00 Theme/index.theme\n\
                       lrwxrwxrwx u/g 0 2024-01-01 00:00 Theme/16x16 -> 16\n";
        let entries = parse_tar_listing(names, verbose).expect("parse");
        assert_eq!(entries[0], entry("Theme", EntryKind::Plain));
        assert_eq!(
            entries[2],
            entry("Theme/16x16", EntryKind::Symlink("16".into()))
        );
        assert!(parse_tar_listing("a\nb\n", "-rw-r--r-- x a\n").is_err());
        let hard = parse_tar_listing("a\n", "hrw-r--r-- u/g 0 2024-01-01 00:00 a link to b\n")
            .expect("parse");
        assert_eq!(hard[0].kind, EntryKind::Hardlink);
        let device =
            parse_tar_listing("a\n", "crw-r--r-- u/g 1,3 2024-01-01 00:00 a\n").expect("parse");
        assert_eq!(device[0].kind, EntryKind::Special);
    }

    #[test]
    fn directory_names_are_single_plain_components() {
        for good in ["Papirus", "Breeze Dark", "Layan-cursors_1.0"] {
            assert!(valid_name(good), "{good:?}");
        }
        for bad in ["", ".hidden", "a/b", "..", "a\\b", "a\nb", &"x".repeat(65)] {
            assert!(!valid_name(bad), "{bad:?}");
        }
    }

    // --- tests that need the real `tar` ------------------------------------------------------

    fn tool(name: &str) -> bool {
        std::process::Command::new(name)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok()
    }

    /// A minimal ustar archive: `(path, typeflag, link target, contents)` per entry.
    fn tar_bytes(entries: &[(&str, u8, &str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (path, flag, link, data) in entries {
            let mut header = [0u8; 512];
            header[..path.len()].copy_from_slice(path.as_bytes());
            header[100..108].copy_from_slice(b"0000755\0");
            header[108..116].copy_from_slice(b"0000000\0");
            header[116..124].copy_from_slice(b"0000000\0");
            header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
            header[136..148].copy_from_slice(b"00000000000\0");
            header[148..156].copy_from_slice(b"        ");
            header[156] = *flag;
            header[157..157 + link.len()].copy_from_slice(link.as_bytes());
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let sum: u32 = header.iter().map(|b| u32::from(*b)).sum();
            header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
            out.extend_from_slice(&header);
            out.extend_from_slice(data);
            out.resize(out.len().next_multiple_of(512), 0);
        }
        out.resize(out.len() + 1024, 0);
        out
    }

    const FILE: u8 = b'0';
    const DIR: u8 = b'5';
    const SYMLINK: u8 = b'2';
    const HARDLINK: u8 = b'1';
    const ICON_INDEX: &[u8] = b"[Icon Theme]\nName=Cool\nDirectories=16x16/apps\n";

    fn icon_archive(extra: &[(&str, u8, &str, &[u8])]) -> Vec<u8> {
        let mut entries = vec![
            ("Cool/", DIR, "", &b""[..]),
            ("Cool/index.theme", FILE, "", ICON_INDEX),
            ("Cool/16x16/", DIR, "", &b""[..]),
            ("Cool/16x16/apps/", DIR, "", &b""[..]),
            ("Cool/16x16/apps/a.png", FILE, "", b"png"),
            ("Cool/16", SYMLINK, "16x16", &b""[..]),
        ];
        entries.extend_from_slice(extra);
        tar_bytes(&entries)
    }

    fn setup(archive: &[u8]) -> (tempfile::TempDir, StoreDirs, PathBuf) {
        let root = tempfile::tempdir().expect("root");
        let dirs = StoreDirs::under(root.path());
        let file = root.path().join("download");
        fs::write(&file, archive).expect("write");
        (root, dirs, file)
    }

    fn install(
        file: &Path,
        dirs: &StoreDirs,
        kind: Kind,
        id: &str,
    ) -> Result<Vec<String>, StoreError> {
        let scratch = Scratch::new(&dirs.work).expect("scratch");
        install_file(file, &scratch, kind, id, ("theme.tar", "Cool"), dirs)
    }

    #[test]
    fn an_icon_theme_is_unpacked_installed_and_marked() {
        if !tool("tar") {
            return;
        }
        let (_root, dirs, file) = setup(&icon_archive(&[]));
        assert_eq!(
            install(&file, &dirs, Kind::Icons, "42").expect("install"),
            ["Cool"]
        );
        let installed = dirs.icons.join("Cool");
        assert!(installed.join("index.theme").is_file());
        assert!(installed.join("16x16/apps/a.png").is_file());
        assert_eq!(
            fs::read_to_string(installed.join(MARKER)).expect("marker"),
            "42"
        );
        assert_eq!(installed_by_id(&dirs)["42"], ["Cool"]);
        assert_eq!(installed_names(Kind::Icons, &dirs), ["Cool"]);
        assert!(installed_names(Kind::Cursors, &dirs).is_empty());
        assert!(
            fs::read_dir(&dirs.work).expect("work").next().is_none(),
            "the scratch directory is gone"
        );
        // The same entry again replaces its own directory.
        assert!(install(&file, &dirs, Kind::Icons, "42").is_ok());
    }

    #[test]
    fn a_directory_that_is_not_ours_is_never_touched() {
        if !tool("tar") {
            return;
        }
        let (_root, dirs, file) = setup(&icon_archive(&[]));
        let foreign = dirs.icons.join("Cool");
        fs::create_dir_all(&foreign).expect("dir");
        fs::write(foreign.join("keep.txt"), "mine").expect("file");
        let error = install(&file, &dirs, Kind::Icons, "42").expect_err("refused");
        assert!(matches!(error, StoreError::Foreign(name) if name == "Cool"));
        assert_eq!(
            fs::read_to_string(foreign.join("keep.txt")).expect("kept"),
            "mine"
        );
        // Another store entry's directory is refused too.
        fs::write(foreign.join(MARKER), "7").expect("marker");
        assert!(matches!(
            install(&file, &dirs, Kind::Icons, "42"),
            Err(StoreError::Foreign(_))
        ));
    }

    #[test]
    fn unsafe_archives_install_nothing() {
        if !tool("tar") {
            return;
        }
        // (`tar` itself strips a leading `/` when it lists or unpacks, so an absolute name is
        // harmless there; `is_safe_path` still refuses it, see the path test.)
        let cases: [(&str, Vec<u8>); 3] = [
            ("dotdot", icon_archive(&[("../evil", FILE, "", b"x")])),
            (
                "escaping link",
                icon_archive(&[("Cool/out", SYMLINK, "../../..", b"")]),
            ),
            (
                "hard link",
                icon_archive(&[("Cool/hl", HARDLINK, "Cool/index.theme", b"")]),
            ),
        ];
        for (what, archive) in cases {
            let (root, dirs, file) = setup(&archive);
            let error = install(&file, &dirs, Kind::Icons, "1")
                .err()
                .unwrap_or_else(|| panic!("{what}"));
            assert!(matches!(error, StoreError::Unsafe(_)), "{what}: {error:?}");
            assert!(!dirs.icons.join("Cool").exists(), "{what}");
            assert!(!root.path().join("evil").exists(), "{what}");
        }
    }

    #[test]
    fn the_wrong_kind_of_theme_and_garbage_are_reported() {
        if !tool("tar") {
            return;
        }
        let (_root, dirs, file) = setup(&icon_archive(&[]));
        assert!(matches!(
            install(&file, &dirs, Kind::Cursors, "1"),
            Err(StoreError::NothingFound("cursor theme"))
        ));
        let (_root, dirs, file) = setup(b"this is not an archive at all");
        assert!(matches!(
            install(&file, &dirs, Kind::Icons, "1"),
            Err(StoreError::NotAnArchive)
        ));
    }

    #[test]
    fn a_cursor_theme_archive_installs_to_the_icon_directory() {
        if !tool("tar") {
            return;
        }
        let archive = tar_bytes(&[
            ("wrapper/", DIR, "", b""),
            ("wrapper/Big/", DIR, "", b""),
            ("wrapper/Big/cursors/", DIR, "", b""),
            ("wrapper/Big/cursors/left_ptr", FILE, "", b"xcursor"),
        ]);
        let (_root, dirs, file) = setup(&archive);
        assert_eq!(
            install(&file, &dirs, Kind::Cursors, "5").expect("install"),
            ["Big"]
        );
        assert!(
            dirs.icons.join("Big/cursors/left_ptr").is_file(),
            "found one folder down"
        );
        assert_eq!(installed_names(Kind::Cursors, &dirs), ["Big"]);
    }

    #[test]
    fn colour_schemes_install_from_a_bare_file_and_from_an_archive() {
        const SCHEME: &[u8] = b"[General]\nName=Night\n[Colors:Window]\nBackgroundNormal=1,2,3\n";
        let (_root, dirs, file) = setup(SCHEME);
        let scratch = Scratch::new(&dirs.work).expect("scratch");
        let names = install_file(
            &file,
            &scratch,
            Kind::Colours,
            "9",
            ("Night.colors", "Night scheme"),
            &dirs,
        )
        .expect("bare file");
        assert_eq!(names, ["Night"], "named after the file, not the title");
        assert_eq!(
            fs::read(dirs.schemes.join("Night.colors")).expect("scheme"),
            SCHEME
        );
        assert_eq!(installed_by_id(&dirs)["9"], ["Night"]);
        assert_eq!(installed_names(Kind::Colours, &dirs), ["Night"]);
        // A scheme of that name from another entry is refused.
        let error = install_file(
            &file,
            &scratch,
            Kind::Colours,
            "10",
            ("Night.colors", "x"),
            &dirs,
        )
        .expect_err("foreign");
        assert!(matches!(error, StoreError::Foreign(_)));
        if tool("tar") {
            let archive = tar_bytes(&[("Day.colors", FILE, "", SCHEME)]);
            let (_root, dirs, file) = setup(&archive);
            assert_eq!(
                install(&file, &dirs, Kind::Colours, "3").expect("archive"),
                ["Day"]
            );
        }
    }

    #[test]
    fn unpacked_trees_are_checked_for_links_and_special_files() {
        let dir = tempfile::tempdir().expect("dir");
        fs::create_dir_all(dir.path().join("T/a")).expect("dir");
        fs::write(dir.path().join("T/a/f"), "12345").expect("file");
        std::os::unix::fs::symlink("a", dir.path().join("T/ok")).expect("link");
        assert_eq!(verify_tree(dir.path()).expect("fine"), 5);
        std::os::unix::fs::symlink("../../outside", dir.path().join("T/bad")).expect("link");
        assert!(matches!(
            verify_tree(dir.path()),
            Err(StoreError::Unsafe(_))
        ));
    }

    #[test]
    fn stale_scratch_directories_are_swept_but_fresh_ones_stay() {
        let work = tempfile::tempdir().expect("work");
        let fresh = Scratch::new(work.path()).expect("fresh");
        sweep(work.path());
        assert!(fresh.path().exists());
    }
}
