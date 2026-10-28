# Headless runs

Desilium used to ship a compositor with a headless screenshot mode (`--headless`, `--screenshot`,
`scripts/docker-screenshot.sh`, animation freezing, glass debug variables). That compositor is gone;
KWin has taken its place, and none of that tooling exists any more. There is **no replacement for
composed headless screenshots yet**. This page records what is possible today.

## Rendering the Desilium applications without a display

The shell, the launcher and Doria render HTML/CSS documents through Blitz. They can be turned into
PNG files without a compositor:

```sh
cargo test -p desilium-shell                                   # snapshot tests, PNGs in target/shell-snapshots/
./target/debug/doria --render screenshots/doria.png /absolute/folder
```

These images show what an application draws. They contain no window decoration, no wallpaper and no
KWin effect. The `screenshots/` directory is ignored by Git.

## Running KWin without a display

KWin has a virtual backend, intended for tests: `kwin_wayland --virtual` (from memory; check
`kwin_wayland --help` on the installed version). It creates a Wayland socket without any output
device. With `desilium-session` the flag is passed after `--`:

```sh
target/debug/desilium-session --allow-any-client -- --virtual
```

This is the intended basis for automated checks against a live KWin (starting the session, running
the bridge with `--once` against a throw-away `XDG_CONFIG_HOME`, checking that the shell binds the
plasma protocols). **None of that has been tried.** Whether the virtual backend offers enough for
the shell's layer-shell surfaces, and how to take a screenshot from it (for example through KWin's
screenshot D-Bus interface or its screen-casting stream), is open.

Until then, visual changes are reviewed with the Blitz renders above, or on a nested KWin with the
host's screenshot tool.

## Environment variables

The variables of the old compositor (`DESILIUM_DEBUG_POPUP`, `DESILIUM_DEBUG_GLASS`,
`DESILIUM_DEBUG_LAYER_BLUR`, `DESILIUM_WALLPAPER`, `DESILIUM_DEBUG_SNAP`, `DESILIUM_SHELL_ALLOW_ANY`)
no longer have an effect. The development switch for the permission model is the session flag
`--allow-any-client` (see [`kwin-integration.md`](kwin-integration.md#9-development)).
