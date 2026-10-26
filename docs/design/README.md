# Desilium Shell: Design Language and Mockups

High-fidelity, static HTML/CSS mockups of the desktop shell, plus the rules behind them. The future `desilium-shell` renders its UI from HTML/CSS with the Blitz engine, so everything here is plain flexbox/grid, CSS custom properties, gradients, `border-radius`, `box-shadow` and opacity. No JavaScript (except a 10-line theme toggle), no `backdrop-filter`, no web fonts, no network.

## Files

| File | Purpose |
|---|---|
| `tokens.css` | Source of truth for both themes (`:root[data-theme="dark"|"light"]`): palette, surfaces, elevation 0-4, radii, spacing, type, motion. Palette values match `crates/desilium-theme/themes/desilium-{dark,light}.toml`. |
| `shared.css` | Shell components (top bar, dock, window chrome, buttons, switch, sliders, segmented control, keycaps, app icons, glass). |
| `icons.svg` | 55 line icons, 24 x 24 grid, 1.5 px stroke, round caps/joins, `currentColor`. |
| `src/*.html` | Page sources with shorthand (`{{wifi}}` expands to an icon, `{{app:files md}}` to an app tile, `{{sprite}}` to the inlined icon set). |
| `*.html` | Generated pages (open these). Run `python3 build.py` after editing `src/` or `icons.svg`. |
| `shoot.sh` | Rebuilds and screenshots every page in both themes into `screenshots/<page>-<theme>.png` (headless Firefox, 1920x1080). |
| `theme.js` | `?theme=light` / saved choice / `?shot` (hides the dev toggle). Each page also has a "Toggle theme" button. |

Pages: `desktop`, `launcher`, `quick-settings`, `notifications`, `lockscreen`, `settings`, `switcher`, `overview`, `window-chrome`.

Why generated pages: Chromium refuses external `<use href="icons.svg#x">` on `file://`, so the sprite is inlined into each page. `icons.svg` stays the single source.

## 1. Principles

1. **Calm, deep, cool.** Surfaces are blue-violet neutrals (hue ~232), never pure black or stark white. Depth comes from stepping lightness, then soft layered shadows.
2. **One confident accent, `#7B61FF`.** Used for focus, selection, the active workspace, primary actions and the active window outline. If it is violet, it means "you are here" or "this is the action".
3. **Generous, concentric radii.** Window 12, panel 16, dock/launcher 20, control 8, pill 999. Inner radius = outer radius minus padding.
4. **Everything is a setting.** The Settings app shows the key behind each control (`appearance.accent`), applies instantly with a live preview and an Undo, and offers a reset per section. There are no config files to edit.
5. **Keyboard first.** Every surface shows its shortcuts (keycaps) inline; the mouse is never required.
6. **Restrained motion.** Fast in, softer out, never bouncy except the dock hover lift.

## 2. Layout decisions

### Top bar + floating dock

- **Top bar (32 px, translucent, full width).** Left: workspace dots and the focused app's name and document. Center: date/time (opens notifications). Right: input method, privacy indicators (microphone), and one grouped **quick-settings button** (Wi-Fi, volume, battery) plus power. Everything system-level lives in one predictable strip.
- **Floating dock (64 px, 14 px above the edge, radius 22).** Launcher button, pinned and running apps, settings, trash. A 4 px dot marks a running app, a 14 x 4 accent pill marks the focused app. Hover lifts the icon 8 px and scales 1.14 with a tooltip.

Why this split:
- The bar is the only place that must always be visible and glanceable (time, battery, network, notifications); a 32 px strip costs little vertical space and anchors window drag targets and the app menu.
- The dock is the *launching and switching* surface. Floating (with a gap) reads as an object, keeps window shadows from fighting a hard edge, and lets maximized windows meet the screen edge.
- Splitting "status" (top) from "apps" (bottom) means each stays small and a screen reader or keyboard user has two clear landmarks.

Alternatives considered:
| Option | Why not |
|---|---|
| Single bottom panel (Windows/KDE style) | Mixes status and apps; long, crowded; clock and tray compete with running tasks. |
| Left edge dock (Ubuntu style) | Eats horizontal space, bad for wide editors/terminals, uneven on ultrawide. |
| Top bar only, no dock (GNOME) | Needs an overview round-trip to switch apps; weak for discoverability. We keep the overview as a power feature, not a requirement. |
| Edge-to-edge dock | Looks like a second panel; floating is lighter and more "designed". |
| Dock auto-hide by default | Hides running-app state. Offered as a setting (`dock.auto_hide`), off by default. |

### Other surfaces

- **Launcher** (800 px, centered at ~15% from top): search, category chips, results grouped by type, pinned strip with `Alt+1..6`, hint bar. The top-hit calculator answer is a card, not a row.
- **Quick settings** (408 px, anchored under the tray): 2-column toggle tiles with a split affordance (tap = toggle, chevron = details), thick sliders, power profile segmented control, media tile.
- **Notifications:** toasts top right (392 px, max 3 visible), notification center under the clock (480 px) with grouped cards, stack peeks, DND switch and a calendar summary.
- **Lock screen:** 176 px / weight 200 clock, date, notification count without content, avatar and password pill.
- **Switcher:** centered glass strip of window thumbnails, selected card gets the accent ring, a caption shows the full title.
- **Overview:** workspace strip on top (active one ringed), window grid of the active workspace below, type-to-search.

## 3. Component inventory

Top bar item, workspace dots, tray group, dock item (+running/focused/hover), tooltip, window chrome (title bar, window buttons, snapped/max/dialog variants), button (default/primary/ghost/icon/small), toggle tile, switch, slider (thin and thick), segmented control, input (+focus), keycap, chip, avatar, app icon tile (12 gradients), list row (hover/selected), result card, notification card (toast, grouped, critical, progress, reply), calendar card, media tile, wallpaper thumbnail, accent swatch, setting row with key caption, glass panel, scrim, mini-preview.

## 4. Interaction states

| State | Treatment |
|---|---|
| Hover | `--hover` overlay (6% white / 5% ink). Dock icons lift. |
| Pressed | `--pressed` overlay; primary uses `--accent-press`. |
| Selected (list, nav) | `--selected` fill + 1 px `--accent-line` inset + 3 px accent bar on launcher rows. |
| Focus (keyboard) | 2 px `--focus-ring` outline with 2 px gap, on every interactive element. Ring is never removed. |
| Active toggle | Solid accent fill, ink text (`--on-accent`). |
| Disabled | `--text-disabled`, no hover, no shadow. |
| Window focused | Accent-tinted border (`--window-border`), elevation 3, full-contrast title. |
| Window unfocused | Neutral border, elevation 1, dimmed title and glyphs. |
| Destructive | Danger fill only for the confirming button and the close hover. |

## 5. Motion spec

Tokens: `--dur-instant 80`, `--dur-fast 120`, `--dur-base 180`, `--dur-slow 280`, `--dur-emphasis 400` ms. Easings: `--ease-standard (0.2,0,0,1)`, `--ease-enter (0.05,0.7,0.1,1)`, `--ease-exit (0.3,0,0.8,0.15)`, `--ease-spring (0.34,1.4,0.64,1)`.

| Interaction | Duration | Easing | Motion |
|---|---|---|---|
| Hover / press feedback | 80-120 | standard | color/opacity only |
| Toggle, switch thumb | 120 | standard | 16 px slide, track color crossfade |
| Dock hover lift | 180 | spring | translateY -8, scale 1.14, neighbors do not move |
| Tooltip | 120 in (delay 400) / 80 out | enter / exit | fade + 4 px rise |
| Menu / popover open | 180 | enter | fade + scale .98 -> 1 from anchor |
| Quick settings / notification center | 220 | enter | fade + 8 px slide from the bar; exit 140 exit |
| Toast in / out | 280 / 180 | enter / exit | slide 24 px from the right + fade; stack reflows 180 |
| Launcher open / close | 180 / 120 | enter / exit | fade + scale .97 -> 1; results do not animate while typing |
| Window open | 220 | enter | scale .96 -> 1 + fade |
| Window close / minimize | 160 / 280 | exit / standard | scale .96 + fade / genie-less shrink to dock icon |
| Maximize / snap | 280 | standard | geometry tween, content not scaled |
| Switcher | 100 | enter | instant selection move, no easing delay |
| Overview | 280-400 | standard | windows tween to grid slots, workspaces slide 280 |
| Workspace switch | 280 | standard | horizontal slide |
| Lock / unlock | 400 / 280 | standard | clock scales .98 and fades; error shakes 4 px x 2 over 200 |

`animation_speed` (Settings > Motion) scales all durations; `reduce_motion` replaces slides, scales and tweens with 80-120 ms fades and removes the dock lift.

## 6. Keyboard-first affordances

- `Super` launcher, `Super+Tab` overview, `Alt+Tab` / `Shift+Alt+Tab` switcher (`Q` closes, `` ` `` cycles windows of the same app), `Alt+1..6` pinned apps, `Ctrl+K` result actions, `Tab` result category, `/` focus settings search, `Esc` always dismisses the topmost layer.
- Every row in lists shows its primary key as a keycap on hover/selection; the launcher footer always lists navigation keys.
- Arrow keys move selection with wraparound; the selection state is visually distinct from hover (accent fill + bar).
- Quick settings tiles are reachable with `Tab`; `Space`/`Enter` toggle, `Right` opens the detail chevron.
- Window actions: `Super+Arrows` snap, `Super+Up` maximize, `Alt+Space` window menu, `Alt+F4` close.

## 7. Accessibility

- **Contrast.** Text tokens meet AA in both themes (verified by the theme crate tests): primary >= 12:1, secondary >= 6:1, disabled/placeholder >= 3:1. White on `#7B61FF` is only 4.2:1, so **text on accent fills uses ink `#0C0A1E` (`--on-accent`, 4.64:1)**; white is allowed only for non-text glyphs (>= 3:1) such as the check inside a swatch or avatar initials.
- **Light-theme status colors** (`success/warning/danger`) are the darkened text-safe variants.
- **Focus.** Visible 2 px ring (`--focus-ring`: `#9C89FF` dark, `#5B3FE6` light) with a 2 px offset, always present for keyboard navigation, never color-only (shape change on selection too).
- **Targets.** Controls are 32 px (40 px in large contexts); window buttons are 32 x 28 hit areas; touch layouts scale by `--control-h`.
- **Color is never the only signal.** Status uses icon + text; the focused window also changes title weight/contrast and shadow.
- **Reduced motion** and **animation speed** are first-class settings (see 5). No flashing content; the only looping animation is the text caret.
- **Text scale.** Layout uses the 4 px grid and flex wrapping; sizes are tokens so a global text-scale setting multiplies them.

## 8. Implementation guide for Blitz and the Rust theme

### Token mapping (`tokens.css` <-> `crates/desilium-theme`)

| CSS custom property | Theme TOML key |
|---|---|
| `--bg` | `palette.background` |
| `--surface-1` | `palette.surface` |
| `--surface-3` | `palette.surface_raised` (also active title bar) |
| `--surface-4` | `palette.overlay` |
| `--text`, `--text-2`, `--text-3` | `text_primary`, `text_secondary`, `text_disabled` |
| `--accent`, `--accent-hover`, `--accent-press` | `accent`, `accent_hover`, `accent_pressed` |
| `--on-accent` | `text_on_accent` |
| `--accent-soft` / `--selected` | `accent_subtle` / `selection` |
| `--success/--warning/--danger` | same names |
| `--border-strong`, `--divider` | `border`, `divider` |
| `--window-border`, `--window-border-inactive` | `window.border_active/inactive` |
| `--titlebar-bg(-inactive)` | `window.title_bar_active/inactive` |
| `--radius-window`, `--radius-control`, `--titlebar-h` | `window.corner_radius`, `control_radius`, `title_bar_height` |
| `--space-*` | `spacing.*` (4 px unit) |
| `--shadow-1`, `--shadow-3`, `--shadow-4` | `shadow.unfocused`, `shadow.focused`, `shadow.popup`/`modal` |

The shell should generate a `:root { --... }` block from the active `Theme` at runtime so user themes and accent changes need no CSS edits. Tokens that exist only in the mockups (`--glass-*`, `--bar-bg`, `--hover`, `--pressed`, `--control-active`, `--surface-0/2/inset`, `--accent-line`, `--focus-ring`, elevation 2) are derived: mix `surface`, `overlay`, `accent` and white/ink at fixed ratios; the theme crate should expose them as derived values.

### Divergences between mockups and the theme crate (resolved)

1. **Window buttons** follow the mockups: neutral, right-aligned 32 x 28 buttons (minimize, maximize/restore, close), only the close hover turns danger red. The theme crate now models exactly this (`[window.buttons]`: width/height, corner radius, spacing, side inset, `alignment = "right" | "left"`, per-state fills, glyph colors, glyph size and stroke).
2. **Title weight** is 500 (Medium) in both the mockups and the theme.
3. **Shadow levels.** Mockups define five levels; the crate defines four named shadows (unfocused, focused, popup, modal). The mapping is in `docs/design.md` section 3; level 2 (cards, menus) has no named token yet and uses `popup`.

### What to avoid (Blitz)

- `backdrop-filter`, `filter: blur()`, `mix-blend-mode`, `clip-path` shapes, `background-clip: text`: not reliable. Frosted glass is faked: `--glass-bg` (80-92% opaque) + `--glass-sheen` gradient + 1 px hairline + top inner highlight. If a real blur is added later, it must be a compositor-side effect behind the shell surface, with the CSS surface staying translucent.
- The wallpaper grain is an inline SVG `feTurbulence`; in production bake grain into the wallpaper PNG.
- Don't rely on CSS animations in the mockups; the motion spec above is implemented by the shell's animation driver with the same token values (ms and cubic-bezier).
- Avoid `position: sticky`, CSS `transform` on large trees (cost), `calc()` nested with `var()` in hot paths, and `:has()`. Prefer explicit state classes (`.hover`, `.sel`, `.on`) which is how the mockups show states.
- Icons: use the SVG sprite as individual `<svg><use href="#i-name"/></svg>`; strokes inherit `currentColor` from CSS, so theming is free. Keep icon sizes 16/20/24 so 1.5 px strokes stay crisp on the pixel grid.
- Fonts: stack `"Inter", "Noto Sans", "Cantarell", "DejaVu Sans", sans-serif`; mono `"JetBrains Mono", "Noto Sans Mono", "DejaVu Sans Mono", monospace`. Use `font-variant-numeric: tabular-nums` for clocks and percentages (falls back gracefully if unsupported).
- Layout: stay with flexbox and simple grids; absolute positioning is used only for floating shell surfaces (bar, dock, panels) and window placement.

### Verified rendering

The pages were rendered headless with Firefox at 1920 x 1080 in both themes (Chromium was installed but broken: missing `libjsoncpp.so.25`). Blitz was not available for testing; expect small differences in text shaping and shadow blur, and test the avoid-list above first.
