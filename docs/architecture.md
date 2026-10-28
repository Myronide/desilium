# Desilium — Architecture

> Draft v1. Scope: workspace layout, process boundaries and the responsibilities of each process. Derived from `concept.md`; where they disagree, `concept.md` wins until this document is updated.
>
> Desilium used to contain its own Smithay-based compositor (`desilium-compositor`, `desilium-wm`, `desilium-cursor`, `desilium-anim`, `desilium-text`, `desilium-protocols`, and the private protocol `desilium-shell-v1`). Those crates were removed; KWin is the compositor and window manager now. The reasons and the trade-offs are in `concept.md` section 1.1, the interface is described in [kwin-integration.md](kwin-integration.md).

## 1. Goals for this document

- Fix crate and process boundaries.
- Say which process owns which responsibility, so KWin, the bridge and the shell do not drift into each other's work.
- List open questions explicitly instead of deciding them implicitly in code.

Non-goals: the internals of KWin, the settings app, the theme engine, portals. They appear only as interfaces.

## 1.1 Decisions (resolved)

| Topic | Decision |
|---|---|
| Code style | All Rust code follows **Pureline 1.5** (`PURELINE_SPECIFICATION_1.5.md`, profile `PL-RS-*`). See §8. |
| Compositor and window manager | **KWin** (`kwin_wayland`, KDE Plasma 6). Desilium ships no compositor. |
| Window decorations | KWin's. The Desilium theme is exported as a KDE colour scheme; there is no Desilium-drawn decoration. |
| Desilium to KWin | **One bridge crate (`desilium-kwin`)** projects the typed configuration onto KWin's configuration and D-Bus interfaces. Nothing else writes KWin settings. |
| Shell to KWin | The shell is a client of KDE's plasma window-management, virtual-desktop and blur protocols. Access to the first two is granted by KWin through `X-KDE-Wayland-Interfaces` in a desktop file. |
| Wallpaper | Drawn by the shell (one Background-layer surface per output); KWin has none. |
| Session supervision | **Own `desilium-session` binary.** No dependency on systemd user units for supervision (logind is still used for seat/session access). |

## 2. Process model

```text
                    systemd-logind / libseat
                              |
                      desilium-session   (lifecycle, restarts)
                              |
        +---------------+-----+-----------+---------------------+
        |               |                 |                     |
   kwin_wayland   desilium-shell     desilium-kwin     desilium-notifications
  (Wayland server) (Wayland client)  (KWin bridge)      (D-Bus service)
        ^               |                 |
        | plasma window / desktop / blur  |  KConfig files (kwriteconfig6),
        +---- protocols -+                |  org.kde.KWin and KGlobalAccel over D-Bus
        ^                                 |
        +---------------------------------+
```

Responsibilities:

| Process | Owns | Does not own |
|---|---|---|
| `desilium-session` | Start order, the Wayland readiness gate, the session environment, restart policy, shutdown | Anything visible |
| `kwin_wayland` | Outputs, input, rendering, window placement and decorations, window animations, virtual desktops, XWayland, screencast, its own shortcut actions | Panels, launcher, wallpaper, settings, notifications |
| `desilium-shell` | Top bar, dock/taskbar, launcher, quick settings, wallpaper, notification toasts; window list and actions through KWin's protocols; session restore | Compositing, decorations |
| `desilium-kwin` | The mapping from `config.toml` to KWin/KDE settings, shortcut registration, the `launch-terminal` and `quit` shortcuts | Drawing, window management |
| `desilium-notifications` | `org.freedesktop.Notifications` and `org.desilium.Notifications1` | Rendering (the shell renders) |
| `desilium-settings`, `doria` | Ordinary Wayland clients started by the user | Part of the supervised session |
| `desilium-config` (library) | The typed store: loading, validation, transactions, backups, change detection | A process of its own today |

Decisions:

1. **KWin is its own process and never links Desilium code.** The shell is an ordinary Wayland client. This is what makes "a shell crash must not terminate the session" structurally true.
2. **`desilium-session` is our own supervisor.** It starts KWin first, waits for its Wayland socket, then starts the shell, the bridge and the notification daemon. It restarts non-critical components with backoff and ends the session cleanly if KWin dies (a compositor cannot be restarted without losing all clients' connections). It talks to logind for the seat/session but owns restart policy itself, which keeps Desilium independent of one init system. Details in [session.md](session.md).
3. **The bridge is a separate, restartable process.** If it dies, KWin keeps its last settings; a restarted bridge writes the configuration again. KWin itself stays usable without it.
4. **The configuration stays Desilium's.** `config.toml` is the source of truth. KWin's own files (`kwinrc`, `kdeglobals`, ...) are a projection target that the bridge overwrites for the keys it owns; they are not a second place to configure.
5. **KDE-specific knowledge is isolated** in `desilium-kwin` and `desilium-shell/src/services/protocol/`. No other crate may name a KDE key or protocol.

## 3. Workspace layout

```text
desilium/
├── Cargo.toml                 # [workspace]
├── concept.md
├── docs/
└── crates/
    ├── desilium-config-types/ # lib: schema structs only (serde), no I/O
    ├── desilium-config/       # lib: store, validation, transactions, recovery
    ├── desilium-theme/        # lib: tokens, built-in themes, derivation from an accent
    ├── desilium-kwin/         # lib + bin: the KWin bridge
    ├── desilium-session/      # lib + bin: supervisor
    ├── desilium-ui/           # lib: Blitz host for Wayland surfaces
    ├── desilium-shell/        # bin: bar, dock, launcher, quick settings, wallpaper
    ├── desilium-settings/     # bin: graphical settings
    ├── desilium-files/        # bin: Doria, the file manager
    ├── desilium-notifications/# bin: notification daemon
    ├── desilium-apps/         # lib: desktop-file catalog and launching
    └── desilium-wallpaper/    # lib: wallpaper generation and scaling
```

Rules:

- `desilium-config-types` is the only crate every component may depend on for settings. Components depend on **types**, not on the store implementation.
- No crate depends on `desilium-shell`. Graphical Wayland clients (`desilium-shell`, `desilium-settings`, `desilium-files` / Doria) may depend on `desilium-ui`.
- `desilium-kwin` depends on configuration, theme and app-launching crates, never on UI crates. `desilium-shell` depends on `desilium-kwin` only for the shortcut listener and for window placement.
- Nothing may hardcode Myronide paths or services (`concept.md` §10).
- New crates are added when they have content.

## 4. KWin bridge and shell protocol client

The details are in [kwin-integration.md](kwin-integration.md). In short:

- **Plan, then apply.** `plan/` turns a configuration into a list of KConfig entries and a desktop count; it is pure and unit-tested. `apply/` writes only the difference with `kwriteconfig6` and calls KWin over D-Bus. `shortcuts/` translates shortcuts into KGlobalAccel registrations. `bridge.rs` is the process loop: apply at start, inotify on `config.toml`, retry with backoff.
- **Failure is local.** A missing KWin service, a refused protocol or a failing `kwriteconfig6` degrades one feature and is logged. It never stops the desktop from starting (`concept.md` §3, requirement 7).
- **The shell's protocol client** binds `org_kde_plasma_window_management`, `org_kde_plasma_virtual_desktop_management` and `org_kde_kwin_blur_manager` on its own event queue of the connection the UI host owns. It turns them into the shell's own window and workspace model (`model/windows`), so the rest of the shell does not see Wayland objects.

## 5. Configuration interaction

- Settings are defined as serde types in `desilium-config-types`.
- The shell, the settings app and the bridge open the store from the same directory. **A broken or missing config must never block startup** (`concept.md` §3, req. 7).
- Live updates: the bridge and the shell watch the directory (inotify) and re-read a validated snapshot. The bridge writes the difference into KWin's configuration and asks KWin to reconfigure.
- Input-device and output configuration is applied through the same path where KWin exposes it as configuration (keyboard repeat and layout, cursor, focus and snapping). Output layout and scaling are KWin's and are not yet part of the Desilium settings.

## 6. Error handling and observability

- `tracing` everywhere. No secrets or window titles at `info` level.
- Code that handles data from outside (D-Bus replies, protocol events, configuration files) must not `unwrap()`/`panic!`. A misbehaving peer produces a logged error.
- External programs (`kwriteconfig6`, nmcli, wpctl, ...) run with fixed argument vectors, never through a shell, and with a timeout.
- KWin or KDE services that are not up yet are an expected state during session start, not an error.

## 7. Testing strategy

| Layer | Approach |
|---|---|
| Configuration mapping (`desilium-kwin/plan`, `shortcuts`) | Plain unit tests; the mapping is pure |
| KWin application (`apply`) | Stand-in scripts instead of `kwriteconfig6`; no KWin needed |
| Shell protocol client | Unit tests of the state machines and of the blur region; no live KWin |
| Config | Schema round-trip, migration, invalid-input tests |
| Session | Process tests with shell-script stand-ins (restarts, crash loops, readiness) |
| UI | Headless Blitz renders of the shell documents (`desilium-shell` snapshot tests) |
| Against a live KWin | **Not automated.** `docs/kwin-integration.md` lists what has to be checked by hand. `kwin_wayland --virtual` is the intended way to run it without a display; see [headless.md](headless.md) |
| Performance | Reproducible benchmarks after the first prototype (`concept.md` §7) |

## 8. Pureline conventions for this codebase

Pureline 1.5 is the binding style for all Rust code. How its rules map onto the architecture:

| Rule | Consequence here |
|---|---|
| PL-RS-010 / PL-STRUCT-002 (max 10 production `.rs` files per folder) | Crates are split into folders from the start (`plan/`, `apply/`, `shortcuts/` in the bridge). |
| PL-RS-009 / PL-STRUCT-003 (cohesion over size) | A long `impl` or per-algorithm module may be long if cohesive; split only on real responsibility boundaries. |
| PL-RS-008 (traits need a real abstraction) | Traits only once a second implementation exists. No manager/factory traits. No `I` prefix. |
| PL-RS-004 (no `unwrap`/`expect`/`panic!`) | Matches §6. Allowed only in tests and documented invariants. |
| PL-RS-002/003 (guard clauses, `let … else`, `?`) | Validate early and return typed errors (`thiserror`, one enum per crate). |
| PL-RS-013 (async tasks have ownership/shutdown) | Background threads and tasks are owned by a handle with explicit `shutdown()`; no fire-and-forget for important work. Never hold a lock across `.await`. |
| PL-RS-014 (RAII + explicit `close()`) | Supervisor children and listeners expose `shutdown()`; `Drop` only does infallible cleanup. |
| PL-RS-015/016 (unsafe, FFI isolated) | Any `unsafe` carries `// SAFETY:` and a minimal safe wrapper. |
| PL-RS-017 (types encode invariants) | Newtypes for identifiers (`WindowId`) and for logical vs. physical coordinates. |
| PL-RS-011/020 (rustfmt, clippy) | CI runs `cargo fmt --check` and `cargo clippy`; no blanket `allow` attributes. |
| PL-RS-019 | Benchmarks come before any low-level optimisation. |
| PL-CORE-020 / PL-ARCH-001 | Dependencies are passed explicitly (no global state / service locators); the KWin plan takes everything through parameters. |

Open check: whether the Pureline Checker already supports the Rust profile as a CLI we can run in CI, or if CI uses only `fmt` + `clippy` for now.

## 9. Open questions

1. **Version coupling.** Which KWin/Plasma versions are supported, and how is that enforced (package dependency ranges, a runtime check, both)? The plasma window-management protocol is bound for versions 13 to 18 today.
2. **Lock screen.** KWin runs with `--no-lockscreen`. Decide between KDE's screen locker (an extra KDE dependency) and a Desilium lock screen built on the session-lock protocol.
3. **Portal backend.** Screencast, remote desktop and screenshots go to the KDE portal backend. Whether Desilium needs its own backend for the rest (file chooser, settings) is open.
4. **Decorations.** KWin's decoration is used. A Desilium decoration theme or a KDecoration3 plugin would bring the title bar in line with the design tokens, at the price of a C++/Qt component.
5. **Output configuration.** Scale, mode and layout are KWin's and are not in the Desilium settings. Decide how the display settings page reaches them (kscreen over D-Bus, `kscreen-doctor`, or the output-management protocol).
6. **Supervisor protocol.** How `desilium-session` talks to its children (readiness notification, health pings) and what it does on repeated crash loops (safe-mode shell).
7. **Verification on a live KWin.** Everything in the integration that depends on KDE-side names is unverified; see "Verification status" in [kwin-integration.md](kwin-integration.md).

## Desktop integration: WC, Doria and panels

The WC spike is preserved as an excluded nested workspace. Production controls
are ported onto the existing typed configuration, not a second compositor IPC
API. See [wc-integration.md](wc-integration.md).

Doria (`desilium-files`, binary `doria`) is an ordinary XDG Wayland client, not
a compositor dependency or a mandatory session daemon. Its pure model, GUI,
places storage and GIO filesystem worker have separate modules. Mounts and
remote filesystems use the host's established services. Passwords stay outside
configuration. See [doria.md](doria.md).

Panel preferences live in `DesiliumConfig.shell`; graphical registry controls
and quick-panel edits use the existing transactions. Shell rendering keeps
hit geometry/minimize targets synchronized with configurable dock/taskbar
geometry (the minimize target is passed to KWin). System controls use a dedicated
worker with fixed argv and bounded external commands. They never block the shell
event loop. See [desktop-controls.md](desktop-controls.md) and
[desktop-validation.md](desktop-validation.md).
