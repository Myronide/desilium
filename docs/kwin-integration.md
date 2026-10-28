# KWin integration

Desilium has no compositor of its own. `kwin_wayland` (KDE Plasma 6) is the compositor and window
manager. This document describes how Desilium sits on top of it: which process does what, which
protocols and D-Bus interfaces are used, how the typed configuration reaches KWin, and what is not
verified yet.

> **Read [Verification status](#verification-status) first.** The code described here compiles
> and is unit-tested, but it has never been run against a live KWin or `kglobalacceld`. Many KDE-side
> names below come from memory.

Code: `crates/desilium-kwin` (the bridge, a library and the binary `desilium-kwin`),
`crates/desilium-shell/src/services/protocol/` (the shell's protocol client),
`crates/desilium-shell/src/wallpaper.rs`, `crates/desilium-session/src/startup/`.

## 1. Process model

```text
login manager  ->  desilium-session  (supervisor, own process, owns restart policy)
                      |-- kwin_wayland          critical, Wayland provider, started first
                      |-- desilium-shell        restartable, Wayland client
                      |-- desilium-kwin         restartable, "kwin-bridge"
                      `-- desilium-notifications restartable, D-Bus service
```

| Process | Role towards KWin |
|---|---|
| `kwin_wayland` | Compositor, window manager, input, outputs, XWayland. Started with `--xwayland --no-lockscreen`, plus anything given after `--` to `desilium-session`. |
| `desilium-shell` | Wayland client. Reads the window list and desktops, sends window actions, draws the wallpaper, asks for blur behind its panels. |
| `desilium-kwin` | Projects `config.toml` onto KWin and KDE, registers global shortcuts, handles two of them, places windows on request of the shell (library part). |
| `desilium-session` | Starts KWin first, waits for its Wayland socket, then starts the clients. A KWin exit ends the session. Shell, bridge and notification exits are restarted with backoff. |

`desilium-config` is a library used by the shell, the settings app and the bridge; there is no
separate configuration process today. Configuration changes reach KWin through the bridge's inotify watch on `config.toml`.

Consequences:

- If the shell or the bridge dies, KWin and all application windows keep running.
- If KWin dies, the session ends. A compositor cannot be restarted without losing every client.
- The bridge and the shell are independent of each other. Each works, in a reduced way, without the
  other.

## 2. Protocols and interfaces

### Wayland protocols (shell, client side)

Bound through the crate `wayland-protocols-plasma`. The protocol XML names come from that crate and
are verified; the version ranges are what the shell accepts.

| Interface | Versions bound | Used for |
|---|---|---|
| `org_kde_plasma_window_management` | 13 to 18 | Window list (title, app id, state, geometry, desktops, uuid), activate, close, minimize and restore (`set_state`), move between desktops (`request_enter_virtual_desktop` / `request_leave_virtual_desktop`), `set_minimized_geometry` for the minimize target. Version 13 is the oldest with `window_with_uuid` / `get_window_by_uuid`. |
| `org_kde_plasma_virtual_desktop_management` | 1 to 2 | Desktop list and switching (`request_activate`). |
| `org_kde_kwin_blur_manager` | 1 | Backdrop blur behind the shell's surfaces. |
| `wl_compositor` | 1 to 4 | Creating the `wl_region` for blur. |

Windows with the `skiptaskbar` state (panels, docks, the shell itself) are not listed.

The objects live on their own event queue of the connection the UI host uses, so the shell still
has one socket to poll.

Besides these, the shell and the other Desilium applications use the standard protocols for their
own surfaces (layer-shell for the panels and the wallpaper, xdg-shell for windows) through
`desilium-ui`. KWin has to offer them; that has not been checked either.

### D-Bus

| Service | Object / interface | Used by | For |
|---|---|---|---|
| `org.kde.KWin` | `/KWin`, `org.kde.KWin`, method `reconfigure` | bridge | Make KWin re-read `kwinrc` after keys were written. |
| `org.kde.KWin` | `/VirtualDesktopManager`, `org.kde.KWin.VirtualDesktopManager`, properties `count` and `rows` | bridge | Set the number of virtual desktops; rows is set to 1. Only written when it differs. |
| `org.kde.KWin` | `/org/kde/KWin/InputDevice`, `org.kde.KWin.InputDeviceManager`, property `devicesSysNames` | bridge | List the input devices (`event5`, ...). |
| `org.kde.KWin` | `/org/kde/KWin/InputDevice/<sysName>`, `org.kde.KWin.InputDevice`, properties `pointer`, `touchpad` (read) and `pointerAcceleration`, `pointerAccelerationProfileFlat`, `naturalScroll`, `leftHanded`, `scrollFactor` (read, written when different) | bridge | Mouse settings (see [desktop-controls.md](desktop-controls.md#mouse)). Only devices with `pointer` true and `touchpad` false. A device without one of the properties is skipped for it. |
| `org.kde.KWin` | `/Scripting`, `org.kde.kwin.Scripting`, methods `loadScript`, `unloadScript`; `/Scripting/Script<N>`, `org.kde.kwin.Script`, method `run` | shell, via `desilium_kwin::windows` | Place a window at an exact rectangle (section 6). |
| `org.kde.kglobalaccel` | `/kglobalaccel`, `org.kde.KGlobalAccel`, methods `doRegister`, `setShortcut`, `setForeignShortcut` | bridge | Register shortcuts (section 5). |
| any sender | interface `org.kde.kglobalaccel.Component`, signal `globalShortcutPressed(component, action, timestamp)` | bridge, shell | Receive presses of Desilium's own shortcuts. Subscribed by interface and member only; entries whose component is `desilium` are kept. |

### Command-line tools

`kwriteconfig6` writes the KConfig files (section 4). It is started with a fixed argument vector,
never through a shell, and killed after 5 s.

## 3. Permission mechanism

KWin does not offer `org_kde_plasma_window_management` and `org_kde_plasma_virtual_desktop_management`
to every client. It offers them to executables that a `.desktop` file names and that list the
interface in `X-KDE-Wayland-Interfaces`. Desilium ships
`packaging/applications/org.desilium.Shell.desktop`:

```ini
Exec=desilium-shell
X-KDE-Wayland-Interfaces=org_kde_plasma_window_management,org_kde_plasma_virtual_desktop_management
```

This replaces the old name-based trust list of the private compositor protocol. The executable is
matched through the `Exec` line, so `desilium-shell` must be installed in a directory on `PATH`.
`org_kde_kwin_blur_manager` needs no such entry.

For development, `desilium-session --allow-any-client` sets `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1`
in the session environment, which makes KWin grant these protocols to every client (use it for a
shell that runs from `target/`, or in a container). It is off by default.

If the protocols are refused, the shell logs a warning and runs without a window list and with one
workspace (section 8).

## 4. Configuration to KWin mapping

The typed configuration (`config.toml`, `desilium-config-types`) stays the single source of truth.
The bridge turns it into a plan of KConfig entries (`crates/desilium-kwin/src/plan/`), which is pure
and unit-tested, and applies the difference. The full list for a configuration is printed with:

```sh
cargo run -q -p desilium-kwin -- --print-plan --config-dir target/empty-config
```

### Window behaviour and input

| Setting | File | Group | Key | Value |
|---|---|---|---|---|
| `compositor.focus` | `kwinrc` | `Windows` | `FocusPolicy` | `ClickToFocus` for `click`, `FocusFollowsMouse` for `sloppy` |
| (fixed) | `kwinrc` | `Windows` | `ClickRaise` | `true` |
| `compositor.focus` and `raise_on_focus` | `kwinrc` | `Windows` | `AutoRaise` | `true` only for sloppy focus with raise on focus, otherwise `false` |
| `snap_enabled`, `snap_distance` | `kwinrc` | `Windows` | `BorderSnapZone` | the distance, or `0` when snapping is off |
| `snap_enabled`, `snap_distance` | `kwinrc` | `Windows` | `WindowSnapZone` | the distance, or `0` when snapping is off |
| (fixed) | `kwinrc` | `Windows` | `CenterSnapZone` | `0` |
| `snap_enabled` | `kwinrc` | `Windows` | `ElectricBorderMaximize` | `true` / `false` |
| `snap_enabled` | `kwinrc` | `Windows` | `ElectricBorderTiling` | `true` / `false` |
| `keyboard_repeat_delay` | `kcminputrc` | `Keyboard` | `RepeatDelay` | milliseconds |
| `keyboard_repeat_rate` | `kcminputrc` | `Keyboard` | `RepeatRate` | repeats per second |
| `mouse.double_click_ms` | `kdeglobals` | `KDE` | `DoubleClickInterval` | milliseconds |
| `mouse.*` (rest) | D-Bus | `/org/kde/KWin/InputDevice/<sysName>` | `pointerAcceleration` (`pointer_speed / 100`, -1.0 to 1.0), `pointerAccelerationProfileFlat` (`accel_profile = flat`), `naturalScroll`, `leftHanded`, `scrollFactor` (`scroll_speed / 100`) | live properties of every mouse; re-applied at start, on every change and every 10 s for devices plugged in later |
| (fixed) | `kxkbrc` | `Layout` | `Use` | `true` |
| `keyboard_layout` | `kxkbrc` | `Layout` | `LayoutList` | `de` or `us` |
| (fixed) | `kxkbrc` | `Layout` | `VariantList` | empty |
| `desktops` | D-Bus | `/VirtualDesktopManager` | `count` | clamped to 1 to 32; `rows` is set to 1 |

### Animation, cursor and effects

| Setting | File | Group | Key | Value |
|---|---|---|---|---|
| `animations`, `animation_speed`, `reduce_motion` | `kdeglobals` | `KDE` | `AnimationDurationFactor` | `0` when animations are off. Otherwise `1 / speed` with speed clamped to 0.25 to 4, and at most `0.35` when reduced motion is on. Written with up to two decimals (`1`, `0.5`, `2`). |
| `cursor_size` | `kcminputrc` | `Mouse` | `cursorSize` | pixels |
| `cursor_theme` | `kcminputrc` | `Mouse` | `cursorTheme` | only written when a theme is chosen |
| (fixed) | `kwinrc` | `Plugins` | `blurEnabled` | `true` |

### Icons

| Setting | File | Group | Key | Value |
|---|---|---|---|---|
| `icon_theme` | `kdeglobals` | `Icons` | `Theme` | only written when a theme is chosen, so Qt and KDE applications use the shell's icons |

### Colours

With `theme = "kde"` the direction reverses: the bridge writes no colours at all (none of the
groups below, `ColorScheme`, `AccentColor` or `WM`), and the shell and the settings window read
the user's KDE colour scheme instead (see [KDE themes](#kde-themes-and-icons)). With any other
theme:

The resolved Desilium theme (built-in id plus optional accent, `--theme` of the bridge overrides the
id) becomes a KDE colour scheme, so Qt and KDE applications match the shell. The keys are written
straight into `kdeglobals`; a scheme file is only the packaged form of the same keys. Colours use
KConfig's decimal `R,G,B` notation, alpha dropped.

| File | Group | Key | Value |
|---|---|---|---|
| `kdeglobals` | `General` | `ColorScheme` | `DesiliumDark` or `DesiliumLight` |
| `kdeglobals` | `General` | `AccentColor` | `palette.accent` |
| `kdeglobals` | `WM` | `activeBackground` / `activeForeground` | `surface_raised` / `text_primary` |
| `kdeglobals` | `WM` | `inactiveBackground` / `inactiveForeground` | `surface` / `text_secondary` |

For each group `Colors:View`, `Colors:Window`, `Colors:Button`, `Colors:Tooltip`,
`Colors:Complementary`, `Colors:Header` and `Colors:Selection`, twelve keys are written:

| Key | Value |
|---|---|
| `BackgroundNormal` / `BackgroundAlternate` | per group, see the next table |
| `DecorationFocus` | `accent` |
| `DecorationHover` | `accent_hover` |
| `ForegroundNormal` | `text_primary` |
| `ForegroundInactive` | `text_secondary` |
| `ForegroundActive`, `ForegroundLink` | `accent` |
| `ForegroundVisited` | `accent_pressed` |
| `ForegroundNegative` / `ForegroundNeutral` / `ForegroundPositive` | `danger` / `warning` / `success` |

| Group | `BackgroundNormal` | `BackgroundAlternate` |
|---|---|---|
| `Colors:View` | `surface` | `surface_raised` |
| `Colors:Window` | `surface` | `surface_raised` |
| `Colors:Button` | `surface_raised` | `overlay` |
| `Colors:Tooltip` | `overlay` | `surface_raised` |
| `Colors:Complementary` | `background` | `surface` |
| `Colors:Header` | `surface_raised` | `surface` |
| `Colors:Selection` | `accent` | `accent_hover` |

In `Colors:Selection` the keys `ForegroundNormal`, `ForegroundInactive`, `ForegroundActive`,
`ForegroundLink` and `ForegroundVisited` are all `text_on_accent`.

### KDE themes and icons

Desilium can follow the user's KDE theme instead of pushing its own into KDE.

**Colours.** `appearance.theme = "kde"` (the "KDE" choice in Settings → Appearance → Color
scheme) makes the shell and the settings window wear the active KDE colour scheme.
`desilium_theme::kde` reads `kdeglobals` from `$XDG_CONFIG_HOME`, then `$XDG_CONFIG_DIRS`
(earlier files win). Without `[Colors:Window]` in there it reads the scheme file named by
`[General] ColorScheme` from `color-schemes/NAME.colors` in `$XDG_DATA_HOME` and
`$XDG_DATA_DIRS`. The palette is derived like this: `surface` is `Colors:Window`
`BackgroundNormal`, `surface_raised` the button background, `overlay` the tooltip background,
`accent` the selection background, `text_on_accent` the selection foreground, text from
`Colors:Window` `ForegroundNormal` and `ForegroundInactive`, the status colours from
`ForegroundPositive`, `ForegroundNeutral` and `ForegroundNegative`. Light or dark follows the
luminance of the window colour. `appearance.accent` is ignored in this mode. When no usable
scheme exists the dark built-in theme is used and a warning is logged. The shell watches the
directory of `kdeglobals` and re-reads after 300 ms of quiet, so switching the scheme in KDE's
settings is picked up live.

**Icons.** Application icons come from an icon theme, resolved in this order:
`DESILIUM_ICON_THEME` in the environment, `appearance.icon_theme`, `[Icons] Theme` from
`kdeglobals`, then `breeze` (KDE's default). The lookup follows the theme's `Inherits`, then
`hicolor`, then `pixmaps`. Every base directory (`$XDG_DATA_HOME/icons`, the `XDG_DATA_DIRS`
entries, Flatpak's exports, `~/.icons`) is searched for each theme directory, so icons that
applications install into `~/.local/share/icons/hicolor` or Flatpak's `hicolor` are found even
though only one of them carries the `index.theme`. Theme indexes may list up to 4096 directories
(the real `hicolor` has more than 256; a limit of 256 once made the whole theme unusable and left
only `pixmaps` and Breeze, so that most applications showed no icon). `ScaledDirectories` is read
too. A changed choice, in the settings or in KDE, is applied without a restart.

**No invented icons.** The shell draws what the application ships. For each entry it tries, in
order: the icon its desktop file names, then names derived from the desktop id or the Wayland app
id (`org.kde.Dolphin.desktop` gives `org.kde.Dolphin`, `org.kde.dolphin`, `dolphin`), and for a
window without a desktop file the icon name KWin reports (`themed_icon_name_changed`). If none is in
the theme it takes the theme's generic `application-x-executable`; only if the theme has not even
that does it draw a neutral outline glyph. The former gradient tiles chosen from an application's
categories are gone. Pixmap icons that KWin only offers through `icon_changed` and `get_icon`
(Xwayland windows without an icon name) are not read; those windows show the generic icon.

### Settings not mapped onto KWin

| Setting | Handled by |
|---|---|
| `compositor.terminal` | The bridge, for the `launch-terminal` shortcut. Unset means auto-detect. |
| `appearance.wallpaper`, `wallpaper_mode` | The shell (section 7). KWin has no wallpaper. |
| `shell.*` | The shell. |

### How it is applied

1. The plan is built from the configuration. Invalid themes and accents are replaced and reported
   as warnings, as in the shell.
2. Only entries that differ from what this bridge process wrote before are written. Each is one
   `kwriteconfig6 --file F --group G --key K [--type bool] --notify VALUE` call. If that call
   fails, it is repeated without `--notify` so that a `kwriteconfig6` that does not know the option
   does not lose the setting. A value that starts with `-` is preceded by `--`.
3. After at least one write, KWin is asked to `reconfigure` over D-Bus.
4. The desktop count is set over D-Bus when it differs.
5. Shortcuts are registered when the set of bindings differs.
6. A failed step does not stop the others. It is logged and retried: after 1 s, doubling up to 30 s,
   at most 8 attempts, then the bridge waits for the next change of `config.toml`. This covers
   KWin or `kglobalacceld` not being up yet when the session starts.

The bridge applies once at start (this is also all that `--once` does), then watches the
configuration directory with inotify. Events for `config.toml` start a 150 ms debounce; the file is
re-read through the normal store, so a rejected file keeps the last good configuration.

## 5. Shortcut model

The configured shortcuts (`compositor.shortcuts`: modifiers, key, action) are translated into
KGlobalAccel key lists. Keys are Qt key codes or-ed with Qt modifier bits (Shift `0x02000000`,
Ctrl `0x04000000`, Alt `0x08000000`, Meta `0x10000000`). Several shortcuts for one action share one
binding (duplicates collapse, at most four keys per action). A key without a Qt code is skipped and
logged. Shift+Tab is stored as Qt's separate Backtab key.

Two kinds of target exist:

- **KWin's own actions** (component `kwin`) are bound with `setForeignShortcut`. KWin already
  implements the action; Desilium only changes the keys.
- **Desilium's own actions** (component `desilium`) are registered with `doRegister` and
  `setShortcut` (flags `SetPresent | NoAutoloading`, so the configured keys win over stored ones).
  A press arrives as the `globalShortcutPressed` signal.

| Action (`config.toml`) | Component | KGlobalAccel action | Handled by | Default keys |
|---|---|---|---|---|
| `close-focused-window` | kwin | `Window Close` | KWin | Alt+Q |
| `toggle-maximize` | kwin | `Window Maximize` | KWin | Alt+Up |
| `toggle-fullscreen` | kwin | `Window Fullscreen` | KWin | Alt+F |
| `minimize-window` | kwin | `Window Minimize` | KWin | Alt+Down |
| `snap-left` | kwin | `Window Quick Tile Left` | KWin | Alt+Left |
| `snap-right` | kwin | `Window Quick Tile Right` | KWin | Alt+Right |
| `cycle-windows-forward` | kwin | `Walk Through Windows` | KWin | Alt+Tab |
| `cycle-windows-backward` | kwin | `Walk Through Windows (Reverse)` | KWin | Alt+Shift+Tab |
| `switch-desktop-next` | kwin | `Switch to Next Desktop` | KWin | Alt+Ctrl+Right |
| `switch-desktop-prev` | kwin | `Switch to Previous Desktop` | KWin | Alt+Ctrl+Left |
| `switch-to-desktop1` to `4` | kwin | `Switch to Desktop 1` to `4` | KWin | Alt+1 to 4 |
| `move-to-desktop1` to `4` | kwin | `Window to Desktop 1` to `4` | KWin | Alt+Shift+1 to 4 |
| `launch-terminal` | desilium | `launch-terminal` | bridge | Alt+Enter |
| `quit` | desilium | `quit` | bridge | Alt+Shift+Q |
| `toggle-launcher` | desilium | `toggle-launcher` | shell | Alt+Space |
| `restore-minimized` | desilium | `restore-minimized` | shell | Alt+Shift+M |

The shell and the bridge each subscribe to the signal (a `ShortcutListener` thread) and act only on
their own actions.

- `launch-terminal` starts the configured terminal, or the first one found.
- `quit` sends SIGTERM to the parent process, but only when that parent is named `desilium-session`.
  A bridge started by hand ignores the shortcut, so it never signals somebody's shell.

Alt is the default modifier rather than the Logo key because Desilium is developed nested inside
another desktop, which already uses the Logo key.

Shortcuts removed from the configuration are not unregistered: KWin keeps its own default binding
for its actions (section 9).

## 6. Placing windows: `set_geometry`

The window-management protocol can move a window to a desktop but cannot set its position and
size, and session restore needs that. The shell therefore calls
`desilium_kwin::windows::set_geometry(uuid, rect)` on a helper thread:

1. The uuid that `org_kde_plasma_window` reported is validated (only hex digits and `-`, braces
   tolerated, at most 40 characters), so it can be put into script source safely.
2. A small JavaScript file is written to `$XDG_RUNTIME_DIR/desilium/`. It loops over
   `workspace.windowList()`, finds the window whose `internalId` equals the uuid and assigns
   `frameGeometry = { x, y, width, height }`. Sizes below 1 are raised to 1.
3. `loadScript` on `org.kde.KWin /Scripting` loads it, `run` on `/Scripting/Script<N>` runs it, and
   `unloadScript` removes it. The file is deleted afterwards.

Used by session restore (`shell.restore_session`), together with `move_to_workspace` and the
minimize request. Failures are logged as a warning; the window stays where KWin put it.

## 7. Wallpaper and blur

**Wallpaper.** KWin draws no wallpaper. The shell creates one layer-shell surface on the Background
layer per output (`namespace desilium-wallpaper`). The image is an `<img>` fitted with CSS
`object-fit`: `fill` is `cover`, `fit` is `contain`, `stretch` is `fill`, `center` is `none`. The
theme's background gradient shows behind and while the image loads. Changing the wallpaper or theme
rebuilds the surfaces.

**Blur.** For surfaces that ask for a backdrop effect, the shell creates an `org_kde_kwin_blur` and
sets a `wl_region`. The protocol takes only a union of rectangles, so rounded corners are
approximated by one rectangle per pixel row of each corner (adjacent rows with equal inset are
merged). Only zero or non-zero blur strength matters to KWin; the radius is not passed on. Without
the blur manager, panels are simply not blurred.

**Minimize target.** The dock icon rectangle is passed with `set_minimized_geometry`, so KWin's
minimize animation can aim at it.

## 8. Failure behaviour

| Situation | What happens |
|---|---|
| `kwin_wayland` missing | `desilium-session` fails at plan time with an error naming the binary. |
| KWin exits | The session ends. |
| Window or desktop protocol refused (no `X-KDE-Wayland-Interfaces` entry) | The shell logs a warning, shows no windows and one workspace; the rest of the shell works. |
| None of the three KDE protocols offered | The shell runs without any of them. |
| Blur manager missing | No blur, logged at info level. |
| KWin not reachable on D-Bus yet | The bridge retries with backoff (section 4) and keeps running. |
| `kglobalacceld` not running | Shortcut registration fails and is retried; the other settings are still applied. Desilium's own shortcuts cannot be received. |
| `kwriteconfig6` missing or failing | The entry is counted as failed, logged, retried; other entries proceed. |
| Unknown key in a shortcut | The shortcut is skipped and logged. |
| Invalid configuration file | The last good configuration stays in effect (the store's normal recovery). |
| Bridge or shell crash | Restarted by the supervisor with backoff; KWin is unaffected. A restarted bridge writes everything again, since it keeps no record across restarts. |

None of these failures prevents the desktop from starting.

## 9. Development

`desilium-session` flags that matter here:

| Flag | Meaning |
|---|---|
| `--compositor <path>` | Use this `kwin_wayland` instead of the one found next to the session binary or on `PATH`. |
| `--shell <path>`, `--kwin-bridge <path>`, `--notifications <path>` | Same for the other components. |
| `--allow-any-client` | Export `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1` (development only). |
| `-- <args>` | Arguments appended after KWin's defaults (`--xwayland --no-lockscreen`), for example `--width 1600 --height 900` or `--virtual`. |
| `--dry-run[=json]`, `--print-env` | Show the plan or the environment without starting anything. |

Bridge flags (`desilium-kwin`): `--config-dir DIR`, `--theme dark|light`, `--once`, `--print-plan`.

Typical flows:

```sh
# KWin nested in your current Wayland session (KWin picks its nested backend because
# WAYLAND_DISPLAY points at the host), using the built binaries
cargo build --workspace
target/debug/desilium-session --allow-any-client

# the same in the locked-down container
scripts/docker-run.sh --allow-any-client -- --width 1600 --height 900

# look at what would be written, without KWin
target/debug/desilium-kwin --print-plan --config-dir /tmp/empty

# write into a throw-away configuration home (needs KWin and kwriteconfig6)
XDG_CONFIG_HOME=/tmp/desilium-xdg target/debug/desilium-kwin --once
```

Take care with the last two when KWin runs: `kwriteconfig6` writes to `$XDG_CONFIG_HOME` of the
caller. A nested KWin shares your real `~/.config` unless the session environment is isolated, so
the bridge would change the host desktop's `kwinrc`, `kdeglobals`, `kcminputrc` and `kxkbrc`.
Inside the container the files are the container user's own, in `/home/dev/.config` (the XDG
defaults below the home), which is a named Docker volume that persists between runs and is never
a host path. `scripts/docker-run.sh --fresh-home` uses a throw-away home instead, `--reset-home`
deletes the persistent one, and `--save` keeps the whole system, home included, in `pseudo-<distro>/system.tar` in
the repository (see [testing.md](testing.md#the-development-user-space)). On the host, set
`XDG_CONFIG_HOME` to a throw-away directory for development runs. The container images have not
been built in the authoring environment.

## 10. What changed and known gaps

Compared with the former Smithay-based compositor:

- **Lost: Desilium's own animations** and per-window transitions (open, close, maximize,
  minimize, focus cross-fade). KWin's effects are used; Desilium sets only their speed and on/off.
- **Lost: compositor-drawn glass popups.** Popups and translucent shell surfaces get blur only
  through the KWin blur protocol, with approximated corners.
- **Lost: Desilium-drawn window decorations.** KWin's decorations (Breeze unless changed) are
  used, with the Desilium colours exported through the colour scheme. A Desilium decoration theme or
  a KDecoration3 plugin is a possible future item.
- **Lost: the compositor-side window switcher.** Alt+Tab is KWin's Walk Through Windows.
- **Lost: the headless screenshot mode** of the old compositor, together with `scripts/docker-screenshot.sh`.
  There is no replacement yet (see [`headless.md`](headless.md)).
- **Lost: the `tile` wallpaper mode.** It is shown as `fill`.
- **Gap: wallpaper surfaces follow the outputs present when they are created.** Hot-plugging an
  output is not tracked.
- **Gap: shortcuts removed from the configuration keep KWin's default binding** for KWin's own
  actions; Desilium only sets keys, it does not clear them.
- **Gap: no lock screen.** KWin is started with `--no-lockscreen`.
- **Gap: the shell and the settings app are not guaranteed to match KWin's state at every moment.**
  The bridge applies on change and when it starts; a KWin restart without a session restart is not
  supported.
- **Gained:** a real DRM/KMS backend, outputs and HiDPI, XWayland, screencast and remote desktop
  through the KDE portal backend, and tested window management.

## 11. Verification status

**None of the KDE-side facts below could be verified in the environment where this integration was
written. There was no running KWin or `kglobalacceld`, and no way to look them up.** They come from
memory and are the likeliest places for the first real run to fail. Every one of them is
UNVERIFIED.

What is verified: the protocol XML interface and request names (they come from the
`wayland-protocols-plasma` crate and are checked by the compiler), the mapping logic itself (unit
tests), and the process handling (tests with stand-in scripts).

| Item | Status |
|---|---|
| Key names and groups in `kwinrc` (`Windows/FocusPolicy`, `ClickRaise`, `AutoRaise`, `BorderSnapZone`, `WindowSnapZone`, `CenterSnapZone`, `ElectricBorderMaximize`, `ElectricBorderTiling`, `Plugins/blurEnabled`) | UNVERIFIED |
| Key names in `kdeglobals` (`KDE/AnimationDurationFactor`, `General/ColorScheme`, `General/AccentColor`, the `Colors:*` groups and their twelve keys, `WM/*`) and that KDE applications pick them up without a scheme file | UNVERIFIED |
| The KDE theme reading (`theme = "kde"`): that `kdeglobals` carries `[Colors:*]` and `[Icons] Theme` the way the parser assumes, that scheme files live in `color-schemes/*.colors` below the data directories, that `breeze` is KDE's default icon theme, and that the shell's selection-colour-as-accent mapping looks right with real schemes (only a Breeze-like sample was tested) | UNVERIFIED (unit-tested against sample text) |
| What `org_kde_plasma_window.themed_icon_name_changed` reports for windows without a desktop file, and that an icon name resolves in the theme | UNVERIFIED |
| Key names in `kcminputrc` (`Keyboard/RepeatDelay`, `RepeatRate`, `Mouse/cursorSize`, `cursorTheme`) and `kxkbrc` (`Layout/Use`, `LayoutList`, `VariantList`) and that KWin applies them on `reconfigure` | UNVERIFIED |
| `kwriteconfig6` option names (`--file`, `--group`, `--key`, `--type bool`, `--notify`) and the fallback without `--notify` | UNVERIFIED |
| `org.kde.KWin` object `/KWin`, method `reconfigure` | UNVERIFIED |
| `/VirtualDesktopManager` interface, properties `count` and `rows` and that they are writable | UNVERIFIED |
| Mouse settings: that `/org/kde/KWin/InputDevice` with `org.kde.KWin.InputDeviceManager.devicesSysNames` lists the devices, that `org.kde.KWin.InputDevice` has the properties `pointer`, `touchpad`, `pointerAcceleration` (double, -1 to 1), `pointerAccelerationProfileFlat`, `naturalScroll`, `leftHanded` and `scrollFactor` (the last one only in newer KWin), that they are writable over D-Bus and take effect at once, and that `pointer` is also true for touchpads (so `touchpad` has to be false) | UNVERIFIED |
| Mouse settings are not written to `kcminputrc`: after a KWin restart they hold until the bridge applies them again (a few seconds), and plugged-in mice are found by polling every 10 s | by design; whether KWin keeps its own per-device values that win over ours is UNVERIFIED |
| `kdeglobals` `[KDE] DoubleClickInterval` is what Qt and KDE applications read for the double-click time | UNVERIFIED |
| Theme store (Settings → Get new themes): the KDE Store's OCS API (endpoint, category ids, field names, direct download links) and that themes installed to `~/.local/share/icons` and `color-schemes` are picked up by KWin and Qt; the full list is in [`theme-store.md`](theme-store.md#unverified) | UNVERIFIED |
| KGlobalAccel D-Bus names and signatures (`org.kde.kglobalaccel`, `/kglobalaccel`, `doRegister`, `setShortcut` with flags 0x2 and 0x4, `setForeignShortcut`) | UNVERIFIED |
| Signal `globalShortcutPressed` on interface `org.kde.kglobalaccel.Component`, its argument types `(s, s, x)`, and that it is delivered for a component that was registered this way | UNVERIFIED |
| KWin's action names (`Window Close`, `Window Maximize`, `Window Fullscreen`, `Window Minimize`, `Window Quick Tile Left/Right`, `Walk Through Windows` and `(Reverse)`, `Switch to Next/Previous Desktop`, `Switch to Desktop N`, `Window to Desktop N`) and the `kwin` component name | UNVERIFIED |
| Qt key code table (including Shift+Tab as Backtab) and that KGlobalAccel expects exactly this encoding | UNVERIFIED |
| Scripting interface: `/Scripting`, `org.kde.kwin.Scripting.loadScript`, `unloadScript`, `/Scripting/Script<N>`, `org.kde.kwin.Script.run`, the script API (`workspace.windowList()`, `window.internalId`, `frameGeometry`) and that `internalId` matches the uuid of the window protocol | UNVERIFIED |
| `X-KDE-Wayland-Interfaces` is matched through the desktop file's `Exec` line and `PATH`, and the interface names in that file are the ones KWin expects | UNVERIFIED |
| `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1` grants the restricted protocols | UNVERIFIED |
| `kwin_wayland --xwayland --no-lockscreen` are valid flags of the installed KWin, and KWin picks its nested backend when `WAYLAND_DISPLAY` is set | UNVERIFIED |
| Behaviour of the plasma window protocol versions 13 to 18 on the KWin that is installed | UNVERIFIED |
| Desktop icons: that KWin gives a Background-layer surface with `keyboard_interactivity=on_demand` keyboard focus on click (needed to type in the rename field), and that it delivers pointer clicks to such a surface at all when other surfaces cover parts of it | UNVERIFIED |
| Desktop icon drag, rubber band and dock pin drag: that a held button keeps delivering `wl_pointer.motion` to the layer surface after the pointer left it (implicit grab) and that the release arrives there | UNVERIFIED |
| Exclusive zones: that KWin adds the margin of the anchored edge to a positive `exclusive_zone` (the shell sends `height + margin` so the gap above the dock mirrors the gap below), that a surface with zone 0 or more is placed inside the area the top bar already reserved, and that a zone of -1 ignores it (the dock then adds the bar height to its own margin) | UNVERIFIED |
| `org_kde_plasma_window` sends no `virtual_desktop_entered` for a window on all desktops (the shell treats "no desktops" as "on all") | UNVERIFIED |

Also, the shell's wallpaper surface, its blur requests and its plasma protocol client were compiled
and unit-tested, but never run against a live KWin. The container images and `packaging/install.sh`
were not built or run in the environment where this was written.

First things to check on a machine with KWin 6:

1. `desilium-session --dry-run` shows `kwin_wayland` first and `--xwayland --no-lockscreen`.
2. In a throw-away configuration home, run `XDG_CONFIG_HOME=$(mktemp -d) desilium-kwin --once` and
   read the generated `kwinrc`, `kdeglobals`, `kcminputrc` and `kxkbrc`. Compare the keys with what
   System Settings writes when you change the same option by hand.
3. `qdbus6 org.kde.KWin` (or `busctl --user tree org.kde.KWin`) to confirm `/KWin`,
   `/VirtualDesktopManager` and `/Scripting` and their interfaces. `busctl --user introspect
   org.kde.kglobalaccel /kglobalaccel` for the KGlobalAccel methods.
4. After a run, check that the shortcuts show up in System Settings and that each default
   binding works, the Desilium ones (launcher, restore, terminal, quit) included.
5. Run the shell with and without `--allow-any-client` and the desktop file; confirm that the window
   list appears only in the permitted case and that the warning is logged otherwise.
6. Restore a session with `shell.restore_session` on to check `set_geometry`.
7. Check the wallpaper on every output and the blur of the panels and popups.
8. Run a nested KWin whose `XDG_CONFIG_HOME` is isolated, so the host configuration stays untouched.

When an item is checked, record the KWin version and change its status here.
