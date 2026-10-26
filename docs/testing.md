# Testing Desilium

This document describes how Desilium is tested: the unit tests in each crate,
the headless screenshot workflow, and the safety rules for running the
compositor. For the everyday checks, see the "Running the checks" section of
[`CONTRIBUTING.md`](../CONTRIBUTING.md).

The strategy follows section 8 of [`architecture.md`](architecture.md). In
short: pure logic is tested without Wayland, protocol behaviour is tested
against a headless compositor, and rendering is checked against golden images
where that is practical.

## Test layers

| Layer | Where | How to run | Notes |
|-------|-------|------------|-------|
| Unit tests | Each crate, `#[cfg(test)]` modules | `cargo test --workspace` | Deterministic. No wall-clock sleeps: time is passed in explicitly. |
| Single crate | Any `crates/desilium-*` | `cargo test -p desilium-wm` | Fastest feedback while you work on one crate. |
| Config schema | `desilium-config-types` | `cargo test -p desilium-config-types` | Round-trip, empty and partial documents, invalid input. |
| Window management | `desilium-wm` | `cargo test -p desilium-wm` | Pure logic, no Wayland. Plain unit tests, with property tests where they add value. |
| Headless compositor | Compositor tests | `cargo test -p desilium-compositor` | In-process test client against a headless backend. GPU-dependent tests skip themselves when no render node is present. |
| Headless screenshots | Visual checks | See below | Produces PNG files for manual and golden-image review. |

Each test should cover the normal path, the edge cases and the error paths.
A test that only covers the happy path does not count as complete.

### Unit tests per crate

Run the tests of one crate by name:

```sh
cargo test -p desilium-wm
cargo test -p desilium-config-types
```

Pass a test name filter to run a subset:

```sh
cargo test -p desilium-wm focus
```

Use `-- --nocapture` to see `println!` or `tracing` output from a failing test.
Compositor logging is controlled with `RUST_LOG`; inside the sandbox it is
passed through by `scripts/docker-run.sh`.

### Test conventions

- Tests may use `unwrap()` and `expect()`. Production code may not (see the
  Pureline rules in `CONTRIBUTING.md`).
- Tests must not depend on the wall clock, the network, the user's home
  directory or a running Wayland session. Pass in the time, paths and
  environment that the test needs.
- A test that needs a GPU checks for the render node first and returns early
  when it is absent. CI runners have no GPU, so those tests are skipped there.

## Headless screenshots

Screenshots let you review a visual change without a desktop session. The
wrapper [`scripts/docker-screenshot.sh`](../scripts/docker-screenshot.sh) builds
the image, renders the compositor headlessly inside the sandbox (no display and
no Wayland socket required) and writes a PNG to the host:

```sh
scripts/docker-screenshot.sh                                  # writes screenshots/shot.png
scripts/docker-screenshot.sh --no-build --out screenshots/foot.png \
    --command "foot -e sh -c 'echo hi; sleep 5'"
```

Options after the wrapper's own flags (`--no-build`, `--out <file>.png`) are
passed to the compositor, for example `--size`, `--scale` and
`--screenshot-delay`. The `screenshots/` directory is ignored by Git.

Use screenshots for:

- Reviewing a visual change (theme, animation frame, layout) in a pull request.
- Golden-image comparisons where the renderer is deterministic.

Do not commit screenshots to the repository. Attach them to the pull request
instead.

## Running the compositor safely

The compositor is a Wayland client of your session when run with the winit
backend. Running it outside a sandbox gives it access to your display, input
and files. Follow these rules:

1. **Use the sandbox script.** Run the compositor with
   [`scripts/docker-run.sh`](../scripts/docker-run.sh), not as a bare process.
   It builds `docker/Dockerfile` and starts the container with the flags below.
2. **Pass only what is needed.** The container receives the host Wayland
   socket and the GPU render node. It gets no other device, no capabilities,
   no network and no writable host path.
3. **Never start or stop containers you do not own.** The machine may run
   many unrelated containers. Only operate on containers named `desilium-*`.
   Check the name with `docker ps --format '{{.Names}}'` before any `docker stop`
   or `docker rm`.
4. **Keep the test loop on the host when possible.** Unit tests and
   headless tests do not need a live session and run fine on the host.
5. **Stop a runaway instance** with `docker stop desilium-dev`, or press
   `Alt+Shift+Q` in the compositor window. `--rm` removes the container when it
   exits.

The sandbox flags used by `scripts/docker-run.sh` are:

- Read-only root filesystem, with `/tmp` as the only writable location (tmpfs).
- `--cap-drop ALL`, `no-new-privileges`, a non-root user, `--init` as PID 1.
- `--network none`, PID, memory and CPU limits.

Do not add `--privileged`, host network, host PID or `/dev` mounts beyond the
render node to make something work. If the compositor needs more access, change
the design or document the new permission in the pull request.

Example:

```sh
scripts/docker-run.sh                       # build, then run with the default terminal
scripts/docker-run.sh --no-build            # reuse the existing image
scripts/docker-run.sh --command "foot -d info"
```
