# Desilium

Desilium is a desktop environment for Linux, written in Rust on top of
Wayland. It is built to work out of the box: no configuration files to write,
every setting reachable from a graphical interface, and a visual style that
aims for the polish of a modern consumer operating system.

The codebase is modular. The compositor, window management, configuration,
theming, animation and text rendering are separate crates with small,
testable interfaces. The compositor is built on [Smithay](https://github.com/Smithay/smithay).

> **Status: early development.** The first phase (a nested Wayland compositor
> with floating window management) is implemented. Most desktop components are
> still being built. Expect breaking changes.

## Principles

- **Zero-config.** Sensible defaults for everything. Users never need to edit a
  file to use, personalize, maintain or troubleshoot Desilium.
- **GUI-first.** Settings are typed, validated and changeable from the desktop.
- **Modular.** Each concern is a crate with a narrow responsibility. The
  compositor does not know about themes, and the wallpaper does not know about
  windows.
- **Premium look.** Smooth animation, consistent typography and a considered
  theme, with GPU rendering where it helps.

The full product concept is in [`concept.md`](concept.md). The compositor design
and its decisions are in [`docs/architecture.md`](docs/architecture.md).

## Roadmap

Phases follow section 9 of [`concept.md`](concept.md) and the Phase 1 status
in [`docs/architecture.md`](docs/architecture.md).

| Phase | Scope | Status |
|-------|-------|--------|
| 1. Foundation | Cargo workspace, Smithay integration, minimal compositor, floating window management, nested development environment, logging and tests | Implemented (nested winit backend) |
| 2. UI foundation | Rendering abstraction, theme system, panels, application launcher, input and performance validation | In progress (crates scaffolded) |
| 3. Configuration platform | Central configuration service, typed schemas, validation and transactions, automatic defaults, live change notifications, graphical settings, rollback and recovery | Planned |
| 4. Desktop features | Multi-monitor, window snapping, virtual desktops, notifications, system tray, quick settings, audio, network, power management, lock screen | Planned |
| 5. Integration | XWayland, desktop portals, screen sharing and capture, clipboard, drag and drop, accessibility, session management | Planned |
| 6. Stabilization | Profiling, memory work, crash recovery, configuration migration, hardware testing, security and accessibility review, regression testing | Planned |

## Crates

All crates live under `crates/` and share one workspace version.

| Crate | Responsibility |
|-------|----------------|
| `desilium-compositor` | Smithay-based Wayland compositor: backend, event loop, protocol handlers, rendering. Runs as a nested window on the winit backend. |
| `desilium-wm` | Window management without Smithay: stacking, focus, placement, resize anchoring, maximize/fullscreen/minimize, edge snapping and virtual desktops. |
| `desilium-config-types` | Typed, serializable settings and keyboard shortcuts with defaults. |
| `desilium-config` | Configuration service: loading, validation, transactions and change notifications. |
| `desilium-theme` | Theme model: colours, metrics and typography tokens. |
| `desilium-anim` | Deterministic animation engine (easing, tweens, springs). No clock access. |
| `desilium-text` | Text shaping and glyph rendering for UI and window decorations. |
| `desilium-cursor` | Cursor theme loading and cursor image selection. |
| `desilium-wallpaper` | Wallpaper loading and scaling for outputs. |
| `desilium-session` | Session lifecycle: starts and supervises the desktop components. |

Several crates are still scaffolds for upcoming phases. The descriptions above
state each crate's intended responsibility; the crate's `src/` shows what is
implemented so far.

## Building

Desilium needs Rust 1.85 or newer (edition 2024). The repository includes a
`rust-toolchain.toml` that selects the stable channel with `rustfmt` and
`clippy` through [rustup](https://rustup.rs).

System libraries are needed for the compositor's native dependencies.

**Arch Linux**

```sh
sudo pacman -S --needed base-devel pkgconf wayland libxkbcommon libglvnd mesa libinput libdrm systemd seatd
```

**Debian and Ubuntu**

```sh
sudo apt install build-essential pkg-config libwayland-dev libxkbcommon-dev \
    libegl-dev libgles-dev libgbm-dev libdrm-dev libinput-dev libudev-dev libseat-dev
```

Then build and test:

```sh
cargo build --workspace
cargo test --workspace
```

Run `scripts/check.sh` before pushing. It runs the formatter, clippy with
warnings as errors, the tests and the style checker. See
[`CONTRIBUTING.md`](CONTRIBUTING.md) for details.

## Running

### Safely, in Docker (recommended)

`scripts/docker-run.sh` builds the image from `docker/Dockerfile` and runs the
compositor as a client of your current Wayland session. It must be started from
inside a Wayland session, because it needs `WAYLAND_DISPLAY` and
`XDG_RUNTIME_DIR`.

```sh
scripts/docker-run.sh                          # build the image, then run
scripts/docker-run.sh --no-build               # reuse the existing image
scripts/docker-run.sh --command "foot -d info" # start a different first client
```

The container is locked down:

- **Minimal host access.** It receives only the host Wayland socket and the GPU
  render node. No other devices, no host filesystem, no network.
- **No privileges.** No Linux capabilities, `no-new-privileges`, a non-root user
  and a read-only root filesystem with `/tmp` as the only writable location.
- **Bounded resources.** A PID limit, a memory limit and a CPU limit, with
  `--init` reaping child processes.

Running the compositor directly on the host is not recommended during
development. See [`docs/testing.md`](docs/testing.md) for the full safety rules.

### Default shortcuts

These are the Phase 1 defaults from `crates/desilium-config-types/src/settings.rs`.

| Shortcut | Action |
|----------|--------|
| `Alt` + `Enter` | Launch a terminal |
| `Alt` + `Q` | Close the focused window |
| `Alt` + `Shift` + `Q` | Quit the compositor |

Alt is used instead of the Logo key because Desilium is developed nested
inside another desktop, which already uses the Logo key.

## Testing and screenshots

Unit tests live next to the code in each crate. Headless screenshots and the
test layers are described in [`docs/testing.md`](docs/testing.md).

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
