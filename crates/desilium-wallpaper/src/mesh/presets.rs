//! The shipped wallpapers: five calm, deep palettes that sit well behind the Desilium UI.

use super::{Blob, MeshSpec};

/// Names of the shipped wallpapers, in display order.
pub const PRESET_NAMES: [&str; 5] = ["dusk-violet", "aurora", "ember", "slate", "fjord"];

const GRAIN: f32 = 1.5;
/// Global tuning: keeps the glows deep and calm so windows and text stay readable on top.
const STRENGTH_SCALE: f32 = 0.62;
const RADIUS_SCALE: f32 = 0.72;

const fn blob(
    center: (f32, f32),
    radius: f32,
    stretch: f32,
    angle_deg: f32,
    color: [u8; 3],
    strength: f32,
) -> Blob {
    Blob {
        center,
        radius: radius * RADIUS_SCALE,
        stretch,
        angle_deg,
        color,
        strength: strength * STRENGTH_SCALE,
    }
}

/// Returns the recipe for a shipped wallpaper, or `None` for an unknown name.
pub fn preset(name: &str) -> Option<MeshSpec> {
    let (base, blobs, warp, vignette) = match name {
        "dusk-violet" => dusk_violet(),
        "aurora" => aurora(),
        "ember" => ember(),
        "slate" => slate(),
        "fjord" => fjord(),
        _ => return None,
    };
    Some(MeshSpec {
        base,
        blobs,
        warp,
        vignette,
        grain: GRAIN,
    })
}

type Parts = ([[u8; 3]; 2], Vec<Blob>, f32, f32);

fn dusk_violet() -> Parts {
    (
        [[0x0c, 0x0a, 0x1c], [0x08, 0x10, 0x20]],
        vec![
            blob((0.08, 0.10), 0.78, 1.5, 25.0, [0x7b, 0x61, 0xff], 0.85),
            blob((0.30, 0.02), 0.45, 1.2, 0.0, [0xa0, 0x7c, 0xff], 0.45),
            blob((0.92, 0.92), 0.62, 1.4, -20.0, [0x24, 0x6b, 0xd8], 0.65),
            blob((0.55, 1.05), 0.55, 1.8, 0.0, [0xb0, 0x3a, 0x8c], 0.40),
        ],
        0.16,
        0.35,
    )
}

fn aurora() -> Parts {
    (
        [[0x06, 0x0c, 0x18], [0x07, 0x10, 0x1c]],
        vec![
            blob((0.72, 0.22), 0.60, 1.9, 20.0, [0x1f, 0xd1, 0xb6], 0.75),
            blob((0.50, 0.12), 0.40, 2.4, 8.0, [0x4a, 0xe0, 0x9c], 0.35),
            blob((0.14, 0.88), 0.55, 1.5, 30.0, [0x2a, 0x5b, 0xe8], 0.70),
            blob((0.95, 0.75), 0.45, 1.3, 0.0, [0x5a, 0x4a, 0xd8], 0.45),
        ],
        0.20,
        0.38,
    )
}

fn ember() -> Parts {
    (
        [[0x1a, 0x0a, 0x1a], [0x14, 0x08, 0x16]],
        vec![
            blob((0.22, 0.20), 0.75, 1.6, 15.0, [0xff, 0x7a, 0x45], 0.85),
            blob((0.55, 0.45), 0.50, 1.3, -10.0, [0xff, 0x4f, 0x7a], 0.60),
            blob((0.92, 0.95), 0.62, 1.5, -25.0, [0x9a, 0x2f, 0xc4], 0.65),
            blob((0.80, 0.05), 0.40, 1.2, 0.0, [0xff, 0xa0, 0x5a], 0.30),
        ],
        0.14,
        0.32,
    )
}

fn slate() -> Parts {
    (
        [[0x10, 0x13, 0x1a], [0x0b, 0x0d, 0x13]],
        vec![
            blob((0.25, 0.15), 0.70, 1.7, 20.0, [0x4a, 0x56, 0x72], 0.55),
            blob((0.85, 0.80), 0.60, 1.4, -15.0, [0x3a, 0x44, 0x5e], 0.50),
            blob((0.60, 0.50), 0.35, 1.2, 0.0, [0x5a, 0x60, 0x8a], 0.18),
        ],
        0.12,
        0.40,
    )
}

fn fjord() -> Parts {
    (
        [[0x05, 0x14, 0x14], [0x06, 0x10, 0x14]],
        vec![
            blob((0.30, 0.18), 0.65, 1.8, 15.0, [0x3a, 0xc0, 0x8a], 0.70),
            blob((0.12, 0.55), 0.40, 1.4, 0.0, [0x2a, 0x9a, 0x8a], 0.40),
            blob((0.88, 0.85), 0.60, 1.5, -20.0, [0x1a, 0x5a, 0x7a], 0.65),
            blob((0.62, 0.10), 0.35, 1.6, 10.0, [0x7a, 0xd8, 0xa0], 0.25),
        ],
        0.18,
        0.36,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_resolves() {
        for name in PRESET_NAMES {
            assert!(preset(name).is_some(), "{name}");
        }
        assert!(preset("nope").is_none());
    }
}
