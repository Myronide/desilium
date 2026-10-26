# Default wallpapers

Procedural "mesh gradient" images, 2560x1440 JPEG (quality 90, about 150 KB each):

| File | Mood |
| --- | --- |
| `dusk-violet.jpg` | violet `#7B61FF` glow, blue and magenta accents |
| `aurora.jpg` | teal and green sky over deep blue |
| `ember.jpg` | warm orange and pink fading into purple |
| `slate.jpg` | quiet blue-grey |
| `fjord.jpg` | forest green into deep sea blue |

They are generated, not painted, and are byte-for-byte reproducible (fixed seed):

```sh
cargo run --release -p desilium-wallpaper --example gen_wallpapers -- assets/wallpapers
```

Recipes live in `crates/desilium-wallpaper/src/mesh/presets.rs`. Pass preset names after the
output directory to regenerate only some of them.
