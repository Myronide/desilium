# Desilium — Compositor Architecture

> Draft v0. Scope: workspace layout, process boundaries, and the compositor core. Derived from `concept.md`; where they disagree, `concept.md` wins until this document is updated.

## 1. Goals for this document

- Fix crate and process boundaries before code exists.
- Define the compositor's internal structure so Phase 1 (minimal compositor, nested dev environment) does not need to be rewritten for Phase 4/5.
- List open questions explicitly instead of deciding them implicitly in code.

Non-goals: shell UI, settings app, theme engine, portals. They appear only as interfaces the compositor must expose.

## 1.1 Decisions (resolved)

| Topic | Decision |
|---|---|
| Code style | All Rust code follows **Pureline 1.5** (`PURELINE_SPECIFICATION_1.5.md`, profile `PL-RS-*`). See §11. |
| Renderer effects | Phase 1 uses Smithay's GLES renderer. Blur/shadows/effects are expected to move to **wgpu** later; `render/` stays behind a trait so the switch is local. |
| Window decorations | **Server-side** by default via `xdg_decoration`, drawn by the compositor and themed by `desilium-theme`. |
| Session supervision | **Own `desilium-session` binary.** No dependency on systemd user units for supervision (logind is still used for seat/session access). |

## 2. Process model

```text
                    systemd-logind / libseat
                              |
                      desilium-session   (lifecycle, restarts)
                              |
        +---------------------+----------------------+
        |                     |                      |
desilium-compositor    desilium-shell         desilium-config
 (Wayland server)     (Wayland client)        (D-Bus service)
        ^                     |                      |
        |   layer-shell /     |                      |
        +---- private proto --+                      |
        |                                            |
        +------------- change notifications ---------+
```

Decisions:

1. **The compositor is its own process and never links the shell.** The shell is an ordinary Wayland client (layer-shell for panels/launcher, plus a private Desilium protocol for what layer-shell cannot express). This is what makes "a shell crash must not terminate the session" structurally true rather than aspirational.
2. **`desilium-session` is our own supervisor** (no systemd user units). It starts and watches compositor, shell, config service and notifications, restarts non-critical components with backoff, and ends the session cleanly if the compositor dies (a compositor cannot be restarted without losing all clients' connections). It talks to logind for the seat/session but owns restart policy itself, which keeps Desilium distribution-independent.
3. **`desilium-config` is a separate D-Bus service** (zbus). The compositor holds a read-only cached copy of its own settings and applies change notifications live. If the config service is down, the compositor keeps running on its last known good settings, or built-in defaults on first start.

## 3. Workspace layout

```text
desilium/
├── Cargo.toml                 # [workspace]
├── concept.md
├── docs/architecture.md
└── crates/
    ├── desilium-compositor/   # bin: Wayland compositor (Smithay glue)
    ├── desilium-wm/           # lib: pure window management, NO smithay/wayland dependency
    ├── desilium-config/       # lib + bin: typed settings, validation, D-Bus service (later)
    ├── desilium-config-types/ # lib: schema structs only (serde), no I/O
    ├── desilium-protocols/    # lib: private Wayland protocol XML + generated bindings
    ├── desilium-session/      # bin (later)
    ├── desilium-shell/        # bin (Phase 2)
    └── desilium-ui/           # lib (Phase 2)
```

Rules:

- `desilium-config-types` is the only crate every component may depend on for settings. Compositor depends on **types**, not on the config service implementation.
- No crate depends on `desilium-shell`. Graphical Wayland clients (`desilium-shell`,
  `desilium-settings`, and `desilium-files` / Doria) may depend on `desilium-ui`;
  the compositor and pure window manager do not.
- Nothing may hardcode Myronide paths or services (`concept.md` §10).
- Phase 1 creates `desilium-compositor`, `desilium-wm` and `desilium-config-types`; others are added when they have content.
- `desilium-wm` is a separate crate (instead of a `wm/` folder) so the compiler enforces that window-management logic cannot depend on Smithay.

## 4. Compositor internals

### 4.1 Layering

```text
┌──────────────────────────────────────────────┐
│ backend/   winit | drm (later)               │  outputs, input, renderer, vblank
├──────────────────────────────────────────────┤
│ state/     DesiliumState (single owner)      │  smithay handler impls, client data
├──────────────────────────────────────────────┤
│ wm/        pure window-management logic      │  NO smithay types, fully unit-tested
├──────────────────────────────────────────────┤
│ render/    scene -> render elements          │  effects, animations plug in here
└──────────────────────────────────────────────┘
```

- **`desilium-wm` is deliberately smithay-free** (own crate, see §3). It works on opaque `WindowId`s, rectangles and events (`WindowOpened`, `FocusRequest`, `MoveBegin`, `OutputAdded`, …) and returns *layout decisions* (`Vec<Placement>`). This keeps tiling/floating/snapping/virtual desktops testable without a Wayland connection and matches the "rendering and business logic stay separated" rule.
- **`backend/` is a trait boundary.** Phase 1 ships only winit; the DRM/KMS backend implements the same trait later. Nothing above `backend/` may assume a nested session.
- **`state/`** is the single mutable state object smithay's calloop-based design expects. It translates between smithay events and `wm/` events. No logic beyond translation lives here.
- **`render/`** builds a list of render elements from `wm/` placements. Animations are time-based interpolations owned by `render/`, never by `wm/`, so layout stays deterministic. Phase 1 renders through Smithay's GLES renderer; effects (blur, shadows) are designed to move to wgpu later, so effect code talks to a small `render/` interface instead of GLES types directly.
- **Server-side decorations:** title bars, borders and shadows are drawn by the compositor as render elements, sourced from the theme (`desilium-theme`) via config types.

### 4.2 Event loop

- Single-threaded **calloop** loop, as Smithay requires. Wayland socket, input, timers and backend events are calloop sources.
- Tokio is **not** the compositor's main loop. Where async is needed (D-Bus via zbus), run it on a separate thread and hand events into calloop through a channel source. Blocking the compositor loop on I/O is not allowed.
- Frame scheduling is event-driven: render only when damage exists or an animation is active (idle CPU requirement, `concept.md` §7).

### 4.3 Window management model

- `Workspace` (virtual desktop) → set of windows, each `Floating` or `Tiled`.
- `Output` ← displays one workspace at a time; workspaces migrate between outputs on hot-plug.
- Layout algorithms are pluggable behind a `Layout` trait (`fn arrange(&self, area: Rect, windows: &[WindowId]) -> Vec<Placement>`). Phase 1: floating only. Tiling/snapping follow in Phase 4.
- Focus, stacking order and per-window state (maximized, fullscreen, minimized) live in `wm/`.

### 4.4 Protocol support (staged)

| Stage | Protocols |
|---|---|
| Phase 1 (done) | `wl_compositor`, `wl_shm`, `wl_seat`, `wl_output` (+ xdg-output), `xdg_shell`, `wl_data_device` (clipboard basics) |
| Phase 1.5 | `xdg_decoration` — registered together with server-side decoration rendering; advertising it earlier would leave windows without title bars |
| Phase 2 | `wlr-layer-shell` (panels/launcher), `linux-dmabuf`, `viewporter`, `fractional-scale` |
| Phase 4 | `ext-session-lock` (lock screen), `idle-notify`, `xdg-activation` |
| Phase 5 | XWayland, `xdg-foreign`, screencopy / ext-image-capture + PipeWire for portals, `text-input`/`input-method`, `pointer-constraints` |
| Private | `desilium-shell-v1` (see §5) |

Security: privileged protocols (screencopy, input injection, lock, layer-shell exclusive zones) are only offered to clients identified as trusted (e.g. spawned by `desilium-session`, verified via pid/security-context), not to every client.

## 5. Compositor ⇄ shell interface

`desilium-shell` needs things layer-shell does not provide. A private protocol `desilium-shell-v1` (in `desilium-protocols`) is the contract:

- window list with title/app-id/state (taskbar), activate/close/minimize requests
- workspace list and switching
- output list and geometry
- overview / snap-assist hints, drag-to-snap overlays
- compositor → shell: key bindings that the shell should handle (launcher key)

It is versioned from day one. Shell absence is a normal state: the compositor must be fully usable (focus, move, close, switch workspace by shortcut) without it.

## 6. Configuration interaction

- Compositor-owned settings (keybindings, gaps, animation speed, snapping, input) are defined as serde types in `desilium-config-types`.
- On startup: request settings from `desilium-config` over D-Bus with a short timeout; fall back to the cached last-known-good file, then to defaults. **A broken or missing config must never block startup** (`concept.md` §3, req. 7).
- Live updates arrive as a full validated snapshot or a typed diff; the compositor applies it at a frame boundary.
- Input-device and output configuration (scale, mode, layout) is applied through the same path; auto-detection fills defaults, user overrides win.

## 7. Error handling and observability

- `tracing` everywhere; per-client and per-output spans. No secrets or window titles at `info` level.
- Handler code must not `unwrap()`/`panic!` on client-supplied data. A misbehaving client gets a protocol error and is disconnected, never takes the compositor down.
- Backend loss (GPU reset, output unplug) is a handled event, not an error.
- A `--nested` / `DESILIUM_LOG` development workflow is part of Phase 1, not an afterthought.

## 8. Testing strategy

| Layer | Approach |
|---|---|
| `wm/` | Plain unit and property tests (no Wayland) |
| Protocol handlers | Headless compositor + in-process test Wayland client (smithay supports a headless/dummy backend) |
| Rendering | Golden-image tests against a software renderer where feasible |
| Config | Schema round-trip, migration, invalid-input tests |
| Performance | Reproducible benchmarks after the first prototype (`concept.md` §7) |

## 9. Phase 1 deliverable (concrete)

Status: implemented.

1. Cargo workspace with `desilium-compositor`, `desilium-wm`, `desilium-config-types`. ✔
2. Winit backend: nested window, one `wl_output`, redraw only on demand (a calloop ping wakes the renderer; idle CPU ≈ 0 %). ✔
3. Smithay event loop with the Phase 1 protocols from §4.4, graceful shutdown on SIGINT/SIGTERM. ✔
4. Floating window management: cascade placement, click-to-focus + raise, move/resize via `xdg_toplevel` requests (with top/left edge anchoring), close. ✔
5. Default shortcuts from `desilium-config-types` (Alt+Enter terminal, Alt+Q close, Alt+Shift+Q quit). ✔
6. `tracing` logging, unit tests for `desilium-wm` and `desilium-config-types`. ✔

Known gaps (intentional): no client cursor images (the host draws the cursor while nested), no popup grabs, no server-side decorations yet, no HiDPI scaling, full repaint instead of buffer-age damage, no config service connection (defaults only).

### Status 2026-10-26 (beyond the Phase 1 deliverable)

Implemented and verified (unit tests + headless screenshots in `docs/screenshots/`): server-side decorations (title bar, buttons, borders, resize zones; xdg-decoration + KDE decoration), GPU effects (rounded corners, soft shadows, gradients, backdrop blur element), theme-driven rendering (`desilium-theme`), wallpapers with crossfade, own cursor rendering with cursor-shape-v1, motion (open/maximize/minimize/focus transitions, close via snapshot), maximize/fullscreen/minimize/snap (keyboard and drag-to-snap with preview), virtual desktops, config store with live reload (inotify), wlr-layer-shell with exclusive zones, primary selection, viewporter, fractional-scale, xdg-activation, linux-dmabuf, idle-notify/inhibit, headless screenshot mode, `desilium-session` supervisor (library + binary, not yet wired into a real session start).

Decided: the shell UI is built on Blitz (`spikes/blitz/REPORT.md`: GO, behind a replaceable `desilium-ui` interface). Next: private protocol `desilium-shell-v1` (window/workspace list and actions), `desilium-ui` (Blitz host for layer-shell surfaces), `desilium-shell` (top bar, dock, launcher), integrating backdrop blur into panels/popups, DRM/KMS backend, notifications daemon.

Known gaps: popups are unstyled (no rounded clip/shadow), text-input/IME and xdg-toplevel-icon missing, presentation-time not advertised, state requests (maximize/minimize) from clients on inactive desktops are ignored, close animation only when a snapshot exists, real-time pacing in the winit backend not yet verified.

### Running it safely (Docker)

`scripts/docker-run.sh` builds `docker/Dockerfile` and runs the compositor as a client of the host Wayland session. The container gets only the host Wayland socket and the GPU render node: read-only root, no capabilities, no network, `no-new-privileges`, PID/memory/CPU limits, non-root user, `--init` as PID 1. Extra arguments go to the compositor: `scripts/docker-run.sh --command "foot -d info"`.

## 10. Open questions

1. **wgpu timing:** when does the effects pipeline move from GLES to wgpu? Decide after the first prototype with measurements (`PL-RS-019`). Smithay's wgpu support and interop with DMA-BUF import need evaluation first.
2. **Shell rendering model:** Blitz renders to its own surface per client, so the shell is a normal client. Confirm Blitz can do layer-shell surfaces with acceptable latency before committing.
3. **Decoration widgets:** server-side is decided; open is who owns the title-bar widgets (compositor-built vs. rendered from theme assets).
4. **Supervisor protocol:** how `desilium-session` talks to its children (readiness notification, health pings, restart policy/backoff) and what it does on repeated crash loops (safe-mode shell).
5. **Multi-GPU / hybrid graphics:** out of scope for Phase 1; the DRM backend design should not make it impossible.
6. **Tokio vs. calloop boundary:** is a dedicated async thread for D-Bus enough, or do we want a tokio-calloop bridge crate?

## 11. Pureline conventions for this codebase

Pureline 1.5 is the binding style for all Rust code. How its rules map onto the architecture:

| Rule | Consequence here |
|---|---|
| PL-RS-010 / PL-STRUCT-002 (max 10 production `.rs` files per folder) | `backend/`, `state/`, `wm/`, `render/` are separate folders from the start; `state/` splits handlers per protocol group (`xdg.rs`, `seat.rs`, `output.rs`, …) instead of one giant file. `wm/` splits into `workspace`, `layout`, `focus`, `snap`, … once it grows. |
| PL-RS-009 / PL-STRUCT-003 (cohesion over size) | `DesiliumState` and per-algorithm `impl` blocks may be long if cohesive; split only on real responsibility boundaries. |
| PL-RS-008 (traits need a real abstraction) | Traits only once a second implementation exists. Phase 1 has exactly one backend (winit), one layout (floating) and one renderer (GLES), so there are plain modules/functions instead; the `Backend`, `Layout` and renderer traits are introduced when DRM/KMS, tiling or wgpu land. No manager/factory traits. No `I` prefix. |
| PL-RS-004 (no `unwrap`/`expect`/`panic!`) | Matches §7: handlers never panic on client data. Allowed only in tests and documented invariants. |
| PL-RS-002/003 (guard clauses, `let … else`, `?`) | Handlers validate early and return protocol errors; typed error enums per crate (`thiserror`). |
| PL-RS-013 (async tasks have ownership/shutdown) | The D-Bus thread and any tokio tasks are owned by a handle with explicit `shutdown()`; no fire-and-forget for important work. Never hold a lock across `.await`. |
| PL-RS-014 (RAII + explicit `close()`) | Backends, outputs and the supervisor children expose `shutdown()`; `Drop` only does infallible cleanup. |
| PL-RS-015/016 (unsafe, FFI isolated) | DRM/GBM/libseat/Wayland FFI lives in the `backend/` modules; any `unsafe` carries `// SAFETY:` and a minimal safe wrapper. Generated protocol bindings (`desilium-protocols`) are exempt from density limits. |
| PL-RS-017 (types encode invariants) | Newtypes for `WindowId`, `OutputId`, `WorkspaceId`, logical vs. physical coordinates. |
| PL-RS-011/020 (rustfmt, clippy) | CI runs `cargo fmt --check` and `cargo clippy`; no blanket `allow` attributes. |
| PL-RS-019 | Frame-latency benchmarks come before any low-level optimisation. |
| PL-CORE-020 / PL-ARCH-001 | Dependencies are passed explicitly (no global state / service locators); `wm/` takes everything through parameters. |

Open check: whether the Pureline Checker already supports the Rust profile as a CLI we can run in CI, or if CI uses only `fmt` + `clippy` for now.


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
geometry. System controls use a dedicated worker with fixed argv and bounded
external commands. They never block the compositor or shell event loop.
See [desktop-controls.md](desktop-controls.md) and
[desktop-validation.md](desktop-validation.md).
