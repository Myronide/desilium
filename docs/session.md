# Desilium session

How a Desilium session starts, how `desilium-session` supervises it, and how to test it
without touching a real desktop.

> **Status: the session cannot yet run a real desktop.**
> The compositor has one real backend today, **winit**, which is a window inside an existing
> Wayland or X11 session, plus a **headless** mode for screenshots. There is no **DRM/KMS**
> backend yet, so a login manager cannot start a usable desktop on bare hardware. Everything in
> this document is implemented and tested as far as it goes; the DRM backend is the missing piece
> (see [What is not done yet](#what-is-not-done-yet)).

## 1. Start-up chain

```text
login manager (greetd, SDDM, GDM, ...)
  └─ reads /usr/share/wayland-sessions/desilium.desktop   (Exec=desilium-session)
       └─ desilium-session                     supervisor, own process, owns restart policy
            ├─ re-exec under dbus-run-session  only when no session bus exists
            ├─ desilium-compositor             critical, Wayland provider, started first
            │     └─ creates wayland-N in $XDG_RUNTIME_DIR, then logs "listening for Wayland clients"
            ├─ desilium-shell                  restartable, Wayland client, after the socket
            └─ desilium-notifications          restartable, Wayland client, after the socket
```

The login manager only has to know the `.desktop` file. Nothing is installed as a systemd user
unit; logind still provides the seat and session the compositor needs.

## 2. Plan: what starts, in which order

`desilium-session` first builds a **plan** from its arguments and the environment. Building the
plan spawns nothing, so `--dry-run` shows exactly what a real start would do.

Components, in start order (`crates/desilium-session/src/startup/defaults.rs`):

| Component | Binary | Criticality | Wayland role | If not installed |
|---|---|---|---|---|
| compositor | `desilium-compositor` | Critical | provider | error, session does not start |
| shell | `desilium-shell` | Restartable | client | skipped with an info log |
| notifications | `desilium-notifications` | Restartable | client | skipped with an info log |

Discovery per binary: next to the `desilium-session` executable first (installed prefix or
`target/`), then `PATH`. An explicit flag (`--compositor`, `--shell`, `--notifications`) always wins
and must exist; compositor arguments after `--` (or the first unknown argument) go to the compositor
only.

## 3. Wayland readiness protocol

Clients must not start before the compositor can accept them. The compositor picks its own socket
name (the first free `wayland-N` in `$XDG_RUNTIME_DIR`), so the supervisor cannot dictate it.

1. Before the compositor starts, the supervisor lists the `wayland-N` entries in `$XDG_RUNTIME_DIR`
   (`.lock` files are ignored).
2. It starts the compositor and polls the directory every **25 ms** for an entry that was not in
   the first listing.
3. The new name is the display. The supervisor stores it and passes `WAYLAND_DISPLAY=<name>` to
   every child it starts from then on, including restarts.
4. Two things end the wait early: the compositor exiting (`ProviderExitedEarly`) and the **10 s**
   timeout (`WaylandSocketTimeout`; the compositor is then killed and the session fails).

Polling was chosen over inotify on purpose: the wait happens once per session, is bounded, and needs
no extra dependency. A socket that appears from another display's leftovers is never mistaken for
ours, because it was already in the first listing. The protocol is tested with shell scripts that
create the socket file (`crates/desilium-session/tests/wayland.rs`), so no compositor is needed.

## 4. Environment

Set for every child (`crates/desilium-session/src/startup/environment.rs`):

| Variable | Value | Rule |
|---|---|---|
| `XDG_SESSION_TYPE` | `wayland` | always set |
| `XDG_CURRENT_DESKTOP` | `Desilium` | always set |
| `XDG_SESSION_DESKTOP` | `desilium` | always set |
| `DESKTOP_SESSION` | `desilium` | always set |
| `WAYLAND_DISPLAY` | the compositor's socket | set after the compositor is ready |
| `QT_QPA_PLATFORM` | `wayland;xcb` | only when the user has not set it |
| `MOZ_ENABLE_WAYLAND` | `1` | only when the user has not set it |
| `GDK_BACKEND` | `wayland,x11` | only when the user has not set it |
| `ELECTRON_OZONE_PLATFORM_HINT` | `auto` | only when the user has not set it |
| `SDL_VIDEODRIVER` | `wayland,x11` | only when the user has not set it |

`--dry-run` marks each variable as added (`+`), replaced (`~ ... (was ...)`), unchanged (`=`) or
kept because the user set it (`= ... (kept: set by the user)`).

## 5. D-Bus session bus

Portals, notifications and settings need a session bus. The decision is a pure function,
`ensure_dbus_session_bus` (`startup/dbus.rs`):

- `DBUS_SESSION_BUS_ADDRESS` is set: use it.
- Not set and `dbus-run-session` is on `PATH`: re-execute the session once under
  `dbus-run-session -- desilium-session ...`. The re-executed process carries
  `DESILIUM_DBUS_REEXEC=1`, so the re-exec cannot loop.
- Otherwise: continue without a bus and log a warning.

Login managers that already start a user bus (systemd user manager, `dbus-broker-launch`) make the
re-exec unnecessary; the check is a no-op in that case.

## 6. Supervision and restart policy

Restart rules are pure and live in `policy.rs`; `supervisor.rs` applies them to real processes.

| Situation | Behaviour |
|---|---|
| Compositor (Critical) exits, any status | session ends; exit code follows the compositor's status |
| Restartable component exits with status 0 | restarted after 500 ms; crash history untouched |
| Restartable component fails (non-zero, signal, spawn error) | restarted after backoff: 500 ms, doubling, capped at 30 s |
| Run lasted at least 60 s | counts as healthy: backoff and crash history reset |
| 5 failures within 60 s | crash loop: the component is given up, the session continues |
| SIGTERM or SIGINT to the supervisor | SIGTERM to every component group, 5 s grace, then SIGKILL |
| Compositor cannot be started or its socket never appears | session fails with an error, children are stopped |

Each component runs in its own process group, so signals reach whole process trees and the
supervisor can kill stragglers. A compositor restart is not supported: it would drop every client
connection, so a compositor exit always ends the session.

## 7. Command line

```text
desilium-session [--dry-run[=text|json]] [--print-env]
                 [--compositor <path>] [--shell <path>] [--notifications <path>]
                 [--] [compositor args...]
```

- `--dry-run` prints the plan (components, paths and how they were found, start order, skipped
  components, D-Bus action, environment changes) and exits 0 without starting anything. A missing
  compositor is an error (exit 1) even in a dry run.
- `--dry-run=json` prints the same plan as one JSON document for tooling.
- `--print-env` prints `export` lines for the session environment, for shells and debugging.

## 8. Logging

The session logs with `tracing` to **stderr**. Its messages cover component start, exit with status
and uptime, backoff decisions, the Wayland socket, skipped components and shutdown. The level is set
with `RUST_LOG` (default `info`).

When a login manager starts the session, stderr is captured by the session's systemd scope or the
display manager's log, so the messages land in the journal (`journalctl --user`, or the display
manager's unit). A separate `$XDG_STATE_HOME/desilium/session.log` with size-capped rotation is
**not implemented**; the journal is the supported place for now.

## 9. Testing

Automated (no display, no real session):

```sh
cargo test -p desilium-session          # unit tests plus process tests with sh stand-ins
cargo clippy -p desilium-session --all-targets -- -D warnings
```

- Plan resolution, discovery, environment, D-Bus decision, CLI parsing and renderers are unit tests
  with injected environments and fake executables in temporary directories. No test changes the
  process environment.
- `tests/supervisor.rs` covers restarts, crash loops, shutdown and process cleanup with shell
  scripts.
- `tests/wayland.rs` covers the readiness gate: socket hand-over, timeout, early provider exit,
  missing provider, missing runtime directory.

Manual, without a host desktop:

```sh
target/debug/desilium-session --dry-run                 # plan for the installed or built binaries
target/debug/desilium-session --dry-run=json
target/debug/desilium-session --print-env
```

Running the compositor itself safely, nested:

- `scripts/docker-run.sh --command foot` runs the compositor (winit) in a hardened container as a
  client of your host Wayland session. Only the host Wayland socket and the render node are passed
  in; the container has no network and no capabilities. The session supervisor is not involved.
- `scripts/docker-screenshot.sh --command "..."` runs the compositor headless and writes a PNG,
  which is the easiest way to check visual changes.

To exercise the supervisor end to end without a compositor, point it at a stand-in: a script that
creates `$XDG_RUNTIME_DIR/wayland-1` and then sleeps, passed with `--compositor`. Never run
`desilium-session` on a host desktop without a sandbox; a real start creates a compositor and
children in your session.

## 10. Packaging

`packaging/` holds the desktop files, portal preferences, `install.sh` (prefix, `DESTDIR`,
`--dry-run`, `--uninstall`) and an Arch `PKGBUILD`. See `packaging/README` for the layout and for
why systemd user units are not used.

## What is not done yet

- **DRM/KMS backend.** The compositor runs only as winit (a window in an existing session) or
  headless. A login-manager start on bare hardware therefore fails at compositor start today:
  the session stops with an error and logs why. This is the largest gap.
- A session-level log file with rotation (section 8).
- Restarting the Wayland provider. Clients keep the socket name of the first start.
- Health checks beyond exit status (no ping or readiness from the shell and notification daemon).
- A Desilium portal backend; `packaging/xdg-desktop-portal/desilium-portals.conf` uses GTK for now.
- Config-service supervision (the architecture lists it; it does not exist yet).
- Safe mode after repeated crash loops of the shell (the architecture lists it as a goal).
