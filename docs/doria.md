# Doria — Desilium Files

Doria is Desilium's own Rust file manager. Its application model, navigation,
selection, places, file actions and views are written in this repository.
HTML/CSS is rendered by the existing `desilium-ui` Blitz adapter. GTK and an
external file manager are not used for the interface.

The name fits alongside Desilium while giving the application its own identity.

## Run

```sh
cargo build -p desilium-files --locked
./target/debug/doria
./target/debug/doria ~/Downloads
./target/debug/doria sftp://user@server/home/user
```

A running Wayland desktop is required for the window. The desktop entry is
`org.desilium.Doria.desktop`; the executable is `doria`. Packaging installs
both and prefers Doria in the shell's first-run Files slot. Existing dock pins
are not changed.

## Interface

The window is deliberately plain: a narrow sidebar (Home, standard folders, Trash,
pinned folders, devices, network) with the current place highlighted, one header
row (back, forward, parent, path, pin, refresh, search) and the file list. New folder
is always one click away; Open, Copy, Cut, Rename and Trash appear in the action row
only while an item is selected. Click an item to select it and click it again to open
it. Sorting is done by clicking the column headers; "Show hidden files" sits in the
status line. Colors come from the Desilium theme, so light/dark and the accent follow
the desktop.

## Implemented

- Local folder navigation, back/forward, parent and editable location bar.
- Folder-first sorting by name, size or modification time.
- Current-folder search and show/hide hidden entries.
- Keyboard selection; Enter opens, Delete asks to move to Trash.
- Ctrl+L location, Ctrl+F filter, Ctrl+H hidden files, Ctrl+R refresh.
- Alt+Left/Right history and Alt+Up parent.
- New folders, rename, in-app Copy/Cut/Paste, and system Trash with confirmation.
- Directory copies without overwriting existing files or following symlinks.
- Cancellation of file/mount jobs; failures retain partial copies and report that fact.
- Persistent, graphically editable pinned places, atomic writes, and recovery
  of broken places files while preserving the original.
- Detected volumes and mounts, local free/total space, explicit mount and safe
  unmount actions. No forced unmount or formatting.
- Network browsing, direct server addresses, native sign-in and host-confirmation dialogs.
- Directory/device monitors instead of periodic rescanning.
- Live light/dark/accent theme updates from the central Desilium configuration.
- Pages of 100 entries to keep large directory views bounded.

File actions use the selected item. The **Open** button opens a folder or
launches the file's registered application. Copy/Cut/Paste currently use one
selected item and Doria's own clipboard, not the session clipboard.

## Filesystem and service boundary

`backend/files.rs` performs file operations through Rust GIO bindings. GIO/GVfs
supplies established local, removable-volume and remote filesystem services;
it does not supply the UI or application logic. Background work has its own
GLib context, with results handed to calloop. The window thread does not wait
for directory scans, copies, mounts or network authentication.

SMB, SFTP, FTP and WebDAV depend on the corresponding installed GVfs backends.
See the [upstream GVfs project](https://github.com/GNOME/gvfs) for the supported
backends. Typical packages are `gvfs` and `gvfs-smb` on Arch or `gvfs-backends`
on Debian/Ubuntu. Volume discovery/mounting normally uses UDisks2 and the
session's policy authorization agent. Missing services and denied access are
shown as errors; Doria does not run privileged shell commands.

Passwords are rejected in saved URIs and entered only in a native dialog.
The mount operation uses `PasswordSave::Never`; credentials are not stored in
places or the central TOML configuration. Host key/trust questions from GVfs
are shown with explicit user choices rather than automatically accepted.

## Storage

Saved places live in `$XDG_CONFIG_HOME/desilium/doria/places.toml` (normally
`~/.config/desilium/doria/places.toml`). The GUI adds/removes them; no file edit
is required. New writes use mode 0600 and atomic rename. A parse error does
not overwrite the file; **Reset saved places** preserves a backup before
starting over.

## Testing and limits

```sh
cargo test -p desilium-files --locked
./target/debug/doria --render screenshots/doria.png /absolute/test/folder
```

Tests exercise real local GIO operations in temporary directories, no-overwrite
copies, symlinks, cancellation, filtering/history, saved-place recovery,
credential-bearing URI rejection, escaped markup and authentication signals.
Screenshots are generated into ignored output directories.

Real SMB/SFTP/WebDAV servers and removable-device mount/unmount actions have
not been exercised end-to-end on every service and driver combination. The
file manager does not yet have tabs, split view, thumbnails, archive browsing,
multi-selection or Trash restoration. It never substitutes permanent deletion
when Trash is unsupported. This is a working first version, not Dolphin feature
parity.
