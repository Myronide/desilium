//! Regenerates the shipped wallpapers.
//!
//! `cargo run --release -p desilium-wallpaper --example gen_wallpapers -- assets/wallpapers [name ...]`

use desilium_wallpaper::{PRESET_NAMES, Size, preset, render_mesh};
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use std::error::Error;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;

const WIDTH: u32 = 2560;
const HEIGHT: u32 = 1440;
const JPEG_QUALITY: u8 = 90;
/// Fixed seed: the images must come out identical on every run.
const SEED: u64 = 0xD351_1001;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let out_dir = PathBuf::from(args.next().unwrap_or_else(|| "assets/wallpapers".into()));
    let requested: Vec<String> = args.collect();
    fs::create_dir_all(&out_dir)?;
    for name in PRESET_NAMES {
        if !requested.is_empty() && !requested.iter().any(|r| r == name) {
            continue;
        }
        let spec = preset(name).ok_or("preset missing")?;
        let rgba = render_mesh(Size::new(WIDTH, HEIGHT), &spec, SEED);
        let rgb: Vec<u8> = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2]])
            .collect();
        let path = out_dir.join(format!("{name}.jpg"));
        let file = BufWriter::new(File::create(&path)?);
        JpegEncoder::new_with_quality(file, JPEG_QUALITY).write_image(
            &rgb,
            WIDTH,
            HEIGHT,
            ExtendedColorType::Rgb8,
        )?;
        println!("{} ({} bytes)", path.display(), fs::metadata(&path)?.len());
    }
    Ok(())
}
