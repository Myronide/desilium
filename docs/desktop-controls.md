# Panels, dock and quick settings

The shell now offers two application-panel layouts:

- **Dock:** a floating, content-sized strip of pinned and running applications.
- **Taskbar:** a wide panel with optional application names, running indicators
  and a system/clock button.

Both use the existing application catalog, the window list that KWin provides
through `org_kde_plasma_window_management` (see
[kwin-integration.md](kwin-integration.md)), minimize targets (passed to KWin with
`set_minimized_geometry`) and the launcher. Without that protocol (refused by KWin, or
KWin not running) the panels still show pinned applications but no running windows. Left click launches, activates, cycles grouped windows or
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
| Reserve space for the panel (`shell.panel_reserve`) | On | On/off |
| Panel corner radius (`shell.panel_radius`) | 22 px | 0–36 px (never more than half the panel height) |
| Popup corner radius (`shell.popup_radius`) | 20 px | 0–36 px (launcher and quick settings, also the blur clip) |
| Top bar height (`shell.bar_height`) | 32 px | 24–56 px |
| Top bar: application (`bar_show_app`), window title (`bar_show_title`), date (`bar_show_date`), keyboard layout (`bar_show_layout`), power button (`bar_show_power`) | On | On/off each |

**Sliders.** Every numeric setting is a slider with its value (and unit) beside it: drag the thumb,
or click the track to jump to a value. While the thumb is dragged only the display follows; the
value is stored once, when the button is released, so one drag is one entry in the undo list.

**Shape.** Under Settings → Appearance → Shape, `appearance.corner_radius` (default 12 px, 0–32)
rounds windows, cards and large surfaces, and `appearance.control_radius` (default 8 px, 0–20)
rounds buttons, inputs and list rows. They override the radii of the chosen theme and feed the
`--radius-*` design tokens (control ×0.5, ×0.75, ×1, ×1.25; window ×1, ×4/3, ×5/3), so the
Settings app and every shell surface that uses a token follow. Fixed pixel radii inside the shell's
own style sheets (launcher rows, the desktop menu) do not change yet. KWin's window decoration is
not touched.

**More themes.** Settings → Get new themes downloads icon themes, cursor themes and colour schemes
from the KDE Store and selects them; see [theme-store.md](theme-store.md).

The top system panel remains available in both layouts. Moving the application
panel to the top places it below the system panel. Long application strips can
scroll horizontally. Auto-hide, vertical side docks and ungrouped per-window
task buttons are not yet implemented.

**Reserved space.** While the dock or taskbar is always visible and "Reserve space" is on, windows
end above it instead of lying under it, in both layouts. The panel's exclusive zone is its height
plus the edge margin: the compositor adds the margin on the screen side by itself, and the margin
in the zone leaves the same gap between the panel and the window above it, so the "Edge margin"
option sets the distance on both sides. With auto-hide or avoid-windows, or with the option off,
nothing is reserved and windows may use the whole output. The same holds for the top bar
(`bar_visibility = always` reserves its height). Whether KWin adds the anchored-edge margin to the
zone exactly like this is listed in the verification table in `kwin-integration.md`.

**Top bar content.** The bar shows the focused application only when its window is on the current
workspace (or on all of them); on an empty workspace it shows "Desktop". The workspace is taken
from the virtual desktops the window entered.

## Quick settings

Click the status group in the top panel, or the system button of the taskbar.
Escape or clicking elsewhere closes the panel. The layout is icons first and at most one
word per control: a top row with the settings button and the battery charge, a volume and
a brightness slider (the speaker icon mutes), and six pills in two columns: Wi-Fi (split:
the arrow unfolds the network list), Bluetooth, energy profile (a click cycles saver,
balanced, performance), microphone, dark style and taskbar mode. A lit (accent) pill means
on. A slider row or pill whose service is missing is left out or dimmed. The panel is as
tall as what it shows.

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
3-second timeout. State is refreshed when opening the panel and after an action;
there is no background control polling while closed.
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
workspace overlaps it, using the window geometry that
`org_kde_plasma_window` reports. Only `always` reserves screen space (see "Reserved space"
above); the other modes let windows use the full output. An open popup keeps its panel visible.
There is no slide animation yet.

## Mouse

Settings → Mouse (`page mouse`, the `[compositor.mouse]` table of `config.toml`), all as sliders,
switches or a segmented control:

| Setting | Key | Default | Range | KWin |
|---|---|---|---|---|
| Pointer speed | `pointer_speed` | 0 | -100 to 100 | `pointerAcceleration` = value / 100 (-1.0 to 1.0) |
| Acceleration | `accel_profile` | adaptive | adaptive, flat | `pointerAccelerationProfileFlat` = (flat) |
| Left-handed mouse | `left_handed` | off | on/off | `leftHanded` |
| Double-click interval | `double_click_ms` | 400 ms | 200 to 1000 | `kdeglobals` `[KDE] DoubleClickInterval` |
| Natural scrolling | `natural_scroll` | off | on/off | `naturalScroll` |
| Scroll speed | `scroll_speed` | 100 % | 25 to 400 | `scrollFactor` = value / 100 |

The bridge (`desilium-kwin`) sets the properties on every input device that KWin reports as a
pointer and not as a touchpad, and writes only the ones that differ. It does this at start, on every
configuration change and every 10 seconds, so a mouse that is plugged in later gets the settings
too. Touchpads are not affected. A device or KWin version without a property (`scrollFactor` is
newer than the others) is skipped for that property. The settings are not written to `kcminputrc`;
see the verification table in [kwin-integration.md](kwin-integration.md). **None of this has been
run against a real KWin.** Values outside the ranges are clamped when the configuration is loaded.

## Network

The Wi-Fi pill shows the name of the active Wi-Fi connection. Its arrow unfolds up to five
networks in range (strongest first). Selecting one runs `nmcli device wifi connect`, which
works for open and already saved networks; password entry is not implemented. Everything
goes through `nmcli`, so the pill is dimmed without it. An active Ethernet connection is
not shown separately any more.

## Desktop icons

The desktop shows the contents of the desktop folder: launchers (`.desktop` files), folders and
files. The folder is `XDG_DESKTOP_DIR` from `user-dirs.dirs`, else `~/Desktop`; it is created when
it is missing. If `XDG_DESKTOP_DIR` points at the home directory, the desktop folder is off, as the
specification says. `shell.desktop_icons` (Settings → Desktop → "Show icons on the desktop", on by
default) shows or hides the icons.

The icons are drawn in the wallpaper's layer surface on the first output, so the shell needs no
extra window. They sit in a grid that is fitted to the free area of the screen (between the bar and
the dock, with a 12 px margin): the cell size is at least 88 × 96 px and grows so that whole cells
fill the area without a ragged edge, and it is computed again when the output, the bar or the dock
changes. Icons without a saved place start with folders first and then by name, filling a column
from the top before starting the next; an icon that appears later takes the first free cell
instead of shifting the others. Hidden files (names starting with a dot) are not shown, and at
most 512 icons are.

A moved icon is remembered by its file name in `desktop-layout.toml` in the state directory
(`$XDG_STATE_HOME/desilium/`). A missing or unreadable file only means the default order; entries of
files that are gone are dropped.

| Action | How |
|---|---|
| Select | Click; click the empty desktop to clear |
| Select several | Press on the empty desktop and drag a rectangle (rubber band); every icon it touches is selected |
| Move | Drag an icon (a selection moves together) onto a free cell; it snaps to the grid, a taken cell sends it to the nearest free one, and nothing leaves the grid |
| Open | Double click, or Open in the menu. A launcher is started, a folder or file is opened with `xdg-open` |
| Menu | Right click an icon (Open, Allow launching, Rename, Move to Trash) or the empty desktop (New folder, New file, Open desktop folder, Desktop settings) |
| Create | New folder and New file make "New Folder", "New File.txt" (numbered when taken) and start renaming them. When the menu was opened on the empty desktop, the new item appears in the cell under the pointer (the nearest free one if that is taken) |
| Rename | Rename in the menu, type, Enter. A name with `/`, an empty name or an existing name is refused with a message, never replaced |
| Remove | Move to Trash runs `gio trash` for every selected icon; without `gio` the shell says so |
| Reorder the dock | Drag a pinned application's icon along the dock and drop it at its new place; the order is stored in the pin list. Running, unpinned applications cannot be dragged |
| Add an application | Right click its dock icon and choose "Add to desktop": the `.desktop` file is copied onto the desktop and marked as trusted. An existing launcher of that name is kept |
| Add files | Put them in the desktop folder with any file manager (Doria, for example); the folder is watched and they appear within a fraction of a second |

**Trust.** A launcher is only started by a double click when its executable bit is set, which is
how a launcher you put there yourself (Add to desktop, or `chmod +x`) differs from one that arrived
in a download. An untrusted launcher is drawn dimmed, and its menu offers "Allow launching", which
sets the owner's execute bit. Double clicking it only explains this.

Every change goes through the file name inside the desktop folder, never a path, so the desktop
cannot rename or remove anything outside it.

Dragging works through a generic drag event of the UI layer (`data-drag` on an element; a press
that moves more than 5 px becomes a drag with start, move and end phases). Dropping files from
another application onto the desktop (the Wayland data-device protocol) is not implemented, and
neither is dragging a launcher from the dock onto the desktop (use "Add to desktop"), the keyboard
(arrow keys, F2, Delete), thumbnails of images, and the other outputs: they show the wallpaper only.
Drag and drop, the rubber band and the grid have unit and snapshot tests with synthetic pointer
events; they have not been tried with a real pointer against a running KWin. The rename field needs
keyboard focus on a background-layer surface, which was written for `on_demand` keyboard
interactivity and has not been tried against a running KWin.

## Restoring the session

`shell.restore_session` (Settings → Desktop → Session, off by default) saves which
applications have windows open, with their workspace, position, size and minimized
state, to `$XDG_STATE_HOME/desilium/session.toml` three seconds after the last window
change. At the next start on a desktop without windows the shell launches those
applications again and puts each new window back, matched by Wayland app id. The workspace
comes from the virtual desktop protocol; the position and size are set by a one-shot KWin
script, addressed by the window's uuid (`desilium_kwin::windows::set_geometry`, see
[kwin-integration.md](kwin-integration.md#6-placing-windows-set_geometry)), because the
window protocol cannot set a geometry. That script path is unverified against a live KWin.
Windows that do not open within a minute are skipped. Only applications found in the
desktop-file catalog are saved.

This restores the arrangement of the desktop, like "reopen windows" on macOS. It does
not freeze and thaw processes: a program's own state (browser tabs, unsaved text) is
only back if the program restores it itself. A shell that merely restarts on a running
desktop never moves or duplicates windows.

## Arch Linux test image

`scripts/docker-run.sh --arch` builds `docker/Dockerfile.arch`: the same
binaries on an Arch runtime with pacman-installed KWin (`kwin`, `kglobalacceld`, `kconfig`,
`xorg-xwayland`), NetworkManager, WirePlumber, brightnessctl and bluez-utils. The container runs
`desilium-session`, which starts KWin nested in your Wayland session. The images were not
built or run in the environment where this documentation was updated. Add `--net` to attach the container to the bridge
network (the default is no network) so the Ethernet state can be tested.
The container has no Wi-Fi radio, so only the Ethernet path is exercisable there.

Besides pacman the image has the AUR helpers `yay` and `paru` (`yay-bin`, and `paru` built from
source because `paru-bin` breaks when pacman's libalpm moves on; both are built while
the image is built, which needs network access and a few minutes), `base-devel`,
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
