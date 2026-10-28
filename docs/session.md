# Desilium session

How a Desilium session starts, how `desilium-session` supervises it, and how to test it
without touching a real desktop.

> **Status: implemented and unit-tested, never run against a live KWin.**
> The compositor is KWin (`kwin_wayland`), which brings its own DRM/KMS backend and a nested
> Wayland backend. The supervisor, the plan and the readiness gate are tested with stand-in
> scripts; a full session with the real `kwin_wayland` has not been started yet. The KDE-side
> facts are listed in [kwin-integration.md](kwin-integration.md#11-verification-status), and the
> open points in [What is not done yet](#what-is-not-done-yet).

## 1. Start-up chain

```text
login manager (greetd, SDDM, GDM, ...)
  └─ reads /usr/share/wayland-sessions/desilium.desktop   (Exec=desilium-session)
       └─ desilium-session                     supervisor, own process, owns restart policy
            ├─ re-exec under dbus-run-session  only when no session bus exists
            ├─ kwin_wayland                    critical, Wayland provider, started first
            │     └─ creates wayland-N in $XDG_RUNTIME_DIR
            ├─ desilium-shell                  restartable, Wayland client, after the socket
            ├─ desilium-kwin                   restartable, the KWin bridge, after the socket
            └─ desilium-notifications          restartable, after the socket
```

The login manager only has to know the `.desktop` file. Nothing is installed as a systemd user
unit; logind still provides the seat and session KWin needs.

## 2. Plan: what starts, in which order

`desilium-session` first builds a **plan** from its arguments and the environment. Building the
plan spawns nothing, so `--dry-run` shows exactly what a real start would do.

Components, in start order (`crates/desilium-session/src/startup/defaults.rs`):

| Component | Binary | Criticality | Wayland role | If not installed |
|---|---|---|---|---|
| compositor | `kwin_wayland` | Critical | provider | error, session does not start |
| shell | `desilium-shell` | Restartable | client | skipped with an info log |
| kwin-bridge | `desilium-kwin` | Restartable | client | skipped with an info log |
| notifications | `desilium-notifications` | Restartable | client | skipped with an info log |

Discovery per binary: next to the `desilium-session` executable first (installed prefix or
`target/`), then `PATH`. KWin is normally found on `PATH`. An explicit flag (`--compositor`,
`--shell`, `--kwin-bridge`, `--notifications`) always wins and must exist.

KWin is started with the arguments `--xwayland --no-lockscreen`: a rootless Xwayland for X11
applications, and no lock screen, because that needs `kscreenlocker`, which Desilium does not ship.
Arguments after `--` (or the first argument that is not a session flag) are appended to those and
go to KWin only; the other components get none. The component is still called `compositor` because
it is the Wayland provider; the binary is KWin. Desilium ships no compositor.

## 3. Wayland readiness protocol

Clients must not start before the compositor can accept them. KWin picks its own socket name (a
free `wayland-N` in `$XDG_RUNTIME_DIR`), so the supervisor cannot dictate it.

1. Before KWin starts, the supervisor lists the `wayland-N` entries in `$XDG_RUNTIME_DIR`
   (`.lock` files are ignored).
2. It starts KWin and polls the directory every **25 ms** for an entry that was not in
   the first listing.
3. The new name is the display. The supervisor stores it and passes `WAYLAND_DISPLAY=<name>` to
   every child it starts from then on, including restarts.
4. Two things end the wait early: KWin exiting (`ProviderExitedEarly`) and the **10 s**
   timeout (`WaylandSocketTimeout`; KWin is then killed and the session fails).

Polling was chosen over inotify on purpose: the wait happens once per session, is bounded, and needs
no extra dependency. A socket that appears from another display's leftovers is never mistaken for
ours, because it was already in the first listing; this matters when KWin runs nested, because the
host's own socket is in the same directory. The protocol is tested with shell scripts that create
the socket file (`crates/desilium-session/tests/wayland.rs`), so no compositor is needed. Whether a
real `kwin_wayland` creates its socket within the 10 s limit has not been measured.

## 4. Environment

Set for every child (`crates/desilium-session/src/startup/environment.rs`):

| Variable | Value | Rule |
|---|---|---|
| `XDG_SESSION_TYPE` | `wayland` | always set |
| `XDG_CURRENT_DESKTOP` | `Desilium:KDE` | always set |
| `XDG_SESSION_DESKTOP` | `desilium` | always set |
| `DESKTOP_SESSION` | `desilium` | always set |
| `KDE_SESSION_VERSION` | `6` | always set |
| `KWIN_WAYLAND_NO_PERMISSION_CHECKS` | `1` | only with `--allow-any-client` |
| `WAYLAND_DISPLAY` | the compositor's socket | set after the compositor is ready |
| `QT_QPA_PLATFORM` | `wayland;xcb` | only when the user has not set it |
| `MOZ_ENABLE_WAYLAND` | `1` | only when the user has not set it |
| `GDK_BACKEND` | `wayland,x11` | only when the user has not set it |
| `ELECTRON_OZONE_PLATFORM_HINT` | `auto` | only when the user has not set it |
| `SDL_VIDEODRIVER` | `wayland,x11` | only when the user has not set it |

`XDG_CURRENT_DESKTOP` is a list: portals and applications try `Desilium` first and fall back to
what they know for `KDE`, which is what KWin and its portal backend belong to.
`KWIN_WAYLAND_NO_PERMISSION_CHECKS` makes KWin hand its restricted protocols (window list, virtual
desktops) to every client. It is a development switch; the normal route is the shell's desktop
file with `X-KDE-Wayland-Interfaces` (see
[kwin-integration.md](kwin-integration.md#3-permission-mechanism)).

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
| KWin (Critical) exits, any status | session ends; exit code follows KWin's status |
| Restartable component exits with status 0 | restarted after 500 ms; crash history untouched |
| Restartable component fails (non-zero, signal, spawn error) | restarted after backoff: 500 ms, doubling, capped at 30 s |
| Run lasted at least 60 s | counts as healthy: backoff and crash history reset |
| 5 failures within 60 s | crash loop: the component is given up, the session continues |
| SIGTERM or SIGINT to the supervisor | SIGTERM to every component group, 5 s grace, then SIGKILL |
| KWin cannot be started or its socket never appears | session fails with an error, children are stopped |

Each component runs in its own process group, so signals reach whole process trees and the
supervisor can kill stragglers. A KWin restart is not supported: it would drop every client
connection, so a KWin exit always ends the session. A bridge restart is harmless: it applies the
configuration again.

## 7. Command line

```text
desilium-session [--dry-run[=text|json]] [--print-env] [--allow-any-client]
                 [--compositor <kwin_wayland path>] [--shell <path>]
                 [--kwin-bridge <path>] [--notifications <path>]
                 [--] [kwin_wayland args...]
```

- `--dry-run` prints the plan (components, paths and how they were found, start order, skipped
  components, D-Bus action, environment changes) and exits 0 without starting anything. A missing
  `kwin_wayland` is an error (exit 1) even in a dry run.
- `--allow-any-client` exports `KWIN_WAYLAND_NO_PERMISSION_CHECKS=1`. Development only.
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

Running a session safely, nested:

- Start KWin nested in your current Wayland session: `target/debug/desilium-session
  --allow-any-client`. KWin should use its nested Wayland backend when it finds `WAYLAND_DISPLAY`
  (not verified). Extra arguments for KWin go after `--`, for example `-- --width 1600
  --height 900`. The bridge writes KDE configuration files into `$XDG_CONFIG_HOME`, so use a
  throw-away directory for it on the host.
- `scripts/docker-run.sh` does the same in a hardened container: it runs `desilium-session` (default
  argument `--allow-any-client`; your arguments replace it, and arguments after `--` go to
  `kwin_wayland`). Only the host Wayland socket and the render node are passed in; the container
  has no network and no capabilities except `SYS_NICE`, which `kwin_wayland` needs to be
  executed at all. The container user is `dev` with the home `/home/dev`, a named Docker volume
  (`desilium-dev-home-<distro>`) seeded from `docker/home-skel` with the XDG user folders,
  `Applications`, wallpapers and executable launchers on the desktop; the XDG directories are the
  defaults below that home, so the configuration the bridge writes persists between runs
  (`--fresh-home`, `--reset-home`, `--save`; images for Debian, `--arch`, `--ubu`
  and `--fed`; see [testing.md](testing.md#the-development-user-space)). The images were not
  built or run when this was written.
- `kwin_wayland --virtual` runs KWin without a display, for tests (see [headless.md](headless.md)).

To exercise the supervisor end to end without KWin, point it at a stand-in: a script that creates
`$XDG_RUNTIME_DIR/wayland-1` and then sleeps, passed with `--compositor`. Never run
`desilium-session` on a host desktop without a sandbox or a nested run; a real start creates a
compositor and children in your session.

## 10. Packaging

`packaging/` holds the desktop files, portal preferences, `install.sh` (prefix, `DESTDIR`,
`--dry-run`, `--uninstall`) and an Arch `PKGBUILD`. See `packaging/README` for the layout and for
why systemd user units are not used.

## What is not done yet

- **A run with a real KWin.** No session has been started with `kwin_wayland` yet. The DRM/KMS
  backend is no longer a Desilium gap (KWin has one), but whether a login-manager start works end
  to end is unproven.
- A session-level log file with rotation (section 8).
- Restarting the Wayland provider. Clients keep the socket name of the first start.
- Health checks beyond exit status (no ping or readiness from the shell, the bridge and the
  notification daemon).
- A lock screen. KWin is started with `--no-lockscreen`.
- A Desilium portal backend; `packaging/xdg-desktop-portal/desilium-portals.conf` sends ScreenCast,
  RemoteDesktop, Screenshot, GlobalShortcuts and InputCapture to the KDE backend and everything
  else to GTK.
- Config-service supervision (there is no separate configuration process).
- Safe mode after repeated crash loops of the shell (the architecture lists it as a goal).
