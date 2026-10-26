# Desilium Design Language

Source of truth in code: `crates/desilium-theme` (typed, validated, serde/TOML). Built-in files: `crates/desilium-theme/themes/desilium-{dark,light}.toml`. Every token below is a user-changeable setting with a default; HTML mockups, shaders and the compositor all read the same values.

## 1. Rationale

Desilium should feel calm, deep and precise: macOS polish, Fluent depth, its own identity.

- **Deep, slightly cool surfaces.** Neutrals lean toward hue 232 (blue-violet). Dark is near-black navy, never pure black, so shadows still read. Light is a cool paper white, never stark.
- **One confident accent: `#7B61FF`.** It is used only for focus, selection, primary actions and the active window outline. Everything else is neutral, so the accent always means something.
- **Depth by layers, not lines.** Surfaces step up in lightness (background < surface < raised < overlay in dark mode) and soft, large, low-opacity shadows separate windows. Borders are 1 px hairlines for definition only.
- **Generous radii.** 12 px windows, 8 px controls: friendly, but concentric (window radius = control radius + 4 px padding).
- **Restrained motion.** Things arrive fast and settle softly (`ease-out-expo`), leave quickly, and never bounce for show.
- **Accessible by construction.** All text pairs reach WCAG AA (4.5:1; disabled text 3:1) in both themes and for every derived accent. Tests enforce it; `Theme::validate` repairs user themes that break it.

Note on the accent: white on `#7B61FF` is only 4.2:1, so text on accent fills is deep ink `#0C0A1E` (4.64:1). `derive_from_accent` picks white, ink or black per accent, whichever reads best.

## 2. Token reference

Units are logical px unless noted. Colors are `#RRGGBB[AA]`.

### Fixed brand decisions

| Token | Value |
|---|---|
| Accent | `#7B61FF` |
| Base grid | 4 px |
| Window corner radius | 12 px |
| Control radius | 8 px |
| Title bar height | 36 px |
| UI font stack | Inter, Noto Sans, Cantarell, DejaVu Sans, sans-serif |
| Default mode | dark |

### Palette (`[palette]`)

| Token | Dark | Light | Use |
|---|---|---|---|
| `background` | `#0E1016` | `#F1F2F8` | desktop base |
| `surface` | `#151821` | `#F8F9FC` | window content, panels |
| `surface_raised` | `#1D212D` | `#FFFFFF` | active title bar, cards, inputs |
| `overlay` | `#262B3A` | `#FFFFFF` | menus, popovers, dialogs |
| `text_primary` | `#ECEEF5` | `#14161F` | main text (>= 12:1) |
| `text_secondary` | `#A3A9BC` | `#545B70` | supporting text (>= 6:1) |
| `text_disabled` | `#7A8199` | `#7C8398` | disabled, placeholder (>= 3:1) |
| `text_on_accent` | `#0C0A1E` | `#0C0A1E` | text on accent fills (4.64:1) |
| `accent` | `#7B61FF` | `#7B61FF` | primary actions, focus |
| `accent_hover` | `#8D77FF` | `#8D77FF` | hover (lighter keeps ink readable) |
| `accent_pressed` | `#6F54F5` | `#6F54F5` | pressed, transient (>= 3:1) |
| `accent_subtle` | `#7B61FF29` | `#7B61FF1F` | selected rows, focus halo |
| `success` | `#4CC38A` | `#167A4E` | positive |
| `warning` | `#F0B54A` | `#8F5A0B` | caution |
| `danger` | `#F2616F` | `#C42B41` | destructive, error |
| `border` | `#2C3244` | `#D8DBE6` | control and window outlines |
| `divider` | `#1F2430` | `#E4E6EE` | hairlines |
| `selection` | `#7B61FF52` | `#7B61FF38` | text selection |

### Window (`[window]`, `[window.buttons]`)

| Token | Default | Notes |
|---|---|---|
| `corner_radius` | 12 | range 0-64 |
| `control_radius` | 8 | range 0-32 |
| `border_width` | 1 | 0 disables |
| `border_active` / `border_inactive` | border mixed 35% toward accent / `divider` | |
| `title_bar_height` | 36 | range 24-64 |
| `title_bar_active` / `inactive` | `surface_raised` / `surface` | opaque; see translucency |
| `title_text_active` / `inactive` | `text_primary` / `text_secondary` | |
| `title_font_size` / `weight` | 13 / 500 | weight 100-900; the renderer snaps to 400, 500 or 700 |
| `buttons.width` / `height` | 32 / 28 | hit area; height must fit the bar (bar - 8) |
| `buttons.corner_radius` / `spacing` / `side_inset` | 8 / 2 / 4 | spacing is the gap between buttons |
| `buttons.alignment` | `right` | `left` is allowed, for a user setting later |
| `buttons.rest_fill` | transparent | normal buttons are neutral at rest |
| `buttons.hover_fill` / `pressed_fill` | `text_primary` at 6% / 10% (dark), 5% / 9% (light) | derived from the palette |
| `buttons.close_hover_fill` / `close_pressed_fill` | `danger` / `danger` darkened 8% | the only coloured button |
| `buttons.glyph_active` / `glyph_inactive` | `text_secondary` / `text_disabled` | 3:1 minimum on the title bar |
| `buttons.glyph_on_danger` | `text_on_accent` if >= 4.5:1 on danger, else white or black | dark `#0C0A1E`, light `#FFFFFF` |
| `buttons.glyph_size` / `glyph_stroke` | 16 / 1.5 | glyph box is centred in the hit area; size <= min(width, height) |

Window buttons are neutral and right-aligned, as in the approved mockups: minimize, maximize/restore, close. Only close turns danger on hover and press. The compositor draws server-side decorations from these tokens and hit-tests each button as its `width` x `height` rectangle.

### Spacing (`[spacing]`, always snapped to `unit` = 4)

| xxs | xs | sm | md | lg | xl | xxl | xxxl |
|---|---|---|---|---|---|---|---|
| 4 | 8 | 12 | 16 | 24 | 32 | 48 | 64 |

### Typography (`[typography]`)

| Size | caption | small | body | subtitle | title | headline | display |
|---|---|---|---|---|---|---|---|
| px | 11 | 12 | 13 | 15 | 18 | 24 | 32 |

Weights: regular 400, medium 500, semibold 600, bold 700. Line heights: tight 1.2 (titles, controls), normal 1.45 (body), relaxed 1.65 (long reading). Monospace stack: JetBrains Mono, Noto Sans Mono, DejaVu Sans Mono, monospace. `Typography::ui_css_stack()` yields the CSS value for mockups.

### Motion (`[motion]`)

| Event | ms | Easing |
|---|---|---|
| fast (hover, press) | 120 | `standard` = ease-in-out-cubic |
| normal | 200 | `standard` |
| slow | 320 | `standard` |
| window open | 240 | `enter` = ease-out-expo |
| window close | 160 | `exit` = ease-in-cubic |
| workspace switch | 360 | ease-out-expo |

Spring (drags, snapping, interruptible): stiffness 320, damping 28, mass 1 (damping ratio about 0.78, a single faint overshoot). Easings available: `linear`, `ease-out-cubic`, `ease-in-cubic`, `ease-in-out-cubic`, `ease-out-expo`, `ease-in-out-quad`. `reduce_motion = true` collapses every duration to 0.

### Background and effects

| Token | Dark | Light |
|---|---|---|
| gradient angle (CSS, 0 = up) | 160 | 160 |
| stop 0 / 0.55 / 1 | `#0A0817` `#15102D` `#31184E` | `#F4F2FD` `#E4E0FB` `#C8CFF9` |
| `noise` (grain, 0-0.25) | 0.025 | 0.025 |

Gradient stops are the accent hue at low (dark) or high (light) lightness, with the last stop turned +18 / -18 degrees for gentle depth. Grain hides 8-bit banding.

| Effect | Default | Meaning |
|---|---|---|
| `blur_radius` | 24 | backdrop blur, px (0-128) |
| `backdrop_saturation` | 1.35 | multiplier behind translucent surfaces (0-3) |
| `panel_translucency` | 0.28 dark / 0.22 light | 0 = opaque, 1 = see-through |
| `titlebar_translucency` | 0.12 | |
| `popup_translucency` | 0.18 | |

Contrast guarantees are computed on the opaque colors. Keep translucency modest (<= 0.35) so text stays readable over any wallpaper.

## 3. Elevation model

Shadows are tinted `#0A0C1F` (not pure black), large and soft; dark mode uses roughly 3x the opacity of light mode so depth stays visible.

| Level | Used for | Blur | Spread | Offset Y | Dark alpha | Light alpha |
|---|---|---|---|---|---|---|
| 0 | desktop, panels flush with edge | 0 | 0 | 0 | none | none |
| 1 | unfocused windows (`shadow.unfocused`) | 24 | 0 | 6 | 0.35 | 0.12 |
| 2 | focused window (`shadow.focused`) | 48 | -4 | 16 | 0.55 | 0.19 |
| 3 | menus, popovers, tooltips (`shadow.popup`) | 56 | -4 | 20 | 0.60 | 0.20 |
| 4 | modal dialogs, overlays (`shadow.modal`) | 96 | -10 | 32 | 0.65 | 0.22 |

Rules: negative spread keeps the shadow from showing at the sides (light comes from above); offset X is 0. Focus change animates level 1 to 2 over `normal`. Use `Shadow::level(n)` and `ShadowStyle::extent()` (reach beyond the window, for damage tracking).

### Mapping the mockup elevation levels (0-4)

The approved mockups (`docs/design/tokens.css`, `--shadow-0` to `--shadow-4`) use their own numbering. The crate's `Shadow::level(n)` uses the table above, so the two numberings differ. Code must map by use, not by number:

| Mockup level | Used for | Crate token | `Shadow::level(n)` |
|---|---|---|---|
| 0 | flat, in-window; tiled, fullscreen and low-power windows | none (`ShadowStyle::NONE`) | 0 |
| 1 | inactive window | `shadow.unfocused` | 1 |
| 2 | cards, menus | no named token yet; use `shadow.popup` for menus until a lighter token is added | 3 (approximation) |
| 3 | active window, dock | `shadow.focused` | 2 |
| 4 | launcher, dialogs | `shadow.modal` | 4 |

Level 2 of the mockups is lighter than `popup`; deriving it as a lighter focused shadow is the planned follow-up, not part of the current crate.

## 4. Motion principles

1. **Fast in, soft landing.** Entering uses `ease-out-expo`; 80% of movement happens in the first third.
2. **Leaving is shorter than arriving** (160 vs 240 ms) and uses `ease-in`. Closing never makes the user wait.
3. **Animate what changed, nothing else.** Transform and opacity only; never layout on the hot path.
4. **Small is fast.** Hover and press feedback are 120 ms; whole-screen changes at most 360 ms.
5. **Interruptible.** User-driven motion (drag, snap) uses the spring so reversal is continuous.
6. **Open: scale 0.96 to 1 plus fade; close: scale 1 to 0.97 plus fade.** Subtle, never from a point.
7. **Respect `reduce_motion`:** instant state changes, no exceptions.

## 5. Window chrome mockups

Active (focused: raised bar, accent-tinted outline, `shadow.focused`; glyphs at `glyph_active`):

```
 ╭──────────────────────────────────────────────╮
 │ [icon] Files  Documents        [–][□][×]     │  36px, surface_raised, title 13/500
 ├──────────────────────────────────────────────┤  1px divider
 │                                              │
 │   surface content                            │
 │                                              │
 ╰──────────────────────────────────────────────╯
  neutral buttons 32 x 28, right-aligned, gap 2, inset 4
  close alone turns danger on hover (white or ink glyph)
  radius 12, outline: border -> accent 35%, elevation 3 (mockup)
```

Inactive (unfocused: flat bar, dimmed title and glyphs, quiet outline, `shadow.unfocused`):

```
 ╭──────────────────────────────────────────────╮
 │ icon Files  Documents          –  □  ×       │  surface bar, glyph_inactive
 ├──────────────────────────────────────────────┤
 │   surface content                            │
 ╰──────────────────────────────────────────────╯
  no button fills; outline = divider
```

Snapped (half-tiled: outer corners square at screen edges, 8 px gap, radius kept on inner corners):

```
 ┌──────────────────────┐ ┌──────────────────────╮
 │ Terminal     [–][□][×]│ │ Browser              │
 ├──────────────────────┤ ├──────────────────────┤
 │                      │ │                      │
 └──────────────────────┘ ╰──────────────────────╯
  edge-touching corners: radius 0     gap = spacing.xs (8)
```

## 6. Do and don't

**Do**
- Read every color, radius, duration and font from the theme; never hard-code.
- Use `accent` only for focus, selection, primary action and the active outline.
- Express spacing as multiples of 4 (use the named steps).
- Pair text with its intended surface (`text_on_accent` on accent fills only).
- Run user-supplied themes through `Theme::validate` and show the returned issues in the settings GUI.
- Animate with the theme's durations and easings; honor `reduce_motion`.
- Derive custom looks with `Theme::derive_from_accent` instead of picking colors by hand.

**Don't**
- Use pure black or pure white as a surface, or white text on the accent.
- Put several accent-colored elements next to each other; one accent focus per view.
- Use hard, small, dark shadows or borders thicker than 2 px.
- Differentiate state by color alone (focus also changes outline and shadow).
- Animate longer than 400 ms, bounce, or loop decorative animation.
- Let text sit on translucent surfaces over a busy wallpaper without checking blur and translucency.
- Assume a theme file is valid: partial documents are fine, invalid values are repaired and reported.
