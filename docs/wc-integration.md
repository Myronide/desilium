# WC integration

The WC prototype from the sibling `wc` directory was imported into
`spikes/wc`, including source, its own lockfile, demo client, smoke test and
Smithay attribution. Build artifacts and screenshots were excluded. The
original sibling source files are unchanged.

This is a source integration, not a Git merge with an unrelated history:
WC did not have a Git repository. Local work lives on `integrate-wc`.

## Production integration

Desilium already had a more complete compositor, window manager, shell,
configuration service and Blitz UI. Those implementations remained the primary
desktop. WC's additional controls were added to Desilium's existing interfaces:

- Click or pointer focus, optionally without raising the window.
- Drag-to-edge snapping enabled/disabled and configurable edge distance.
- Keyboard repeat delay/rate.
- GUI controls, persistence, validation, reset and Undo through the settings registry.

Since then Desilium's own compositor has been replaced by KWin. The settings stayed in the typed
configuration (`[compositor]`), and the KWin bridge now applies them as KWin settings: focus policy
and raise, the snap zones and edge actions, and the keyboard repeat and layout (see the mapping in
[kwin-integration.md](kwin-integration.md#4-configuration-to-kwin-mapping)). The behaviour that
the old compositor implemented itself (for example repeat updates for already connected clients) is
now KWin's, and has not been checked there.

The existing 8 px snap distance and 200 ms / 25 Hz repeat defaults were
preserved for old documents. Kitty is now the terminal default. Explicitly
chosen terminals are preserved; clearing the terminal selects automatic
probing with Kitty first.

The new desktop work is documented in [desktop-controls.md](desktop-controls.md)
and [doria.md](doria.md).

## Why the prototype keeps a separate workspace

WC is its own compositor prototype: it uses the current Smithay Git API and Dioxus Native /
Blitz 0.2, while Desilium uses KWin as compositor and its own Blitz 0.3 adapter for the UI. Mixing
those APIs would introduce two competing compositors and two configuration paths. The excluded
nested workspace preserves WC as an executable reference without adding it to the production
binaries. It keeps its Smithay dependency and is not part of Desilium's build.

```sh
cargo check --manifest-path spikes/wc/Cargo.toml --workspace --locked
cargo test --manifest-path spikes/wc/Cargo.toml --workspace --locked
```

`./scripts/wc-dev.sh` starts the original prototype. For testing a Desilium session
(KWin, shell and bridge), follow the sandbox instructions in [testing.md](testing.md).

## Validation

The production workspace has unit tests for configuration compatibility, the mapping of focus,
snapping and keyboard settings onto KWin's keys, and GUI transactions. (The tests of the former
compositor for focus without raising and real Wayland keyboard-repeat events were removed with
it.) The imported workspace is checked independently. License attribution remains in `spikes/wc/THIRD_PARTY.md` and
`spikes/wc/LICENSE-SMITHAY`.
