//! Filesystem layer: atomic writes, backups, quarantine and change fingerprints.
//!
//! Unix only: permissions are enforced with `0600` files and `0700` directories.

use std::fs::{self, DirBuilder, File, OpenOptions, Permissions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use desilium_config_types::DesiliumConfig;

pub(crate) const CONFIG_FILE_NAME: &str = "config.toml";
const BACKUPS_DIR_NAME: &str = "backups";
const BACKUP_PREFIX: &str = "config-";
const BACKUP_SUFFIX: &str = ".toml";
const QUARANTINE_INFIX: &str = ".corrupt-";
/// Number of backups kept after rotation.
pub const KEEP_BACKUPS: usize = 10;
const FILE_MODE: u32 = 0o600;
const DIR_MODE: u32 = 0o700;
const HEADER: &str = "# Desilium configuration. Managed by the Desilium settings app.\n\
                      # Do not store secrets such as passwords or tokens in this file.\n";
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A saved configuration version, identified by its sequence number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupInfo {
    pub seq: u64,
    pub path: PathBuf,
}

/// Identity of a file's contents, used to detect external edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    modified: Option<SystemTime>,
    len: u64,
    hash: u64,
}

/// A file read together with its fingerprint.
pub(crate) struct Snapshot {
    pub fingerprint: Fingerprint,
    pub bytes: Vec<u8>,
}

/// Paths inside one configuration directory.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    dir: PathBuf,
}

impl Layout {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub(crate) fn dir(&self) -> &Path {
        &self.dir
    }

    pub(crate) fn config_file(&self) -> PathBuf {
        self.dir.join(CONFIG_FILE_NAME)
    }

    pub(crate) fn backups_dir(&self) -> PathBuf {
        self.dir.join(BACKUPS_DIR_NAME)
    }

    pub(crate) fn backup_file(&self, seq: u64) -> PathBuf {
        self.backups_dir()
            .join(format!("{BACKUP_PREFIX}{seq}{BACKUP_SUFFIX}"))
    }
}

impl Fingerprint {
    fn of(bytes: &[u8], modified: Option<SystemTime>, len: u64) -> Self {
        Self {
            modified,
            len,
            hash: fnv1a64(bytes),
        }
    }
}

/// FNV-1a is stable across runs and builds, unlike `DefaultHasher`.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(FNV_OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

/// Reads `path`; `Ok(None)` when it does not exist.
pub(crate) fn read_snapshot(path: &Path) -> io::Result<Option<Snapshot>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let bytes = fs::read(path)?;
    let fingerprint = Fingerprint::of(&bytes, metadata.modified().ok(), metadata.len());
    Ok(Some(Snapshot { fingerprint, bytes }))
}

/// Creates the config and backup directories with private permissions.
pub(crate) fn ensure_dirs(layout: &Layout) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(DIR_MODE)
        .create(layout.backups_dir())
}

/// Serializes the config with a header comment.
pub(crate) fn render(config: &DesiliumConfig) -> Result<String, toml::ser::Error> {
    Ok(format!("{HEADER}{}", toml::to_string_pretty(config)?))
}

/// Writes `bytes` to `path` via a synced temp file and rename, then syncs the directory.
///
/// A crash leaves either the old or the new file, never a partial one.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::Error::other(
            "path has no parent directory or file name",
        ));
    };
    let temp = parent.join(format!(".{}.tmp", name.to_string_lossy()));
    if let Err(error) = write_and_sync(&temp, bytes) {
        fs::remove_file(&temp).ok();
        return Err(error);
    }
    if let Err(error) = fs::rename(&temp, path) {
        fs::remove_file(&temp).ok();
        return Err(error);
    }
    File::open(parent)?.sync_all()
}

fn write_and_sync(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(FILE_MODE)
        .open(path)?;
    file.set_permissions(Permissions::from_mode(FILE_MODE))?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Lists backups sorted by sequence number, oldest first.
pub(crate) fn list_backups(layout: &Layout) -> io::Result<Vec<BackupInfo>> {
    let entries = match fs::read_dir(layout.backups_dir()) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut backups = Vec::new();
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let Some(seq) = name.to_str().and_then(parse_backup_seq) else {
            continue;
        };
        backups.push(BackupInfo {
            seq,
            path: entry.path(),
        });
    }
    backups.sort_by_key(|backup| backup.seq);
    Ok(backups)
}

fn parse_backup_seq(name: &str) -> Option<u64> {
    name.strip_prefix(BACKUP_PREFIX)?
        .strip_suffix(BACKUP_SUFFIX)?
        .parse()
        .ok()
}

/// Stores `bytes` as the next backup and prunes to [`KEEP_BACKUPS`]. Returns the new sequence number.
pub(crate) fn write_backup(layout: &Layout, bytes: &[u8]) -> io::Result<u64> {
    let existing = list_backups(layout)?;
    let seq = existing.last().map_or(1, |last| last.seq.saturating_add(1));
    write_atomic(&layout.backup_file(seq), bytes)?;
    let surplus = (existing.len() + 1).saturating_sub(KEEP_BACKUPS);
    for backup in existing.iter().take(surplus) {
        fs::remove_file(&backup.path).ok();
    }
    Ok(seq)
}

/// Moves the config file aside as `config.toml.corrupt-<n>` using the first free `n`.
pub(crate) fn quarantine(layout: &Layout) -> io::Result<PathBuf> {
    let target = (1u64..)
        .map(|n| {
            layout
                .dir()
                .join(format!("{CONFIG_FILE_NAME}{QUARANTINE_INFIX}{n}"))
        })
        .find(|path| !path.exists())
        .ok_or_else(|| io::Error::other("no free name for a quarantined file"))?;
    fs::rename(layout.config_file(), &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> (tempfile::TempDir, Layout) {
        let dir = tempfile::tempdir().expect("tempdir");
        let layout = Layout::new(dir.path().to_path_buf());
        (dir, layout)
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).expect("metadata").permissions().mode() & 0o777
    }

    #[test]
    fn atomic_write_leaves_no_temp_file_and_is_private() {
        let (_dir, layout) = layout();
        write_atomic(&layout.config_file(), b"a = 1\n").expect("write");
        let names: Vec<String> = fs::read_dir(layout.dir())
            .expect("list")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![CONFIG_FILE_NAME.to_owned()]);
        assert_eq!(mode(&layout.config_file()), FILE_MODE);
        assert_eq!(fs::read(layout.config_file()).expect("read"), b"a = 1\n");
    }

    #[test]
    fn atomic_write_replaces_existing_content() {
        let (_dir, layout) = layout();
        write_atomic(&layout.config_file(), b"old").expect("first");
        write_atomic(&layout.config_file(), b"new").expect("second");
        assert_eq!(fs::read(layout.config_file()).expect("read"), b"new");
    }

    #[test]
    fn failed_write_removes_temp_file() {
        let (_dir, layout) = layout();
        let missing_parent = layout.dir().join("absent").join(CONFIG_FILE_NAME);
        assert!(write_atomic(&missing_parent, b"x").is_err());
        assert!(!layout.dir().join(".config.toml.tmp").exists());
    }

    #[test]
    fn snapshot_of_missing_file_is_none() {
        let (_dir, layout) = layout();
        assert!(read_snapshot(&layout.config_file()).expect("ok").is_none());
    }

    #[test]
    fn fingerprint_changes_with_content() {
        let (_dir, layout) = layout();
        write_atomic(&layout.config_file(), b"a").expect("write");
        let first = read_snapshot(&layout.config_file())
            .expect("ok")
            .expect("some");
        write_atomic(&layout.config_file(), b"b").expect("write");
        let second = read_snapshot(&layout.config_file())
            .expect("ok")
            .expect("some");
        assert_ne!(first.fingerprint, second.fingerprint);
    }

    #[test]
    fn backups_rotate_and_keep_ten_newest() {
        let (_dir, layout) = layout();
        ensure_dirs(&layout).expect("dirs");
        for n in 1..=12u8 {
            write_backup(&layout, &[n]).expect("backup");
        }
        let seqs: Vec<u64> = list_backups(&layout)
            .expect("list")
            .iter()
            .map(|b| b.seq)
            .collect();
        assert_eq!(seqs, (3..=12).collect::<Vec<u64>>());
        assert_eq!(mode(&layout.backup_file(12)), FILE_MODE);
    }

    #[test]
    fn quarantine_uses_first_free_suffix() {
        let (_dir, layout) = layout();
        write_atomic(&layout.config_file(), b"bad").expect("write");
        write_atomic(&layout.dir().join("config.toml.corrupt-1"), b"old").expect("old");
        let moved = quarantine(&layout).expect("moved");
        assert_eq!(
            moved.file_name().and_then(|n| n.to_str()),
            Some("config.toml.corrupt-2")
        );
        assert!(!layout.config_file().exists());
        assert_eq!(fs::read(&moved).expect("read"), b"bad");
    }

    #[test]
    fn render_is_loadable_and_starts_with_header() {
        let text = render(&DesiliumConfig::default()).expect("render");
        assert!(text.starts_with("# Desilium configuration"));
        let parsed: DesiliumConfig = toml::from_str(&text).expect("parses");
        assert_eq!(parsed, DesiliumConfig::default());
    }
}
