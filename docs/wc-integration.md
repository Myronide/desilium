# WC integration

The WC prototype from the sibling `wc` directory was imported into
`spikes/wc`, including source, its own lockfile, demo client, smoke test and
Smithay attribution. Build artifacts and screenshots were excluded. The
original sibling source files are unchanged.

This is a source integration, not a Git merge with an unrelated history:
WC did not have a Git repository. Local work lives on `integrate-wc`.

## Production integration

Desilium already had a more complete compositor, window manager, shell,
configuration service and Blitz UI. Those implementations remain the primary
desktop. WC's additional controls now use Desilium's existing interfaces:

- Click or pointer focus, optionally without raising the window.
- Drag-to-edge snapping enabled/disabled and configurable edge distance.
- Keyboard repeat delay/rate, including updates to already connected clients.
- GUI controls, persistence, validation, reset and Undo through the settings registry.

The existing 8 px snap distance and 200 ms / 25 Hz repeat defaults were
preserved for old documents. Kitty is now the terminal default. Explicitly
chosen terminals are preserved; clearing the terminal selects automatic
probing with Kitty first.

The new desktop work is documented in [desktop-controls.md](desktop-controls.md)
and [doria.md](doria.md).

## Why the prototype keeps a separate workspace

WC uses the current Smithay Git API and Dioxus Native / Blitz 0.2. Desilium
uses released Smithay 0.7 and its own Blitz 0.3 adapter. Mixing those APIs or
replacing Desilium's compositor would discard working features and introduce
two competing configuration paths. The excluded nested workspace preserves
WC as an executable reference without adding it to the production binaries.

```sh
cargo check --manifest-path spikes/wc/Cargo.toml --workspace --locked
cargo test --manifest-path spikes/wc/Cargo.toml --workspace --locked
```

`./scripts/wc-dev.sh` starts the original prototype. For testing Desilium's
compositor itself, follow the sandbox instructions in [testing.md](testing.md).

## Validation

The production workspace has unit tests for configuration compatibility,
window focus without raising, configurable snapping, GUI transactions and
real Wayland keyboard-repeat events. The imported workspace is checked
independently. License attribution remains in `spikes/wc/THIRD_PARTY.md` and
`spikes/wc/LICENSE-SMITHAY`.
