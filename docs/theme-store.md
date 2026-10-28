# Theme store

Settings → **Get new themes** (`desilium-settings --page themes`) browses the KDE Store
(store.kde.org, the Pling platform), downloads icon themes, cursor themes and colour schemes and
lets you select them. Everything below was written without access to the live service or to a
running KWin. **Nothing in this document's UNVERIFIED list has been tried.** What is tested are the
parsers, the safety checks, the install step with archives built in the tests, and the page logic
with a stand-in for the network.

## What the page does

- Three tabs: **Icons**, **Cursors**, **Colours**. Each has a search field (Enter submits), the
  results of one page (10 entries, most downloaded first), Previous/Next, and a button that opens
  the store's own page for that category in the browser (`xdg-open`).
- A result shows the title, the author and the download count, and either **Install**, a pending
  "Installing…" (one download at a time) or **Use** for what that entry installed earlier.
- A second card lists everything installed on this computer for the tab (all icon directories, or
  all `color-schemes` directories), with **Use** or "In use".
- Nothing blocks the window: searches and installs run on two background threads and report back
  through the event loop, like the wallpaper thumbnails. While waiting the page says "Loading…";
  a failure (no network, store down, a tool missing) is shown in the card with "Try again".

### Selecting

| Tab | "Use" sets | Effect |
|---|---|---|
| Icons | `appearance.icon_theme` | Icons of the shell and, through the bridge's `kdeglobals` projection, of KDE applications |
| Cursors | `appearance.cursor_theme` | The bridge writes `kcminputrc` as for the Appearance page |
| Colours | `appearance.theme = "kde"` and `appearance.kde_scheme = <name>` in **one** transaction | The shell and Settings read `color-schemes/<name>.colors` instead of the scheme named in `kdeglobals` |

All of them go through the normal change path, so Undo, validation and live apply work as for every
other setting. Only names that exist on disk can be chosen. `appearance.kde_scheme` (validated like
an icon theme name: no `/`, no leading dot) is new. `None` follows the user's `kdeglobals`. KDE
applications are not told about a picked scheme; the bridge does not write it into `kdeglobals`.

## Where things go

| What | Installed to | Marker |
|---|---|---|
| Icon and cursor themes | `$XDG_DATA_HOME/icons/<name>/` (default `~/.local/share/icons`) | `<name>/.desilium-store` holds the store id |
| Colour schemes | `$XDG_DATA_HOME/color-schemes/<name>.colors` | `.<name>.colors.desilium-store` next to it |
| Downloads, unpacking | `$XDG_CACHE_HOME/desilium/store/job-*` (removed after each job; leftovers older than an hour are swept at start) | |

A theme directory that already exists is replaced only when its marker names the same store entry.
Anything else (a system copy in the user directory, a manual install, another entry) is never
touched; the page says so and asks you to remove it first. Several themes in one archive are all
installed (for example `Papirus` and `Papirus-Dark`).

## How it talks to the store

The client uses the Open Collaboration Services API with `curl`, fixed arguments, no shell:

```
https://api.kde-look.org/ocs/v1/content/data?categories=<id>&search=<text>&sortmode=down&page=<n>&pagesize=10&format=json
https://api.kde-look.org/ocs/v1/content/data/<id>?format=json     (details of one entry, if a list lacks the files)
```

Category ids (constants in `store/ocs.rs`): icons 132, cursors 107, colour schemes 112. The answer is
read tolerantly: entries under `data` or `ocs.data.content`, one entry or a list, numbers as numbers
or strings; entries without a numeric id or a name are skipped; `downloadlink1…12` with
`downloadname`/`downloadsize` give the files. The first installable file is used (`.tar.gz`, `.tgz`,
`.tar.xz`, `.txz`, `.tar.bz2`, `.tbz2`, `.tar.zst`, `.tar`, `.zip`; a bare `.colors` file is preferred
for colour schemes).

Tools: `curl` (always), `tar` (archives), `unzip` (zip archives). A missing tool is reported by name.

## Safety

The download is untrusted data from a third party, so:

- **HTTPS only**, for the request, for every redirect (`--proto =https --proto-redir =https`) and
  for every link in an answer; links with user info, whitespace, control characters or over 2 KiB
  are dropped. The download host is whatever the store names (files live on several hosts); it is
  not pinned.
- **Limits**: 4 MiB for an API answer, 200 MiB for a download (checked by `curl` and by watching
  the file grow, since `--max-filesize` only sees a declared length), 15 s to connect, 30 s for an
  answer, 15 min for a download, 3 min per `tar`/`unzip` run, at most 50 000 entries and 1 GiB
  unpacked.
- **Before unpacking**, the archive is listed twice (`tar -tf` and `tar -tvf`, which must agree
  entry for entry) and refused as a whole if any entry has an absolute path, a `..` component, a
  backslash, a NUL, a symbolic link that leaves the archive (resolved lexically from the link's
  own directory), a hard link or a device/pipe/socket. `tar` runs with `--no-same-owner
  --no-same-permissions`.
- **After unpacking**, the tree is walked again (no special files, no escaping link, size and depth
  bounds) before anything is moved.
- **What is installed** must be what was asked for: an icon theme is a top-level (or, if there is
  none, second-level) directory whose `index.theme` has `Directories`; a cursor theme has a
  `cursors/` directory; a colour scheme is a `*.colors` file. Everything else in the archive is
  thrown away. Names must be a single plain path component (no `/`, `\`, leading dot, control
  characters; at most 64 characters).
- **Nothing is executed.** Icon, cursor and colour themes are data. Archives with scripts,
  installers or Plasma packages are not unpacked into place, and `kpackagetool` is not called.
- The page escapes every store text before it reaches the document.

Not covered: a zip bomb that unpacks to more than the disk holds is only noticed after unpacking
(the 1 GiB check runs afterwards); a theme can still be ugly or broken; there is no signature
check (the store offers none this app uses).

## UNVERIFIED

None of the following could be checked in the environment where this was written (no network, no
KDE Store account, no KWin). They are the likeliest places for a first real run to fail.

| Item | Status |
|---|---|
| Endpoint `https://api.kde-look.org/ocs/v1/content/data` answers JSON with `format=json` and accepts `categories`, `search`, `sortmode` (`new`, `alpha`, `high`, `down`), `page` (from 0) and `pagesize` | UNVERIFIED |
| Category ids 132 (full icon themes), 107 (cursors) and 112 (colour schemes) | UNVERIFIED (from memory; a Plasma 6 global-theme or Plasma-style category is not offered) |
| Field names in an answer (`id`, `name`, `personid`, `downloads`, `score`, `detailpage`, `downloadlink1..`, `downloadname1..`, `downloadsize1..`, `totalitems`, `status`) and that a **list** answer already carries `downloadlink1` (otherwise the detail request is used) | UNVERIFIED |
| That `downloadlink1` is a direct file link rather than a page or a redirect that needs a session, and that `curl -L` over https reaches the file | UNVERIFIED |
| `https://store.kde.org/browse?cat=<id>` is the right address of a category's page | UNVERIFIED |
| `tar -tvf` prints the entry type as its first character and `name -> target` for links on the installed `tar` (GNU tar and bsdtar both do, tested with GNU tar only), and `unzip -Z1` lists names | UNVERIFIED for bsdtar and unzip; GNU tar is exercised by the tests |
| That a scheme chosen through `appearance.kde_scheme` looks right with real schemes (only a Breeze-like sample is tested), and that icon and cursor themes installed to `~/.local/share/icons` are picked up by KWin and Qt applications after the bridge writes the setting | UNVERIFIED |
| Preview pictures of store entries are not shown (the page never loads images from the network) | not implemented |

## Code map

- `store/ocs.rs`: URLs and answer parsing (pure, unit-tested).
- `store/process.rs`: running `curl`/`tar`/`unzip` with a time limit, an output limit and a watched file.
- `store/install.rs`: listing checks, unpacking, tree walk, kind detection, installing, markers, installed lists.
- `store/worker.rs`: the two background threads and the request functions.
- `store/state.rs`, `app/themes.rs`, `view/themes.rs`: the page's state, actions and markup.
- `desilium_theme::kde::load_theme_for`: reads a picked scheme (used by the shell and by Settings).
