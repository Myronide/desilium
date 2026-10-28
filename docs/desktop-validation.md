# Desktop integration validation

> **Historical.** This record was taken on 2026-10-26, before the move from Desilium's own
> compositor to KWin. The test counts below include the removed compositor crates and are no
> longer current. It was not repeated after the migration. The KWin integration has no runtime
> validation yet; see the verification status in [kwin-integration.md](kwin-integration.md#11-verification-status).

Local validation on 2026-10-26, Linux, stable Rust 1.98.0, debug builds:

- `cargo build --workspace --locked`: passed.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo test --workspace --locked`: 1,375 passed; no failures. The UI host's
  two illustrative Wayland doctests remain ignored, as before.
- Imported WC: independent workspace check and tests passed.
- Packaging: shell syntax check and installation dry-run passed. The dry-run
  correctly reports that release binaries need building before a real install.
- Pureline's external executable was not installed; that separate style checker
  was not run.

The tests included real Wayland keyboard-repeat notifications and focus and snap
policy of the former compositor (both are now KWin settings written by the bridge, see
[configuration.md](configuration.md)), panel GUI transactions, explicit secondary-click routing, atomic app
pins, local GIO file copies/renames, symlink handling, cancellation and native
GIO authentication/host-question signals. Tests did not change host Wi-Fi,
Bluetooth, volume, brightness or physical mount state.

## Visual validation

Generated and inspected, not checked into Git:

- `screenshots/doria.png`
- `target/shell-snapshots/quick-dark.png`, `quick-light.png`
- `target/shell-snapshots/taskbar-dark.png`, `taskbar-light.png`

These are actual Blitz renders of the application documents. The quick-panel
snapshots use explicitly supplied test service state, not a claim about the
host's radios or battery.

## Initial performance sample

Doria's headless renderer was run three times against a generated directory of
1,000 small files. The view displays 100 entries per page. Times for complete
process startup, enumeration, layout, rasterization and PNG output were
485.0 ms, 480.3 ms and 476.3 ms (median 480.3 ms). This is a debug-build,
warm-filesystem end-to-end sample, not an input-latency or idle-memory claim.

Reproduce with a temporary fixture directory containing 1,000 files:

```sh
./target/debug/doria --render screenshots/doria-large.png /absolute/fixture
```

Idle work is event-driven: the file manager waits on Wayland, directory/device
notifications and its worker channel. Quick-control queries run when the panel
opens, after an action, or on explicit refresh. Hardware compatibility, live
remote servers and physical drive mount/unmount still need separate testing.
The changed Docker image definition was not built or used for a full graphical
session during this validation.
