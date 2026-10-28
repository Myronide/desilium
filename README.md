# Desilium

Desilium is a desktop environment for Linux, written in Rust on top of
Wayland. It is built to work out of the box: no configuration files to write,
every setting reachable from a graphical interface, and a visual style that
aims for the polish of a modern consumer operating system.

Desilium builds on [KWin](https://invent.kde.org/plasma/kwin) (`kwin_wayland`
from KDE Plasma 6) and ships no compositor of its own. What Desilium provides is
the shell (top bar, dock/taskbar, launcher, quick settings, wallpaper), the
settings application, the Doria file manager, a notification service, a session
supervisor and a small bridge that projects the Desilium configuration onto
KWin. The codebase is modular: configuration, theming, the KWin bridge, the
session and the applications are separate crates with small, testable interfaces.

> **Status: early development.** The move to KWin is implemented in code and
> unit-tested, but **nothing has been run against a live KWin yet**: the KDE-side
> key names, D-Bus signatures and shortcut action names were written from memory.
> The list of what is unverified, and what to check first on a machine with KWin 6,
> is in [`docs/kwin-integration.md`](docs/kwin-integration.md#11-verification-status).
> Expect breaking changes.

## Migration note: from the own Smithay compositor to KWin

Earlier revisions of Desilium contained their own Wayland compositor built on
[Smithay](https://github.com/Smithay/smithay): the crates `desilium-compositor`,
`desilium-wm`, `desilium-cursor`, `desilium-anim`, `desilium-text` and
`desilium-protocols` (with the private protocol `desilium-shell-v1`). They were
removed, and KWin is now the compositor and window manager. The reasons are in
[`concept.md`](concept.md) section 1.1 (maturity of the DRM/KMS backend,
multi-monitor, HiDPI, XWayland, screencast portals and window management that other
people test, against the Qt/KDE dependency and the loss of custom effects).

What this means in practice:

- Desilium needs KWin, `kglobalacceld`, `kconfig` (`kwriteconfig6`) and, for screen
  sharing, the KDE portal backend at runtime. `desilium-session` starts `kwin_wayland`.
- The typed configuration (`config.toml`) is unchanged and stays the single source
  of truth. The new crate `desilium-kwin` writes it into KWin's and KDE's configuration
  and registers the global shortcuts.
- The shell reads the window list from KWin (`org_kde_plasma_window_management`) instead
  of the private protocol. KWin only grants that to executables named in a desktop file
  with `X-KDE-Wayland-Interfaces`; for development use `desilium-session --allow-any-client`.
- The shell draws the wallpaper, because KWin has none.
- **Lost for now:** Desilium's own window animations and transitions, compositor-drawn
  glass popups, Desilium-drawn window decorations (KWin's are used), the compositor-side
  window switcher (Alt+Tab is KWin's), the headless screenshot mode and
  `scripts/docker-screenshot.sh`, the `tile` wallpaper mode, and `--no-shell`,
  `--headless` and the other flags of the old compositor. The full list is in
  [`docs/kwin-integration.md`](docs/kwin-integration.md#10-what-changed-and-known-gaps).
- **Gained:** a DRM/KMS backend, outputs and HiDPI, XWayland and tested window management.
- The Smithay-based WC prototype remains in `spikes/wc` as an excluded, separate workspace.

## Current desktop work

- **Kitty** is the default terminal; explicit user choices are preserved.
- **Desktop icons**: the desktop folder's launchers, folders and files appear on the desktop;
  right-click creates, renames and trashes. See [desktop controls](docs/desktop-controls.md).
- **Doria**, the native Rust/Blitz file manager, browses files, disks, pinned
  folders and GIO/GVfs network locations. See [Doria](docs/doria.md).
- **Theme store** in Settings ("Get new themes"): browse the KDE Store, download icon themes,
  cursor themes and colour schemes, and select them. See [theme store](docs/theme-store.md).
- **Dock and taskbar modes**, right-click app controls and a configurable panel.
- **Quick settings** for real audio, radios, brightness and energy services.
- **Panel & Dock** in Settings exposes fourteen live, reversible preferences.
- Focus, snapping and key-repeat settings keep their place in the typed configuration and are
  applied to KWin by the bridge.

See [desktop controls](docs/desktop-controls.md) and
[WC integration](docs/wc-integration.md). The original prototype is retained as
an excluded workspace in `spikes/wc`; Desilium remains the primary desktop.

## Principles

- **Zero-config.** Sensible defaults for everything. Users never need to edit a
  file to use, personalize, maintain or troubleshoot Desilium.
- **GUI-first.** Settings are typed, validated and changeable from the desktop.
- **Modular.** Each concern is a crate with a narrow responsibility. The
  bridge knows KWin; the wallpaper does not know about windows.
- **Builds on KWin.** No compositor of our own. KWin may keep running when the shell
  dies; if KWin dies, the session ends.
- **Premium look.** Consistent typography and a considered theme, with GPU
  rendering where it helps.

The full product concept is in [`concept.md`](concept.md). The process model and
its decisions are in [`docs/architecture.md`](docs/architecture.md), the KWin
interface in [`docs/kwin-integration.md`](docs/kwin-integration.md).

## Roadmap

Phases follow section 9 of [`concept.md`](concept.md).

| Phase | Scope | Status |
|-------|-------|--------|
| 1. Foundation: KWin integration | Cargo workspace, KWin under the session supervisor, configuration projected onto KWin and KDE, plasma protocol client in the shell, wallpaper, nested development environment, logging and tests | Implemented in code and unit-tested; not yet run against a live KWin |
| 2. UI foundation | Rendering abstraction, theme system, panels, application launcher, input and performance validation, blur on KWin | In progress |
| 3. Configuration platform | Central configuration service, typed schemas, validation and transactions, automatic defaults, live change notifications, graphical settings, rollback and recovery | Store, transactions, backups and the settings app exist; the rest is in progress |
| 4. Desktop features | Per-output shell surfaces, notifications, system tray, quick settings, audio, network, power management, lock screen (multi-monitor, snapping and virtual desktops come from KWin) | In progress (notifications, quick settings, panels); lock screen planned |
| 5. Integration | Desktop portals (KDE backend for screencast and remote desktop), screen sharing and capture, clipboard, drag and drop, accessibility, session management (XWayland comes from KWin) | Planned |
| 6. Stabilization | Profiling, memory work, crash recovery, configuration migration, hardware testing, security and accessibility review, regression testing | Planned |

## Crates

All crates live under `crates/` and share one workspace version.

| Crate | Responsibility |
|-------|----------------|
| `desilium-config-types` | Typed, serializable settings and keyboard shortcuts with defaults. |
| `desilium-config` | Configuration store: loading, validation, transactions, backups and change detection. |
| `desilium-theme` | Theme model: colours, metrics and typography tokens. |
| `desilium-kwin` | The KWin bridge: maps the configuration onto KWin and KDE (KConfig keys, D-Bus, a KDE colour scheme), registers global shortcuts through KGlobalAccel, places windows through a KWin script. Library and the `desilium-kwin` binary. |
| `desilium-session` | Session lifecycle: starts `kwin_wayland`, the shell, the bridge and the notification daemon and supervises them. |
| `desilium-ui` | Blitz-based UI host for Wayland surfaces. |
| `desilium-shell` | Top bar, dock/taskbar, application menus, launcher, quick settings and wallpaper; client of KWin's window, virtual desktop and blur protocols. |
| `desilium-settings` | Graphical appearance, desktop, keyboard and Panel & Dock preferences. |
| `desilium-files` / `doria` | Native file manager: local files, removable volumes, pinned folders and network locations. |
| `desilium-notifications` | Notification daemon (`org.freedesktop.Notifications`) and the interface the shell uses. |
| `desilium-apps` | Desktop-file catalog and application launching. |
| `desilium-wallpaper` | Wallpaper loading, scaling and generation. |

The compositor is not a crate: it is KWin, installed from your distribution.

## Building

Desilium needs a current stable Rust toolchain (edition 2024). The repository includes a
`rust-toolchain.toml` that selects the stable channel with `rustfmt` and
`clippy` through [rustup](https://rustup.rs).

Building the workspace needs a few native libraries. **Running** a session needs KWin and
the KDE services it works with: `kwin_wayland`, `kglobalacceld` (global shortcuts), `kconfig`
(for `kwriteconfig6`), Xwayland and `xdg-desktop-portal-kde`.

**Arch Linux**

```sh
sudo pacman -S --needed base-devel pkgconf wayland libxkbcommon mesa glib2 fontconfig dbus kitty \
    kwin kglobalacceld kconfig xorg-xwayland xdg-desktop-portal-kde
```

**Debian and Ubuntu**

```sh
sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev libglib2.0-dev \
    libfontconfig-dev kitty kwin-wayland kglobalacceld kf6-kconfig-bin xwayland \
    xdg-desktop-portal-kde
```

The Debian and Ubuntu package names for the KDE parts were written from memory and
differ between releases (they are the same guesses as in `docker/Dockerfile`, which has not
been built); check them against your release. The Arch list matches `packaging/arch/PKGBUILD`.

Then build and test:

```sh
cargo build --workspace
cargo test --workspace
```

Run `scripts/check.sh` before pushing. It runs the formatter, clippy with
warnings as errors, the tests and the style checker. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for details.

## Running

Nothing here has been verified against a live KWin; see the status above.

### Nested in your current Wayland session

`desilium-session` starts `kwin_wayland` (with `--xwayland --no-lockscreen`), then the shell,
the bridge and the notification daemon. Started inside a Wayland session, KWin should open as
a window of that session (its nested backend; this is unverified). Arguments after `--` go to
KWin.

```sh
cargo build --workspace
XDG_CONFIG_HOME=/tmp/desilium-xdg target/debug/desilium-session --allow-any-client
target/debug/desilium-session --allow-any-client -- --width 1600 --height 900
```

`--allow-any-client` switches off KWin's per-executable permission check, which is needed
when the shell runs from `target/`. Use a throw-away `XDG_CONFIG_HOME`, because the bridge
writes KDE configuration files (`kwinrc`, `kdeglobals`, `kcminputrc`, `kxkbrc`); otherwise it
changes the settings of your host desktop. `desilium-session --dry-run` shows the plan without
starting anything. See [`docs/session.md`](docs/session.md).

### Safely, in Docker (recommended)

`scripts/docker-run.sh` builds the image from `docker/Dockerfile` (Debian; `--arch`, `--ubu` and
`--fed` select the Arch, Ubuntu and Fedora images) and runs `desilium-session` in a locked-down
container, with KWin as a client of your current Wayland session. It must be started from inside a
Wayland session, because it needs `WAYLAND_DISPLAY` and `XDG_RUNTIME_DIR`.

```sh
scripts/docker-run.sh                                       # build the image, then run (--allow-any-client)
scripts/docker-run.sh --no-build                            # reuse the existing image
scripts/docker-run.sh --allow-any-client -- --width 1600 --height 900   # arguments after -- go to kwin_wayland
scripts/docker-run.sh --ubu --check                         # smoke test of the Ubuntu image, no display needed
scripts/docker-run.sh --ubu --save --width 2650 --height 1440   # session whose whole system (/) is kept in pseudo-ubuntu/system.tar
```

The container user is `dev` with the home `/home/dev`, a **named Docker volume**
(`desilium-dev-home-<distro>`, override with `DESILIUM_HOME_VOLUME`), seeded on first use from
`docker/home-skel`: `Applications`, `Desktop`, `Documents`, `Downloads`, `Music`, `Pictures`
(with `Wallpapers`), `Public`, `Templates`, `Videos`, `~/.config/user-dirs.dirs`, executable
Doria and Settings launchers on the desktop and `Documents/Welcome.txt`. What the session writes
there persists between runs. Flags: `--arch` / `--ubu` / `--fed` (distro), `--no-build`, `--net`,
`--sudo` / `--no-sudo` (Arch), `--fresh-home` (throw-away anonymous volume), `--reset-home`
(delete the volume), `--save` (keep the whole system `/`, home and installed packages included, in the git-ignored
`pseudo-<distro>/system.tar`: the session starts from it and is written back to it when it ends;
without `--save` the file is not touched),
`--width` / `--height` (window size), `--check` (smoke test). Details:
[`docs/testing.md`](docs/testing.md#the-development-user-space).

The container is locked down:

- **Minimal host access.** It receives only the host Wayland socket and the GPU
  render node. No other devices, no host directory, no network. (`--save` streams a
  tar through a helper container instead of mounting a host directory.)
- **No privileges.** No Linux capabilities except `SYS_NICE` (the Arch `kwin_wayland` carries it
  as a file capability, and `exec` fails without it), `no-new-privileges`, a non-root user
  and a read-only root filesystem; the only writable locations are `/tmp` (tmpfs) and the home
  volume (Docker-managed storage, not a host path).
- **No resource limits.** PID, memory and CPU are not capped, on purpose; `--init` reaps child
  processes.

The images and `packaging/install.sh` were **not built or run** in the environment where
this documentation was updated. See [`docs/testing.md`](docs/testing.md) for the safety rules.

### Default shortcuts

These are the defaults from `crates/desilium-config-types/src/settings.rs`. They are
registered with KGlobalAccel by the bridge; the full table with the KWin action behind each
one is in [`docs/kwin-integration.md`](docs/kwin-integration.md#5-shortcut-model).

| Shortcut | Action | Handled by |
|----------|--------|------------|
| `Alt` + `Enter` | Launch a terminal | bridge |
| `Alt` + `Q` | Close the focused window | KWin |
| `Alt` + `Shift` + `Q` | End the session | bridge |
| `Alt` + `Space` | Show or hide the launcher | shell |
| `Alt` + `Shift` + `M` | Restore the most recently minimized window | shell |
| `Alt` + `Up` / `Down` | Maximize / minimize | KWin |
| `Alt` + `F` | Fullscreen | KWin |
| `Alt` + `Left` / `Right` | Snap left / right (quick tile) | KWin |
| `Alt` + `Tab` / `Alt` + `Shift` + `Tab` | Next / previous window (KWin's Walk Through Windows) | KWin |
| `Alt` + `Ctrl` + `Left` / `Right` | Previous / next virtual desktop | KWin |
| `Alt` + `1` to `4` | Switch to desktop 1 to 4 | KWin |
| `Alt` + `Shift` + `1` to `4` | Move the window to desktop 1 to 4 | KWin |

Alt is used instead of the Logo key because Desilium is developed nested
inside another desktop, which already uses the Logo key.

## Testing and screenshots

Unit tests live next to the code in each crate. The test layers are described in
[`docs/testing.md`](docs/testing.md). The old compositor's headless screenshot mode is gone and
has no replacement yet; see [`docs/headless.md`](docs/headless.md) for what is possible today.

## Code style

All Rust code follows **Pureline 1.5**. The specification is in
[`PURELINE_SPECIFICATION_1.5.md`](PURELINE_SPECIFICATION_1.5.md). It favours
linear control flow, guard clauses, small cohesive modules and explicit error
handling. The rules that contributors hit most often are summarised in
[`CONTRIBUTING.md`](CONTRIBUTING.md).

The Pureline checker is a separate tool outside this repository. If it is
installed on your `PATH`, or at `$HOME/tools/bin/pureline`, `scripts/check.sh`
runs it automatically. You can also run it yourself:

```sh
pureline --no-info crates docs
```

## Contributing

Contributions are welcome. Read [`CONTRIBUTING.md`](CONTRIBUTING.md) for the
workflow, the checks that must pass and the definition of done.

## License

Desilium is released under the MIT License. See [`LICENSE`](LICENSE).
