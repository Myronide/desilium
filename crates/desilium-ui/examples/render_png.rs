//! HTML file to PNG, headless: `render_png <file.html> <out.png> <WxH> [dark|light] [scale]`.
//!
//! Sprite `<use>` references of the design mockups are inlined first, so the
//! built pages in `docs/design/*.html` render unmodified. Linked stylesheets are
//! resolved relative to the HTML file.

use std::{fs, path::PathBuf, process::ExitCode};

use desilium_theme::Theme;
use desilium_ui::{SurfaceOptions, UiSurface, inline_sprite_uses};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("render_png: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [html_path, out, size, rest @ ..] = args.as_slice() else {
        return Err("usage: render_png <file.html> <out.png> <WxH> [dark|light] [scale]".into());
    };
    let (width, height) = size
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
        .ok_or("size must look like 1920x1080")?;
    let theme = match rest.first().map(String::as_str) {
        Some("light") => Theme::light(),
        _ => Theme::dark(),
    };
    let scale = rest.get(1).and_then(|s| s.parse().ok()).unwrap_or(1.0);

    let path = PathBuf::from(html_path);
    let html = inline_sprite_uses(&fs::read_to_string(&path).map_err(|e| e.to_string())?);
    let mut options = SurfaceOptions::new(width, height);
    options.theme = theme;
    options.scale = scale;
    options.base_dir = path.parent().map(PathBuf::from);
    let mut surface = UiSurface::new(&html, &[], options).map_err(|e| e.to_string())?;

    let stride = surface.min_stride();
    let (pw, ph) = surface.buffer_size();
    let mut buffer = vec![0; stride * ph as usize];
    let stats = surface
        .render(&mut buffer, stride)
        .map_err(|e| e.to_string())?;
    let stats2 = {
        surface.mark_dirty();
        surface
            .render(&mut buffer, stride)
            .map_err(|e| e.to_string())?
    };
    println!(
        "{pw}x{ph}: first frame resolve {:?} paint {:?} convert {:?}; warm frame total {:?}",
        stats.resolve,
        stats.paint,
        stats.convert,
        stats2.total()
    );
    write_png(out, &buffer, pw, ph)
}

/// Writes the premultiplied BGRA buffer as straight-alpha RGBA PNG.
fn write_png(out: &str, bgra: &[u8], width: u32, height: u32) -> Result<(), String> {
    let rgba = desilium_ui::bgra_premultiplied_to_rgba(bgra);
    let file = fs::File::create(out).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgba).map_err(|e| e.to_string())
}
