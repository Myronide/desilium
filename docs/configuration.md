# Configuration

Desilium is configured through its settings application. The configuration file
exists so that the system can store and recover settings. It is an internal storage
format, not an interface that users are expected to edit. Every option has a
graphical control, and nothing in ordinary use requires opening a file.

This document covers how configuration is stored, loaded, applied and recovered,
and how developers use the API. The rules come from `concept.md` sections 3 and 4.

## Principles

- **Zero-config.** Without any file, Desilium runs with built-in defaults. Startup
  never writes a file. The first file appears only when a setting is actually changed.
- **Never blocks startup.** A broken, missing or future-version file never prevents
  the compositor from starting. Problems are repaired, quarantined or reported.
- **Validated before applied.** Every value passes validation, and invalid values are
  replaced by safe ones. A failed change is never applied.
- **Reversible.** Each replaced version is kept as a backup and can be restored.
- **Live.** Changes take effect immediately. Restarting is not required.
- **No secrets.** The file is plain text. Passwords, tokens and keys must go into a
  credential store, never into this file.

## Files and locations

| Path | Purpose |
| --- | --- |
| `$XDG_CONFIG_HOME/desilium/config.toml` | Active configuration. Falls back to `~/.config/desilium/` when `XDG_CONFIG_HOME` is unset or empty. |
| `<dir>/backups/config-<seq>.toml` | Previous versions. The newest 10 are kept, and older ones are removed on rotation. |
| `<dir>/config.toml.corrupt-<n>` | An unreadable file that was moved aside. It is never deleted automatically. |

Permissions are `0600` for files and `0700` for directories. This is Unix-only.

The directory and the file are created only by a committed change. Opening the store
never creates anything.

## What is in the file

`config.toml` is the serialized `DesiliumConfig` from `desilium-config-types`:

- `version`: schema version. Older files are migrated on open.
- `[compositor]`: keyboard `shortcuts` (action, key and modifiers) and the `terminal`
  to launch. An unset terminal means auto-detect.
- `[appearance]`: `theme` (`desilium-dark` or `desilium-light`), an optional `accent`
  as `#RRGGBB` or `#RRGGBBAA`, `wallpaper` and `wallpaper_mode`, `animations`,
  `animation_speed`, `cursor_theme` and `cursor_size`.

Every key has a documented default. Missing keys take their defaults, so a file may
contain only the settings that differ from them.

## Recovery

The store reports every repair as a `Recovery` and logs each one as a warning.

| Situation | What happens |
| --- | --- |
| File is not valid TOML, or is truncated | The file moves to `config.toml.corrupt-<n>`. Defaults are used. |
| File exists but cannot be read (for example, a permission error) | Defaults are used and the file is left alone. |
| A field has the wrong type | That field becomes its default. The rest of the file is kept. |
| A value is out of range (for example, `animation_speed = 9.0`) | The value is clamped and reported. |
| An unknown theme id | The dark theme is used and a warning is logged. |
| An invalid accent | The accent is ignored and a warning is logged. |
| Written by a newer Desilium | It is loaded on a best-effort basis and reported. |
| Older schema | Migrated on open, and the migration is reported. |

Restoring an earlier version uses the normal validate-and-write path. The file being
replaced is backed up first, so a rollback can be undone.

## External edits

Editing `config.toml` by hand, or by another program, is detected by the compositor
without polling. It watches the directory with inotify. Events for `config.toml`
(create, modify, move and delete) start a 100 ms debounce, so an editor that writes
several times causes one reload.

- A valid file is applied and the compositor redraws.
- An invalid file is rejected with a warning. The last good configuration stays active.
  A rejected file is reported once and is not re-reported until it changes again.
- If the directory does not exist yet, its nearest existing ancestor is watched until
  it appears.
- If inotify cannot be set up, the compositor continues with the configuration it
  loaded at startup, and a warning is logged.

When the compositor itself commits a change, it recognizes the file it wrote and does
not reload it.

## Command-line flags

| Flag | Meaning |
| --- | --- |
| `--config-dir <path>` | Use this directory for the store instead of the default. Works with `--headless`. |
| `--no-config` | Use built-in defaults only. Performs no file I/O. Cannot be combined with `--config-dir`. |
| `--theme dark\|light` | Override the theme id for this run, for screenshots and tests. It wins over the file. The accent and every other setting still come from the configuration. |
| `--shell <path>` | Start this desktop shell binary (its file name must be `desilium-shell` to be trusted). Also works with `--command`. |
| `--no-shell` | Start no desktop shell. Cannot be combined with `--shell`. |

`--headless` implies `--no-config` unless `--config-dir` is given explicitly. Screenshot
and CI runs therefore never read or write a user's real configuration.

Logging goes through `RUST_LOG`, as usual. Recovery messages appear at `WARN`.

## How configuration takes effect

All effects go through `DesiliumState::apply_config(&DesiliumConfig)` in
`crates/desilium-compositor/src/state/settings.rs`. Startup, the watcher and
`update_config` all end there.

- `compositor` settings rebuild the keyboard shortcut table and the terminal setting.
- `appearance` resolves to a `Theme`:
  1. The theme id is looked up in `BUILTIN_THEMES`. `--theme` replaces it when given.
  2. A valid `accent` derives the palette with `Theme::derive_from_accent`.
  3. `Theme::validate` repairs anything out of range and reports it.
- The effective appearance is available from `DesiliumState::appearance()`. Animations,
  cursor and wallpaper subsystems read it when they render, so they need no separate
  notification.

Theme resolution is a pure function (`resolve_appearance`) with unit tests. It does
no I/O.

## Developer API

```rust
// Edit, validate, save and apply in one step. Used by the settings app and IPC.
let report = state.update_config(|cfg| {
    cfg.appearance.animation_speed = 1.5;
})?;
// report.changed, report.repairs and report.backup describe what happened.
```

- `update_config` runs a store transaction. Without a store (`--no-config`, `--headless`
  without `--config-dir`) it validates in memory and changes nothing on disk.
- `apply_config` is the only place where a configuration takes effect. Call it directly
  only for configuration that did not come from the store.
- Do not store runtime state in the configuration. Put it in its own file or service.
- Do not add new settings without a default, a validation rule and a settings control.

The store itself lives in `crates/desilium-config` and is documented in its crate docs.

## Limits (current)

- The settings application and its IPC are not implemented. Until then, configuration
  is changed by editing the file or through `update_config` in code.
- The terminal setting applies to the next launch, not to running clients.
- Animations, cursor and wallpaper values are exposed. Their renderers read them as
  those subsystems are integrated.
