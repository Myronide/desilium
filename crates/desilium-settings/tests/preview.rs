//! A wallpaper chosen after the window is up must show in the live preview, even though the
//! thumbnail is a file that appears later and the markup is swapped in afterwards.

use std::{fs, time::Instant};

use desilium_config::ConfigStore;
use desilium_settings::{
    app::{App, AppOptions},
    surface::build_surface,
    wallpapers::Env,
};
use desilium_ui::{Rect, UiAction, UiEvent};

const SIZE: (u32, u32) = (1280, 900);
const COLUMN_START: usize = 880;

fn write_red_png(path: &std::path::Path) {
    let file = fs::File::create(path).expect("create");
    let mut encoder = png::Encoder::new(file, 64, 36);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let pixels: Vec<u8> = (0..64 * 36).flat_map(|_| [220, 20, 20, 255]).collect();
    encoder
        .write_header()
        .expect("header")
        .write_image_data(&pixels)
        .expect("data");
}

fn red_pixels_right_of(buffer: &[u8], stride: usize, x_from: usize) -> usize {
    buffer
        .chunks_exact(stride)
        .flat_map(|row| row[x_from * 4..].as_chunks::<4>().0.iter())
        // BGRA
        .filter(|px| px[2] > 150 && px[1] < 80 && px[0] < 80)
        .count()
}

#[test]
fn choosing_a_wallpaper_shows_it_in_the_preview() {
    let dir = tempfile::tempdir().expect("dir");
    let walls = dir.path().join("walls");
    fs::create_dir_all(&walls).expect("mkdir");
    let red = walls.join("red.png");
    write_red_png(&red);
    let config_dir = dir.path().join("config");
    let mut options = AppOptions::from_environment(config_dir.clone());
    options.cache_dir = dir.path().join("cache");
    options.wallpaper_env = Env {
        override_dir: Some(walls.into_os_string()),
        ..Env::default()
    };
    let mut app = App::new(ConfigStore::open(&config_dir).store, options, None);
    let now = Instant::now();
    let mut ui = build_surface(&mut app, SIZE, 1.0, now).expect("surface");
    let stride = ui.min_stride();
    let mut buffer = vec![0; stride * ui.buffer_size().1 as usize];
    ui.render(&mut buffer, stride).expect("render");
    assert_eq!(
        red_pixels_right_of(&buffer, stride, COLUMN_START),
        0,
        "no wallpaper yet"
    );

    let event = UiEvent::Action(UiAction {
        action: "set".into(),
        arg: Some(format!("appearance.wallpaper={}", red.display())),
        element_id: None,
        rect: Rect::default(),
    });
    let refresh = app.handle_event(&event, now);
    refresh.apply(&app, &mut ui, now).expect("apply");
    ui.render(&mut buffer, stride).expect("render");
    let red = red_pixels_right_of(&buffer, stride, COLUMN_START);
    assert!(
        red > 10_000,
        "the preview is filled with the wallpaper, saw {red} red pixels"
    );
}
