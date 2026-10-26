# Blitz feasibility spike — result

**Verdict: GO.** Blitz renders the approved shell mockups almost pixel-identically to Firefox, fully headless, without patches. A full 1080p frame costs about 45-110 ms, a widget-sized surface (480x300) 3.6 ms. Use it behind a narrow `desilium-ui` interface so the backend stays replaceable (`concept.md` §2).

The lead checked two renders by eye (`out/launcher-dark.png`, `out/desktop-dark.png`) against `docs/design/screenshots/`: gradients, radii, borders, shadows, icons and typography match.

## 1. What was built

`blitz-spike` loads an HTML file from disk (linked CSS fetched through a ~15-line synchronous `file://` `NetProvider`), parses, resolves style and layout, paints with `blitz_paint::paint_scene` into an `anyrender` `ImageRenderer` and writes a PNG. No window, surface or display. It compiled on the first try with rustc/cargo 1.99.1 (edition 2024). Cold release build 54 s, ~800 crates, 32 MB binary.

| crate | version |
|---|---|
| blitz-dom / blitz-html / blitz-paint / blitz-traits | `=0.3.0-beta.4` (latest on crates.io, beta) |
| anyrender | 0.14.0 |
| anyrender_vello_cpu (vello_cpu 0.3.0) | 0.18.0 |
| anyrender_vello (wgpu) | 0.15.0 |
| png | 0.17 |

Not used: `blitz-shell` (winit-only, no layer-shell), `blitz-net` (tokio/http), `blitz-renderer-vello` (old alpha, superseded by the anyrender backends).

```
cd spikes/blitz && export CARGO_TARGET_DIR=/tmp/claude-1000/blitz-spike-target
cargo build --release
SPIKE_EXPAND_USE=1 $CARGO_TARGET_DIR/release/blitz-spike ../../docs/design/settings.html out/settings-light.png 1920x1080 light
# args: <html> <png> <WxH> [dark|light] [scale] [accent-hex]
# env:  SPIKE_EXPAND_USE=1 inline the <use> workaround (needed for icons)
#       SPIKE_GPU=1        wgpu/vello instead of vello_cpu
#       SPIKE_HOVER=x,y    hit-test + hover-restyle + repaint benchmark
```

Theme switching rewrites `data-theme` in the source string and sets `Viewport.color_scheme`; Blitz has no JS, so `theme.js` is ignored. (Stale files from earlier runs remain in `out/`; harmless.)

## 2. Fidelity vs. the Firefox screenshots

Pixel diff over all 18 page/theme renders: mean absolute difference 1.0-4.5 of 255; only 0.3-1.5 % of pixels differ by more than 32. Remaining differences: the dev "Toggle theme" button, text weight, 1 px baseline shifts.

| Feature | Status | Workaround |
|---|---|---|
| Inter + fallback stack | OK (system Inter via fontique) | Bundle Inter with `@font-face` for determinism |
| Font weights | 500/600 render visibly bolder than Firefox; 200 correct | Tune weight tokens (500 → ~450) or bundle one variable Inter and re-test |
| Text shaping, letter-spacing, `tabular-nums` | OK (1 px baseline diffs) | none |
| Flexbox, grid, absolute positioning | OK | none |
| CSS variables, `calc()`, `@media`, `:root[data-theme]` | OK | none |
| radius, borders, outlines, (radial/multi-layer) gradients | OK | none |
| `box-shadow` (blur, spread, inset, multiple) | OK; large window shadows slightly weaker | Raise shadow alpha 10-15 % in tokens |
| `opacity`, `rgba()`, pseudo-elements, inline SVG with `currentColor` | OK | none |
| **SVG `<use href="#id">` to a sprite elsewhere in the document** | **BROKEN** (icons vanish: each inline `<svg>` renders in isolation) | Verified: expand each `<use>` into the symbol body; better: emit self-contained inline `<svg viewBox="0 0 24 24">` from a Rust icon table |
| SVG `feTurbulence` grain (data: URI) | Missing | Bake grain into the wallpaper image |
| `backdrop-filter`, `filter: blur`, CSS animations | Not tested (avoided by design) | Compositor-side blur, shell animation driver |
| External `<script>` | Ignored | State comes from Rust |

## 3. Performance (release build, single runs, this machine)

CPU path (`vello_cpu`, multithreading feature not enabled), 1920x1080:

| page | parse+css | style+layout | paint+raster | warm frame |
|---|---|---|---|---|
| lockscreen | 21 ms | 24 ms | 28 ms | 27-30 ms |
| overview | 21 ms | 29 ms | 77 ms | 77 ms |
| quick-settings | 21 ms | 30 ms | 67 ms | 66-68 ms |
| launcher | 22 ms | 38-43 ms | 65-68 ms | 64-67 ms |
| settings | 21 ms | 43 ms | 66 ms | 66 ms |
| desktop | 23 ms | 55 ms | 71 ms | 70 ms |
| notifications | 21 ms | 35 ms | 83-95 ms | 84-92 ms |
| switcher (worst) | 21 ms | 28 ms | 109 ms | 108 ms |

- Cold start (process → PNG) 105-270 ms; peak RSS 41-46 MiB per 1080p CPU page.
- Widget 480x300: 3.6 ms paint, 26 MiB peak RSS. Live accent change at that size 4.9 ms (the render shows the accent pill turned orange).
- GPU path (`SPIKE_GPU=1`, headless wgpu on the render node): first frame 351 ms (device/pipeline creation), warm 45 ms incl. an 8 MB readback, peak RSS 143 MiB; output differs from CPU by mean 0.37/255.
- `doc.hit(x,y)` 3-5 µs; a hover change (restyle + paint) 67 ms (1080p CPU) / 39 ms (GPU incl. readback).
- Not measured: `vello_cpu` multithreading (likely a big win), damage-region repaint (`paint_scene` repaints the whole viewport), power use.

A full-screen 1080p CPU repaint (30-110 ms) is too slow for 60 Hz animation, but real shell surfaces are small (bar 1920x32, dock ~600x70, panels ~420x700) and the widget result suggests they fit the frame budget.

## 4. Integration with a layer-shell client

Blitz is renderer-agnostic: `doc.resolve(t)` then `blitz_paint::paint_scene(&mut impl PaintScene, doc, scale, w, h, x_off, y_off)`.

1. **CPU/shm (simple, robust, works headless):** `VelloCpuImageRenderer::render(draw, &mut [u8])` writes RGBA8 into a caller-provided buffer → mmapped `wl_shm` buffer, attach/damage/commit. Check R/B order against `Argb8888`. Best for bar, dock, notifications, widgets. Dirty-rect clipping must be added by us.
2. **GPU:** `anyrender_vello::VelloWindowRenderer` takes a raw window handle; a `wgpu::Surface` from the layer-shell `wl_display`/`wl_surface` pointers via smithay-client-toolkit should work without winit (not prototyped). Texture route via `VelloImageRenderer` + dmabuf is also possible. Only headless wgpu image rendering is proven.

`blitz-shell` is winit-only, so the sctk glue (layer surface, frame callbacks, pointer/keyboard mapping) is ours (~300 lines, estimate). Events: `UiEvent::{PointerMove, PointerDown, PointerUp, PointerLeave, Wheel, KeyDown, KeyUp, Ime}` + `doc.handle_ui_event()`; `ShellProvider` (request_redraw, set_cursor, clipboard, IME) and `NavigationProvider` are traits we implement. DOM mutation (`doc.mutate().set_attribute/set_inner_html`, `query_selector`) lets Rust state toggle `.on`/`.sel` classes as in the mockups. Live theme/accent change: rewrite the root `style`, resolve, paint (a whole-surface restyle). Animations through per-frame class/custom-property changes cost a full resolve per frame per surface: fine for small surfaces; animate large panels as compositor-side transform/opacity.

## 5. Risks and recommendation

Risks: (1) beta API with mismatched anyrender/vello versions — pin exact versions, wrap behind `desilium-ui`, consider vendoring; (2) weight: ~800 crates, 54 s cold build, 32 MB binary — keep it in shell binaries, never in the compositor; (3) known gaps above, all with workarounds; (4) no exposed damage tracking, whole-tree restyle per mutation; (5) text input/IME unproven on layer-shell (launcher search field needs real testing); (6) unknowns: wgpu surface from layer-shell handles, `vello_cpu` multithreading, memory with ~6 coexisting surfaces (25-45 MiB each on CPU, ~140 MiB per GPU context unless shared).

**Recommendation: use Blitz for the shell; Phase 2 can proceed**, keeping the replaceable-backend rule: the shell talks to an internal surface/widget interface, HTML/CSS is generated from the theme tokens (a `:root` block from `desilium-theme`) plus a Rust icon table. Slint or a custom vello widget kit is not needed now; revisit only if risk 1 or 4 materialises.

Next steps, in order: (1) sctk layer-shell top-bar client (1920x32) rendering via `VelloCpuImageRenderer` into `wl_shm` with pointer hover/click through `handle_ui_event`; measure frame time and idle CPU, then try the wgpu `Surface` route; (2) enable `vello_cpu` multithreading, add damage-clipped repaints; (3) replace the `<use>` sprite with per-icon inline SVG, decide on bundled Inter via `@font-face`, tune weight tokens; (4) real text input (launcher search) with keyboard focus and IME; (5) test CSS transitions/animations and the cost of per-frame class/custom-property animation on the dock; (6) decide the real-blur plan (compositor-side, behind the shell surface — see `render/effects` backdrop blur).
