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

## Validation

Unit tests cover service-output parsing, invalid power-profile rejection,
explicit right-click action routing, pin persistence and conflicting writers.
Configuration tests cover all graphical panel preferences and old-document
defaults. Headless Blitz snapshots exercise taskbar and quick settings in both
light and dark themes. Tests do not toggle the developer's radios, volume or
physical display backlight.
