//! Seeded hash noise: smooth value noise for domain warping and white noise for film grain.

const MIX_A: u64 = 0xff51_afd7_ed55_8ccd;
const MIX_B: u64 = 0xc4ce_b9fe_1a85_ec53;
const MIX_SHIFT: u32 = 33;
const LATTICE_X: u64 = 0x9e37_79b9_7f4a_7c15;
const LATTICE_Y: u64 = 0xc2b2_ae3d_27d4_eb4f;
/// Bits of the 64-bit hash that feed the unit value (the f32 mantissa width plus one).
const UNIT_BITS: u32 = 24;
const UNIT_SCALE: f32 = 1.0 / (1u32 << UNIT_BITS) as f32;
const OCTAVES: u32 = 3;
const OCTAVE_GAIN: f32 = 0.5;
const OCTAVE_LACUNARITY: f32 = 2.0;
/// Largest possible fbm sum for `OCTAVES` octaves of gain `OCTAVE_GAIN` (1 + 1/2 + 1/4).
const FBM_NORM: f32 = 1.75;

/// 64-bit finalizer (murmur3 style): every input bit affects every output bit.
fn mix(mut value: u64) -> u64 {
    value ^= value >> MIX_SHIFT;
    value = value.wrapping_mul(MIX_A);
    value ^= value >> MIX_SHIFT;
    value = value.wrapping_mul(MIX_B);
    value ^ (value >> MIX_SHIFT)
}

fn lattice(seed: u64, x: i64, y: i64) -> f32 {
    let key = seed
        .wrapping_add((x as u64).wrapping_mul(LATTICE_X))
        .wrapping_add((y as u64).wrapping_mul(LATTICE_Y));
    ((mix(key) >> (u64::BITS - UNIT_BITS)) as f32) * UNIT_SCALE
}

/// Uniform white noise in `0.0..1.0` for integer coordinates and a channel salt.
pub(super) fn white(seed: u64, x: usize, y: usize, salt: u64) -> f32 {
    lattice(seed ^ mix(salt), x as i64, y as i64)
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(seed: u64, x: f32, y: f32) -> f32 {
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (smooth(x - fx), smooth(y - fy));
    let (ix, iy) = (fx as i64, fy as i64);
    let top = lattice(seed, ix, iy) * (1.0 - tx) + lattice(seed, ix + 1, iy) * tx;
    let bottom = lattice(seed, ix, iy + 1) * (1.0 - tx) + lattice(seed, ix + 1, iy + 1) * tx;
    top * (1.0 - ty) + bottom * ty
}

/// Fractal noise in `0.0..1.0`, smooth at the scale of `frequency`.
pub(super) fn fbm(seed: u64, x: f32, y: f32, frequency: f32) -> f32 {
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut scale = frequency;
    for octave in 0..OCTAVES {
        let octave_seed = seed.wrapping_add(u64::from(octave));
        total += amplitude * value_noise(octave_seed, x * scale, y * scale);
        amplitude *= OCTAVE_GAIN;
        scale *= OCTAVE_LACUNARITY;
    }
    total / FBM_NORM
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_noise_is_in_unit_range_and_seeded() {
        let a = white(1, 10, 20, 0);
        assert!((0.0..1.0).contains(&a));
        assert_eq!(a, white(1, 10, 20, 0));
        assert_ne!(a, white(2, 10, 20, 0));
    }

    #[test]
    fn fbm_is_smooth_and_bounded() {
        let a = fbm(7, 0.5, 0.5, 2.0);
        let b = fbm(7, 0.501, 0.5, 2.0);
        assert!((0.0..=1.0).contains(&a));
        assert!((a - b).abs() < 0.01);
    }
}
