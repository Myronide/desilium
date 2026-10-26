# Headless rendering and screenshots

Desilium can run without any display: the compositor renders the desktop into an offscreen GPU buffer (DRM render node, EGL/GLES via GBM) and writes a PNG. This is the visual-verification path for agents, CI and developers.

## Taking a screenshot (recommended: container)

```sh
scripts/docker-screenshot.sh --out screenshots/foot.png \
  --command "foot -e sh -c 'echo Desilium; sleep 5'"
```

- Builds `docker/Dockerfile` (skip with `--no-build`) and runs it with the same hardening as `scripts/docker-run.sh` (read-only rootfs, no capabilities, no network, uid 1000, resource limits, only the render node), but without any Wayland socket.
- Exactly one writable bind mount: the directory of `--out` (default `screenshots/`, created with mode 0777) at `/out`.
- Container name: `desilium-shot`. Without `--command`, `foot` is started.
- Any other arguments are passed to the compositor. Exit status is non-zero if no screenshot was produced.

## Compositor flags

| Flag | Meaning |
| --- | --- |
| `--headless` | Offscreen mode, no window. |
| `--size WxH` | Physical output size, default `1280x800`. |
| `--scale F` | Output scale, default `1.0` (fractional allowed, max 8). |
| `--screenshot FILE.png` | Write a PNG and exit (needs `--headless`). |
| `--screenshot-delay MS` | Quiet time after the last window activity, default `2000`. |
| `--command "prog args"` | Client to start; repeatable. Single and double quotes group words. |

The screenshot is taken once a client window exists and nothing changed for the delay. A hard 20 s timeout guarantees the run ends; if no window was ever shown the picture is still written but the exit code is 1.

## Using it from Rust

`backend::headless::HeadlessGpu` is a stand-alone offscreen renderer, handy for shader and effect tests:

```rust
let mut gpu = HeadlessGpu::new()?;            // first /dev/dri/renderD*
let image = gpu.render_to_image(size, 1.0, &elements, [0.0, 0.0, 0.0, 1.0])?;
image.save_png(Path::new("out.png"))?;
```

`Image { width, height, rgba }` is top-down, straight (non-premultiplied) RGBA8. `gpu.renderer()` exposes the `GlesRenderer`. Tests should skip when `HeadlessGpu::new()` fails (no render node).

## Limits

- Needs a GPU render node (`/dev/dri/renderD*`); software-only hosts without one are not supported.
- No input injection yet: screenshots show what clients draw on their own.
- Timing is wall-clock based; animated clients may need a larger `--screenshot-delay` or may only reach the hard timeout.

## Animation captures

Window animations (open, maximize/snap/minimize, focus cross-fade, hover fades, close) run on the compositor clock. Two development flags make them reproducible in headless runs:

| Flag | Meaning |
| --- | --- |
| `--freeze-time MS[:KINDS]` | Render every animation of the selected kinds exactly `MS` milliseconds after it started and never advance. `KINDS` is a comma list of `open`, `close`, `geometry`, `focus`, `hover` (default: all); unselected kinds finish instantly. |
| `--demo maximize\|close` | Once the first window is drawn, maximize or close the focused window (there is no input injection), so the matching animation can be captured. |

Examples (see `docs/screenshots/motion-*.png`):

```sh
# Window opening, 60 ms into the 240 ms open animation
scripts/docker-screenshot.sh --out docs/screenshots/motion-open-060.png --freeze-time 60:open
# Maximize, halfway through the 320 ms frame transition
scripts/docker-screenshot.sh --out docs/screenshots/motion-maximize-160.png --freeze-time 160:geometry --demo maximize
# Focus cross-fade: the second window opens a second later, so the first one is already settled
scripts/docker-screenshot.sh --out docs/screenshots/motion-focus-025.png --freeze-time 25:focus --screenshot-delay 2500 \
  --command "foot" --command "sh -c 'sleep 1; exec foot -T second'"
# Close, 80 ms into the 160 ms fade of the window's last-frame snapshot
scripts/docker-screenshot.sh --out docs/screenshots/motion-close-080.png --freeze-time 80:close --demo close
```

## Glass (backdrop blur) captures

Popups, translucent layer surfaces and title bars get frosted glass when the surface has see-through pixels (see `render/glass`, `render/popups.rs`). Three environment variables force the effects in headless runs; they are ignored unless the output is the headless one, and `scripts/docker-screenshot.sh` forwards them into the container:

| Variable | Effect |
| --- | --- |
| `DESILIUM_DEBUG_POPUP=1\|0.0-1.0` | Draws a synthetic popup-styled menu over the windows (rounded clip, hairline, theme popup shadow, glass). A fraction replaces the theme's `popup_translucency`. |
| `DESILIUM_DEBUG_GLASS=on\|0.0-1.0` | Forces the title bar translucency (`on` = 0.45) instead of the theme's `titlebar_translucency`. |
| `DESILIUM_DEBUG_LAYER_BLUR=RADIUS[,CORNER]` | Puts backdrop blur behind every layer surface until the shell protocol provides per-surface effects. |

The `layer-client` test client has matching flags: `--glass` (translucent rounded dock and bar, checker-painted window for `--maximized`), `--popup` (plain window with an opaque xdg popup) and `--popup-glass` (the same popup, half transparent).

```sh
# Styled synthetic popup over a terminal (see docs/screenshots/glass-popup-debug-*.png)
DESILIUM_DEBUG_POPUP=1 scripts/docker-screenshot.sh --out docs/screenshots/glass-popup-debug-dark.png --command "foot"
# Real xdg popups from a client (glass-popup-client-*.png)
scripts/docker-screenshot.sh --out docs/screenshots/glass-popup-client-translucent.png --command "layer-client --popup-glass"
# Blurred layer surface over a window (glass-layer-blur.png)
DESILIUM_DEBUG_LAYER_BLUR=24,16 scripts/docker-screenshot.sh --out docs/screenshots/glass-layer-blur.png --command "layer-client --glass --maximized"
# Blurred title bar of a window above another window (glass-titlebar-*.png)
DESILIUM_DEBUG_GLASS=0.5 scripts/docker-screenshot.sh --out docs/screenshots/glass-titlebar-dark.png --screenshot-delay 2500 \
  --command "layer-client --glass --maximized" --command "sh -c 'sleep 1; exec foot -T over-checker'"
```
