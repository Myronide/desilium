# Testing Desilium

This document describes how Desilium is tested: the unit tests in each crate, what is and is not
tested against KWin, and the safety rules for running a session. For the everyday checks, see the
"Running the checks" section of [`CONTRIBUTING.md`](../CONTRIBUTING.md).

The strategy follows section 7 of [`architecture.md`](architecture.md). In short: pure logic
(including the whole mapping from the configuration to KWin) is tested without any display, the
process handling is tested with stand-in scripts, UI documents are rendered headlessly through
Blitz, and the behaviour against a live KWin is checked by hand and is **not automated yet**.

## Test layers

| Layer | Where | How to run | Notes |
|-------|-------|------------|-------|
| Unit tests | Each crate, `#[cfg(test)]` modules | `cargo test --workspace` | Deterministic. No wall-clock sleeps: time is passed in explicitly. |
| Single crate | Any `crates/desilium-*` | `cargo test -p desilium-kwin` | Fastest feedback while you work on one crate. |
| Config schema | `desilium-config-types` | `cargo test -p desilium-config-types` | Round-trip, empty and partial documents, invalid input. |
| KWin mapping | `desilium-kwin` | `cargo test -p desilium-kwin` | The plan (configuration to KConfig keys), the shortcut translation, the geometry script, the retry bookkeeping. `kwriteconfig6` is replaced by stand-in scripts. No KWin needed. |
| Session | `desilium-session` | `cargo test -p desilium-session` | Plan, environment, restart policy, readiness gate, with shell-script stand-ins instead of `kwin_wayland`. |
| Shell | `desilium-shell` | `cargo test -p desilium-shell` | State machines of the protocol client, blur regions, wallpaper look, and headless Blitz snapshots of the shell documents. |
| Against a live KWin | by hand | see [`kwin-integration.md`](kwin-integration.md#11-verification-status) | Not automated. Nothing in the integration has been run against a real KWin yet. |

Each test should cover the normal path, the edge cases and the error paths.
A test that only covers the happy path does not count as complete.

### Unit tests per crate

Run the tests of one crate by name:

```sh
cargo test -p desilium-kwin
cargo test -p desilium-config-types
```

Pass a test name filter to run a subset:

```sh
cargo test -p desilium-kwin shortcuts
```

Use `-- --nocapture` to see `println!` or `tracing` output from a failing test.
Log output of the programs is controlled with `RUST_LOG`.

### Looking at the KWin mapping without KWin

The bridge can print what it would write for a configuration:

```sh
cargo run -q -p desilium-kwin -- --print-plan --config-dir target/empty-config
```

This is the quickest way to check a change to the mapping. It touches neither KWin nor any
KDE configuration file.

### Test conventions

- Tests may use `unwrap()` and `expect()`. Production code may not (see the
  Pureline rules in `CONTRIBUTING.md`).
- Tests must not depend on the wall clock, the network, the user's home
  directory or a running Wayland session. Pass in the time, paths and
  environment that the test needs.
- Tests must never run the real `kwriteconfig6` or talk to the real session bus about KWin: that
  would change the developer's desktop configuration. Use the stand-in writers in the tests.
- A test that needs a GPU checks for the render node first and returns early
  when it is absent. CI runners have no GPU, so those tests are skipped there.

## Screenshots

The old compositor could render itself headlessly into a PNG (`scripts/docker-screenshot.sh`). That
mode is gone with the compositor and has **no replacement yet**. What remains:

- The shell and Doria render their own documents through Blitz without a display: the shell's
  snapshot tests write PNGs under `target/shell-snapshots/`, and Doria has
  `doria --render FILE.png /folder`. These show what the applications draw, not a composed desktop
  with window decorations.
- A composed desktop can be captured by running KWin nested and taking a screenshot with the host's
  tools, or in the future with `kwin_wayland --virtual` (see [`headless.md`](headless.md)).

Do not commit screenshots to the repository. Attach them to the pull request instead.

## Running a session safely

A Desilium session starts KWin, which with `WAYLAND_DISPLAY` set runs as a window in your current
session (this has not been verified on a real KWin). Running it outside a sandbox gives it access
to your display and input, and the bridge writes KDE configuration files into your configuration
directory. Follow these rules:

1. **Prefer the sandbox script.** Run the session with
   [`scripts/docker-run.sh`](../scripts/docker-run.sh). It builds the image of the chosen distro
   (Debian `docker/Dockerfile` by default, `--arch`, `--ubu` or `--fed`) and starts
   `desilium-session` in a container with the flags below. Arguments replace the image's default
   (`--allow-any-client`) and go to `desilium-session`; arguments after a literal `--` go to
   `kwin_wayland`. The four images were not built or run in the environment where this text was
   written; their package names are from memory.
2. **If you run nested on the host,** use `target/debug/desilium-session --allow-any-client` and
   point `XDG_CONFIG_HOME` at a throw-away directory, because the bridge changes `kwinrc`,
   `kdeglobals`, `kcminputrc` and `kxkbrc`.
3. **Pass only what is needed.** The container receives the host Wayland
   socket (mounted as `host-compositor` and addressed by absolute path, because KWin
   creates its own `wayland-0` in the same directory) and the GPU render node. It gets no
   other device, no capabilities except `SYS_NICE`, no network and no writable host path. The
   only writable, persistent location is the home volume described below, which is
   Docker-managed storage and not a host directory.

   **The nested KWin window has no title bar on GNOME.** Mutter does not offer server-side
   decorations to Wayland clients, and KWin's nested backend draws none of its own. The window
   therefore starts at `DESILIUM_WIDTH` x `DESILIUM_HEIGHT` (1600x900), and moving or resizing
   goes through the host's window shortcuts (on GNOME for example `Alt+F7`, `Alt+F8` or
   `Super` plus a mouse drag; not verified with a nested KWin). Applications inside the session
   still get KWin's own decorations.
4. **Never start or stop containers you do not own.** The machine may run
   many unrelated containers. Only operate on containers named `desilium-*`.
   Check the name with `docker ps --format '{{.Names}}'` before any `docker stop`
   or `docker rm`.
5. **Keep the test loop on the host when possible.** Unit tests do not need a live session and run
   fine on the host.
6. **Stop a runaway instance** with `docker stop desilium-dev`, or press
   `Alt+Shift+Q` in the nested session (the bridge turns that shortcut into a stop of
   `desilium-session`; this relies on KGlobalAccel and is unverified).

The sandbox flags used by `scripts/docker-run.sh` are:

- Read-only root filesystem. Writable are `/tmp` (tmpfs, scratch) and the home `/home/dev`, a
  named Docker volume (see "The development user space" below).
- `--cap-drop ALL` plus `--cap-add SYS_NICE` (Arch's `kwin_wayland` has the file capability
  `cap_sys_nice+ep`; without the capability in the bounding set `exec` fails with `EPERM`),
  `no-new-privileges`, a non-root user, `--init` as PID 1.
- `--network none`. There are deliberately no PID, memory or CPU limits: the development
  container may use whatever the host has, so builds and the nested compositor are not throttled.

`--arch` (Arch image) relaxes some of these for package installation; see
[`desktop-controls.md`](desktop-controls.md#arch-linux-test-image).

Do not add `--privileged`, host network, host PID or `/dev` mounts beyond the
render node to make something work. If the session needs more access, change
the design or document the new permission in the pull request.

Example:

```sh
scripts/docker-run.sh                                            # build, then run with --allow-any-client
scripts/docker-run.sh --no-build                                 # reuse the existing image
scripts/docker-run.sh --allow-any-client -- --width 1600 --height 900
```

### The development user space

The container is a pseudo system for developing Desilium. It runs as the user `dev` (uid/gid 1000)
with the home `/home/dev` and the XDG defaults below it (`HOME=/home/dev`; the old `/tmp` overrides
are gone). The user name is a build argument (`USER_NAME`), set by `DESILIUM_USER`, so the same
images can carry a real user name later. The home is seeded from `docker/home-skel/`:

- the folders `Applications` (Desilium's place for application bundles and launchers), `Desktop`,
  `Documents`, `Downloads`, `Music`, `Pictures`, `Public`, `Templates` and `Videos`, plus
  `~/.config/user-dirs.dirs` and `user-dirs.locale` (`en_US`);
- `Pictures/Wallpapers` with the files of `assets/wallpapers`;
- executable launchers for Doria and Desilium Settings in `Desktop` (mode 0755, because Desilium
  only starts launchers that have the executable bit);
- `Documents/Welcome.txt`, which explains the layout.

`scripts/docker-run.sh` mounts a **named Docker volume** at `/home/dev` (`desilium-dev-home-<distro>`,
override with `DESILIUM_HOME_VOLUME`). Docker initialises a new volume from the image's `/home/dev`
(content and ownership) on first use; from then on everything the session writes there, such as
the KWin and Desilium configuration of the bridge, desktop files and pins, persists between runs.
The volume is the only writable, host-persistent location, and it is Docker-managed storage, not a
host path. With `--sudo` (Arch) the root filesystem is writable as well, but the same volume is
used. An image rebuilt with a changed skeleton does not touch an existing volume: use
`--reset-home`.

Each distro has its own image (`desilium-dev`, `-arch`, `-ubuntu`, `-fedora`), home volume
(`desilium-dev-home-debian`, `-arch`, ...) and saved folder (`pseudo-debian/`, `pseudo-arch/`,
`pseudo-ubuntu/`, `pseudo-fedora/`; ignored by git).

| Flag | Effect |
|------|--------|
| (none) | Debian image `docker/Dockerfile` |
| `--arch` | Arch image `docker/Dockerfile.arch`; passwordless sudo/pacman unless `--no-sudo` |
| `--ubu` | Ubuntu image `docker/Dockerfile.ubuntu` (`ubuntu:rolling`, Plasma 6 needs a recent Ubuntu) |
| `--fed` | Fedora image `docker/Dockerfile.fedora` |
| `--no-build` | reuse the existing image |
| `--net` | bridge network instead of none |
| `--sudo` / `--no-sudo` | Arch only: weaker isolation for package installs / back to locked down |
| `--fresh-home` | throw-away home: an anonymous volume at `/home/dev`, initialised from the image (skeleton included) and removed with the container (`--rm`) |
| `--reset-home` | delete the distro's home volume (refused while a container uses it) and exit; it continues only when `desilium-session` arguments follow |
| `--save` | keep the **whole system**, the root filesystem `/` with the home and everything installed (for example with `pacman` on `--arch`), in `pseudo-<distro>/system.tar` in the repository. The session runs in a container that is kept (no `--rm`) and has a writable root filesystem and no home volume; when it ends the container is committed (`docker commit`) and written to the file (`docker save`, no bind mount, the file belongs to you). The next `--save` run loads the file and continues from it; if it does not exist yet, the run starts from the image. Delete `pseudo-<distro>/` to start from the image again. `/tmp` is not saved. Every save adds an image layer (Docker allows about 125; the script warns at 100). Without `--save` the file is neither read nor written. Not combinable with `--fresh-home` |
| `--width N`, `--height N` | size of the nested KWin window (default 1600x900) |
| `--check` | smoke test without a display, see below |

`--check` (for example `scripts/docker-run.sh --ubu --check`) builds the image (unless
`--no-build`) and runs a PASS/FAIL list inside the locked-down container: not root,
`desilium-session --dry-run` prints a plan containing `kwin_wayland`, `desilium-shell` and
`desilium-kwin`, `kwin_wayland --version`, `kwriteconfig6 --version`, `kglobalacceld` is
installed, the home skeleton and `user-dirs.dirs` exist, the launchers are executable, and the
`breeze` and `hicolor` icon themes are present. It needs neither a Wayland socket nor a render
node and exits non-zero if a check fails.

None of this has been run: the images were not built in the authoring environment, and the
package names (especially the KDE ones on Ubuntu and Fedora) are from memory.
