# Panels, dock and quick settings

The shell now offers two application-panel layouts:

- **Dock:** a floating, content-sized strip of pinned and running applications.
- **Taskbar:** a wide panel with optional application names, running indicators
  and a system/clock button.

Both use the existing application catalog, window protocol, minimize targets
and launcher. Left click launches, activates, cycles grouped windows or
minimizes the single active window. Middle click starts a new instance.
Right click opens application controls: pin/unpin, move a pin left/right,
start a new instance, activate a specific window, or request that it close.
Pin edits are atomic. Existing user pin lists are preserved.

## Graphical customization

Open **Settings → Panel & Dock** (CLI page slug `panel`). All controls use the
normal configuration transactions, validation, backup, Undo, search and reset
paths. Changes apply live:

| Setting | Default | Range/options |
| --- | --- | --- |
| Layout | Dock | Dock, Taskbar |
| Edge | Bottom | Bottom, Top |
| Icon size | 48 px | 32–72 px |
| Item spacing | 8 px | 0–24 px |
| Edge margin | 14 px | 0–40 px |
| Background opacity | 92% | 40–100% |
| Taskbar labels | On | On/off |
| Unpinned running apps | Shown | Shown/hidden |
| Running indicators | On | On/off |
| Workspace switcher | On | On/off |
| Clock | On | On/off |
| Clock format | 24 hour | 12/24 hour |
| Battery indicator | On | On/off |
| Quick settings | On | On/off |

The top system panel remains available in both layouts. Moving the application
panel to the top places it below the system panel. A taskbar reserves work area;
a floating dock does not. Long application strips can scroll horizontally.
Auto-hide, vertical side docks and ungrouped per-window task buttons are not yet
implemented.

## Quick settings

Click the status group in the top panel, or the system button of the taskbar.
Escape or clicking elsewhere closes the panel. It offers volume and mute,
microphone mute, Wi-Fi and Bluetooth power, display brightness, energy profile,
light/dark mode, layout switching and links to the full settings.

The controls make real requests to established services using fixed process
arguments, never interpolated shell commands:

| Control | Integration |
| --- | --- |
| Audio/microphone | WirePlumber `wpctl` / PipeWire |
| Wi-Fi | NetworkManager `nmcli` |
| Bluetooth | BlueZ `bluetoothctl` |
| Backlight | `brightnessctl`, backlight class only |
| Energy profile | `powerprofilesctl` |
| Theme/layout | Desilium configuration transactions |

System queries and writes run on a dedicated worker. External tools have a
3-second timeout. State is refreshed when opening the panel, after an action,
or by the Refresh button; there is no background control polling while closed.
An unavailable service has no enabled toggle. Hardware/service failures appear
inside the panel. The feature does not simulate successful changes.

The integrations are optional: installing the corresponding service makes its
control available without changing configuration files. No network scan,
device pairing or system setting is changed merely by starting the shell.
Bluetooth pairing, Wi-Fi network selection/password entry and audio output
selection remain future work. The existing bar's compact network/audio glyphs
are shortcuts; the quick panel carries the queried state.

## Panel visibility

The top bar and the dock/taskbar can each be switched off
(`shell.bar_enabled`, `shell.panel_enabled`) and run in one of three modes
(`shell.bar_visibility`, `shell.panel_visibility`): `always`, `auto-hide` and
`avoid-windows`. A hidden panel shrinks to a 2 px strip at its screen edge;
touching it brings the panel back, and it hides again 350 ms after the pointer
leaves. `avoid-windows` hides a panel while a visible window of the current
workspace overlaps it, using the `geometry` event of
`desilium_toplevel_handle_v1`. Only `always` reserves screen space; the other
modes let windows use the full output. An open popup keeps its panel visible.
There is no slide animation yet.

## Network

The quick panel shows the active Ethernet or Wi-Fi connection and, with Wi-Fi
on, up to five networks in range (strongest first). Selecting one runs
`nmcli device wifi connect`, which works for open and already saved networks;
password entry is not implemented. Everything goes through `nmcli`, so the
section reads "NetworkManager unavailable" without it.

## Restoring the session

`shell.restore_session` (Settings → Desktop → Session, off by default) saves which
applications have windows open, with their workspace, position, size and minimized
state, to `$XDG_STATE_HOME/desilium/session.toml` three seconds after the last window
change. At the next start on a desktop without windows the shell launches those
applications again and puts each new window back, matched by Wayland app id, using the
`move_to_workspace` and `set_geometry` requests. Windows that do not open within a
minute are skipped. Only applications found in the desktop-file catalog are saved.

This restores the arrangement of the desktop, like "reopen windows" on macOS. It does
not freeze and thaw processes: a program's own state (browser tabs, unsaved text) is
only back if the program restores it itself. A shell that merely restarts on a running
desktop never moves or duplicates windows.

## Arch Linux test image

`scripts/docker-run.sh --arch` builds `docker/Dockerfile.arch`: the same
binaries on an Arch runtime with pacman-installed NetworkManager, WirePlumber,
brightnessctl and bluez-utils. Add `--net` to attach the container to the bridge
network (the default is no network) so the Ethernet state can be tested.
The container has no Wi-Fi radio, so only the Ethernet path is exercisable there.

Besides pacman the image has the AUR helpers `yay` and `paru` (built from `yay-bin` and
`paru-bin` while the image is built, which needs network access then), `base-devel`,
`git` and everyday tools (`curl`, `wget`, `vim`, `nano`, `htop`, `fastfetch`, `rsync`,
`openssh`, `unzip`, `pacman-contrib`, `reflector`). Example: `yay -S librewolf-bin`.

The `desilium` user in the Arch image has passwordless `sudo`, and `--arch` runs
the container so that it works: the root filesystem stays writable (for
`sudo pacman -S ...`), `no-new-privileges` is off and the capabilities sudo and
pacman need are added. Packages need `--net`. Pass `--no-sudo` for the fully
locked-down run (sudo then fails with "no new privileges"). The Debian image is
unaffected.

## Validation

Unit tests cover service-output parsing, invalid power-profile rejection,
explicit right-click action routing, pin persistence and conflicting writers.
Configuration tests cover all graphical panel preferences and old-document
defaults. Headless Blitz snapshots exercise taskbar and quick settings in both
light and dark themes. Tests do not toggle the developer's radios, volume or
physical display backlight.
