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
  the desktop from starting. Problems are repaired, quarantined or reported.
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
- `[compositor]`: window and input behaviour that is applied to KWin: keyboard `shortcuts`
  (action, key and modifiers), the `terminal` to launch (unset means auto-detect), `desktops`,
  `focus`, `raise_on_focus`, `snap_enabled`, `snap_distance`, `keyboard_repeat_delay`,
  `keyboard_repeat_rate` and `keyboard_layout`. The table keeps its name although Desilium no
  longer has a compositor of its own; renaming it would need a schema migration.
- `[appearance]`: `theme` (`desilium-dark`, `desilium-light`, or `kde` to follow the user's KDE
  colour scheme), an optional `accent` as `#RRGGBB` or `#RRGGBBAA` (ignored with `kde`), an
  optional `icon_theme` (a theme directory name; unset follows KDE's `[Icons] Theme`, then
  `breeze`), `wallpaper` and `wallpaper_mode`, `animations`, `animation_speed`, `reduce_motion`,
  `cursor_theme` and `cursor_size`.
- `[shell]`: panel, dock, session-restore and desktop preferences, used by the shell, among them
  `desktop_icons` (default `true`), which shows the desktop folder's contents as icons.

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

Editing `config.toml` by hand, or by another program, is detected without polling by the
programs that consume the configuration: the shell and the KWin bridge each watch the directory
with inotify. Events for `config.toml` (create, modify, move and delete) start a debounce (150 ms
in the bridge), so an editor that writes several times causes one reload.

- A valid file is applied: the shell redraws, the bridge writes the difference into KWin's
  configuration (see [`kwin-integration.md`](kwin-integration.md)).
- An invalid file is rejected with a warning. The last good configuration stays active.
  A rejected file is reported once and is not re-reported until it changes again.
- If inotify cannot be set up, the program continues with the configuration it loaded at startup,
  and a warning is logged.

## Command-line flags

There is no compositor with its own flags any more. The two Desilium programs that read the store
share these:

| Flag | Programs | Meaning |
| --- | --- | --- |
| `--config-dir <path>` | `desilium-shell`, `desilium-kwin` | Use this directory for the store instead of the default. |
| `--theme dark\|light` | `desilium-shell`, `desilium-kwin` | Override the theme id for this run, for screenshots and tests. It wins over the file. The accent and every other setting still come from the configuration. |
| `--once` | `desilium-kwin` | Apply the configuration to KWin once and exit. |
| `--print-plan` | `desilium-kwin` | Print the KWin settings the configuration maps to and exit. Touches nothing. |

The former `--no-config`, `--headless`, `--shell` and `--no-shell` flags of the compositor no longer
exist; the shell is started by `desilium-session`, which also passes the compositor arguments
(see [`session.md`](session.md)).

Logging goes through `RUST_LOG`, as usual. Recovery messages appear at `WARN`.

## How configuration takes effect

Nothing in Desilium renders windows, so the configuration takes effect in two places.

**In KWin, through the bridge.** `compositor` settings and the animation, cursor and theme parts of
`appearance` are projected onto KWin and KDE by `desilium-kwin`: KConfig keys written with
`kwriteconfig6`, the virtual desktop count over D-Bus, global shortcuts through KGlobalAccel. The
complete mapping is in [`kwin-integration.md`](kwin-integration.md#4-configuration-to-kwin-mapping).
`desilium-kwin --print-plan` prints it for the current configuration.

**In the shell and the applications,** which read the store themselves: the theme, accent,
wallpaper, panel and dock preferences.

`appearance` resolves to a `Theme` in both cases:

0. The id `kde` loads the user's KDE colour scheme (`desilium_theme::kde`), ignores the accent and
   falls back to the dark built-in when KDE has no usable scheme. `--theme` still overrides it.
1. Otherwise the theme id is looked up in `BUILTIN_THEMES`. `--theme` replaces it when given.
2. A valid `accent` derives the palette with `Theme::derive_from_accent`.
3. `Theme::validate` repairs anything out of range and reports it.

Theme resolution is a pure function (`resolve_appearance`, in `desilium-kwin` for the bridge) with
unit tests. It does no I/O. The bridge additionally exports the theme as a KDE colour scheme so that
Qt and KDE applications match, except with `theme = "kde"`, where KDE's colours are the source and
nothing is exported.

## Developer API

```rust
// Edit, validate, save and notify in one step. Used by the settings app.
let mut store = ConfigStore::open(ConfigStore::default_dir()).store;
let mut transaction = store.transaction();
transaction.edit(|cfg| cfg.appearance.animation_speed = 1.5);
let report = transaction.commit()?;
// report describes what changed, which values were repaired and which backup was written.
```

- A transaction edits a copy; `commit` validates, backs up the previous file, writes atomically and
  notifies subscribers. Dropping it discards the changes.
- Programs that apply the configuration (the shell, the bridge) do it when they see the file
  change; the settings app does not call into them.
- Do not store runtime state in the configuration. Put it in its own file or service.
- Do not add new settings without a default, a validation rule and a settings control.
- A new setting that KWin should honour needs an entry in `crates/desilium-kwin/src/plan/` and a
  row in `kwin-integration.md`.

The store itself lives in `crates/desilium-config` and is documented in its crate docs.

## Limits (current)

- The terminal setting applies to the next launch, not to running clients.
- Settings that map onto KWin only take effect once the bridge runs and KWin accepts the keys. The
  key names are unverified (see [`kwin-integration.md`](kwin-integration.md#11-verification-status)).
- Output layout, resolution and scaling are KWin's and are not part of the Desilium configuration
  yet.
- A shortcut removed from the configuration is not unbound in KWin; its default binding remains.
