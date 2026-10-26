# Shell protocol `desilium-shell-v1`

Private Wayland protocol between the compositor and `desilium-shell` (top bar, dock, launcher,
window switcher). It covers what `wlr-layer-shell` cannot express. XML:
`crates/desilium-protocols/protocols/desilium-shell-unstable-v1.xml` (every request, event and
enum is documented there). Compositor side: `crates/desilium-compositor/src/state/shell/`.

Shell absence is a normal state: all shell state is derived on demand and nothing in the
compositor waits for the shell.

## Using the bindings

`desilium-protocols` has two features: `server` (compositor) and `client` (shell). Both expose
the interfaces under `desilium_protocols::{server,client}::{desilium_shell_manager_v1,
desilium_toplevel_handle_v1}`. They are generated at build time with `wayland-scanner` 0.31, matching
the versions of Smithay 0.7 (`wayland-server` 0.31, `wayland-backend` 0.3).

## Trust model

The global `desilium_shell_manager_v1` is only advertised to trusted clients (`GlobalDispatch::can_view`).
A client is trusted when, at connection time, `/proc/<pid>/exe` (pid from the socket credentials)
has the file name `desilium-shell` (list: `TRUSTED_EXECUTABLES` in `state/shell/trust.rs`), with a
trailing ` (deleted)` ignored. Setting `DESILIUM_SHELL_ALLOW_ANY=1` in the compositor's environment
trusts every client (development only; logged as a warning at start-up).

Limits: this is a guard against accidental or casual access by other programs of the same user,
not a security boundary. The check is by file name, so any program of the same user can copy a
binary to that name; the decision is made once at connect time, so a process that connects and
then `exec`s keeps the verdict of its old executable. The Wayland socket is already restricted to the
user by its permissions. A stronger scheme (a socket handed to the shell by `desilium-session`,
or a security context) can replace the policy without changing the protocol.

## Interfaces

- `desilium_shell_manager_v1` (global, v1). Events: `toplevel(new handle)`, `workspaces(count, active)`,
  `output_geometry(output rect, work area rect, scale_120ths)`, `shortcut(name)`, `done`.
  Requests: `switch_workspace(index)`, `set_surface_effects(layer surface, blur, 4 radii)`, `destroy`.
- `desilium_toplevel_handle_v1`. Events: `window_id`, `title`, `app_id`, `state(array)`, `workspace`,
  `closed`, `done`. Requests: `activate`, `close`, `set_minimized`, `set_maximized`,
  `move_to_workspace`, `set_minimize_rectangle(layer surface or null, x, y, w, h)`, `destroy`.

States: `activated`, `minimized`, `maximized`, `fullscreen`. A minimized window is never also
activated, maximized or fullscreen. `activated` is only set for the focused window of the workspace in view.

Workspace indices are zero-based. Out-of-range indices are ignored (not protocol errors), because the
count can shrink at any time. Real protocol errors (`invalid_surface`, `invalid_rectangle`) are for
programming mistakes only.

## Event flow

1. The shell binds the manager. The compositor sends the complete state: `toplevel` for every window
   (each handle gets `window_id`, `title`, `app_id`, `state`, `workspace`, `done`), then `workspaces`,
   `output_geometry`, and finally the manager `done`.
2. Afterwards only differences are sent. The compositor keeps the last snapshot it sent per client and
   compares it with the current one (`ShellSnapshot`, `state/shell/diff.rs`). A changed window gets only
   its changed properties plus `done`; a closed window gets `closed` + `done`; every batch ends with the
   manager `done`. Nothing is sent when nothing changed.
3. The comparison runs in exactly one function, `DesiliumState::shell_sync`. It is called after each batch
   of dispatched client requests and after each presented frame (`finish_frame`), which together cover
   client, input and animation driven changes, and it returns at once when no shell is bound.
   Consequently the shell sees a change at the latest one frame after it happened, and several changes
   within one dispatch round arrive in one batch.
4. `window_id` is the compositor's window id truncated to 32 bits; ids are sequential and never reused
   in practice.

Clients apply everything between two `done` events atomically.

## Compositor features the shell drives

- **Effects.** `set_surface_effects` stores blur radius and corner radii per layer surface; values are
  clamped to 0..=64 logical px and all zeros removes the entry. The renderer reads them with
  `DesiliumState::layer_surface_effects(&WlSurface)`. Entries are dropped when the surface dies.
- **Minimize target.** `set_minimize_rectangle` stores a rectangle relative to one of the shell's layer
  surfaces. `DesiliumState::minimize_target(WindowId) -> Option<Rect>` resolves it to output coordinates
  at query time, so it follows a moving or resizing dock. `None` means: use the default target.
- **Shortcuts.** Alt+Space is bound to `Action::ToggleLauncher` (`toggle-launcher` in the settings).
  `run_window_action` calls `DesiliumState::shell_shortcut(desilium_protocols::shortcut::TOGGLE_LAUNCHER)`,
  which sends the `shortcut` event to all bound shells, and then `launch::open_launcher`. That starts
  `desilium-shell --open-launcher` when no shell is running, at most once per second. The shell is
  looked up next to the compositor executable first, then on `PATH`; the first missing shell is logged
  once. `shell_shortcut` does not report whether a shell received the event, so the fallback is decided by
  the shell this compositor started (autostart or a previous fallback). A shell started by another
  supervisor is not seen, and then the fallback may start a second instance. Returning a `bool` from
  `shell_shortcut` would close that gap.
- **Startup.** Without `--command` the compositor starts `desilium-shell` (from `--shell`, or found next to
  its executable or on `PATH`) and no terminal; a terminal is only started when no shell exists. With
  `--command` only those clients start, unless `--shell` is also given. A shell started by the compositor
  is watched: its exit is logged as a warning and it is not restarted, because the session supervisor owns
  restarts.
- **Requests** reuse the compositor's own window actions (`minimize_window`, `toggle_maximize_window`,
  `request_close_animated`, `switch_desktop`, `move_window_to_desktop`), so they behave exactly like
  the same action from the keyboard or title bar.

## Versioning rules

- Version 1 is the first version. New requests, events, enum entries and error codes are added by
  raising the interface version; existing ones never change meaning or signature.
- Clients ignore unknown events, state values and shortcut names. The compositor ignores unknown
  requests (they cannot occur for a lower bound version) and only sends events a client's bound
  version knows.
- Breaking changes need a new global name (`desilium_shell_manager_v2`), which the compositor can
  serve next to v1.
