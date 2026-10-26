//! `<img>` with `file://` URLs: PNG and SVG icons must render, also when inserted after creation.

use std::path::Path;

use desilium_ui::{SurfaceOptions, UiSurface};

fn write_png(path: &Path, rgba: [u8; 4]) {
    let file = std::fs::File::create(path).unwrap();
    let mut encoder = png::Encoder::new(file, 8, 8);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = rgba.iter().copied().cycle().take(8 * 8 * 4).collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&data)
        .unwrap();
}

fn pixel(surface: &mut UiSurface, x: usize, y: usize) -> [u8; 4] {
    let stride = surface.min_stride();
    let mut buf = vec![0; stride * surface.buffer_size().1 as usize];
    surface.render(&mut buf, stride).unwrap();
    let at = y * stride + x * 4;
    [buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]
}

const CSS: &str = "html,body{margin:0} img{position:absolute;left:0;top:0;width:32px;height:32px}";

#[test]
fn png_and_svg_files_render_in_img() {
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("red.png");
    write_png(&png_path, [255, 0, 0, 255]);
    let svg_path = dir.path().join("green.svg");
    std::fs::write(
        &svg_path,
        "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='16'><rect width='16' height='16' fill='#00ff00'/></svg>",
    )
    .unwrap();

    let html = format!("<img src='file://{}'>", png_path.display());
    let mut surface = UiSurface::new(&html, &[CSS], SurfaceOptions::new(32, 32)).unwrap();
    assert_eq!(pixel(&mut surface, 16, 16), [0, 0, 255, 255], "png (bgra)");

    let html = format!("<img src='file://{}'>", svg_path.display());
    let mut surface = UiSurface::new(&html, &[CSS], SurfaceOptions::new(32, 32)).unwrap();
    assert_eq!(pixel(&mut surface, 16, 16), [0, 255, 0, 255], "svg");
}

#[test]
fn images_inserted_later_render() {
    let dir = tempfile::tempdir().unwrap();
    let png_path = dir.path().join("blue.png");
    write_png(&png_path, [0, 0, 255, 255]);
    let mut surface =
        UiSurface::new("<div id=slot></div>", &[CSS], SurfaceOptions::new(32, 32)).unwrap();
    surface
        .replace_children(
            "#slot",
            &format!("<img src='file://{}'>", png_path.display()),
        )
        .unwrap();
    assert_eq!(
        pixel(&mut surface, 16, 16),
        [255, 0, 0, 255],
        "png inserted via DOM update"
    );
}
