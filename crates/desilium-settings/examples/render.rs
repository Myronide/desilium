//! Renders the settings UI to a PNG without a display, against a throwaway config:
//! `render --out shot.png [--page appearance] [--search text] [--theme dark|light]
//!  [--accent #RRGGBB] [--wallpaper NAME] [--size 1240x800] [--scale 1] [--lang en|de]
//!  [--do ACTION[:ARG]]...`
//!
//! Without `--lang` the language follows the environment, like the app.

use std::{path::PathBuf, process::ExitCode, time::Instant};

use desilium_config::ConfigStore;
use desilium_settings::{
    app::{App, AppOptions},
    i18n::{Locale, set_locale},
    registry::Page,
    runtime::cli::split_action,
    surface::build_surface,
    wallpapers::Env,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("render: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mut out = PathBuf::from("settings.png");
    let mut page = Page::Appearance;
    let mut query = None;
    let mut theme = "dark".to_owned();
    let mut accent = None;
    let mut wallpaper = None;
    let mut size = (1240, 800);
    let mut scale = 1.0;
    let mut actions: Vec<String> = Vec::new();
    while let Some(flag) = args.next() {
        let mut value = || args.next().ok_or(format!("{flag} needs a value"));
        match flag.as_str() {
            "--out" => out = PathBuf::from(value()?),
            "--page" => page = Page::from_slug(&value()?).ok_or("unknown page")?,
            "--search" => query = Some(value()?),
            "--theme" => theme = value()?,
            "--accent" => accent = Some(value()?),
            "--wallpaper" => wallpaper = Some(value()?),
            "--do" => actions.push(value()?),
            "--lang" => {
                let lang = match value()?.as_str() {
                    "en" => Locale::English,
                    "de" => Locale::German,
                    _ => return Err("--lang is en or de".to_owned()),
                };
                set_locale(Some(lang));
            }
            "--scale" => scale = value()?.parse().map_err(|_| "bad scale")?,
            "--size" => {
                let text = value()?;
                let (w, h) = text.split_once('x').ok_or("size is WxH")?;
                size = (
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
            }
            other => return Err(format!("unknown flag {other}")),
        }
    }
    let dir = std::env::temp_dir().join(format!("desilium-settings-render-{}", std::process::id()));
    let walls = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/wallpapers");
    let mut store = ConfigStore::open(dir.join("config")).store;
    let mut tx = store.transaction();
    tx.edit(|config| {
        config.appearance.theme = format!("desilium-{theme}");
        config.appearance.accent = accent;
        config.appearance.wallpaper =
            wallpaper.map(|name| walls.join(format!("{name}.jpg")).display().to_string());
    });
    tx.commit().map_err(|e| e.to_string())?;
    let mut options = AppOptions::from_environment(dir.join("config"));
    options.page = page;
    options.query = query;
    options.cache_dir = dir.join("cache");
    options.wallpaper_env = Env {
        override_dir: Some(walls.into_os_string()),
        ..Env::default()
    };
    let mut app = App::new(store, options, None);
    let now = Instant::now();
    for text in &actions {
        let (name, arg) = split_action(text);
        let event = desilium_ui::UiEvent::Action(desilium_ui::UiAction {
            action: name.to_owned(),
            arg: arg.map(str::to_owned),
            element_id: None,
            rect: desilium_ui::Rect::default(),
        });
        app.handle_event(&event, now);
    }
    let mut ui = build_surface(&mut app, size, scale, now).map_err(|e| e.to_string())?;
    let stride = ui.min_stride();
    let (width, height) = ui.buffer_size();
    let mut buffer = vec![0; stride * height as usize];
    ui.render(&mut buffer, stride).map_err(|e| e.to_string())?;
    ui.mark_dirty();
    ui.render(&mut buffer, stride).map_err(|e| e.to_string())?;
    let rgba = desilium_ui::bgra_premultiplied_to_rgba(&buffer);
    let file = std::fs::File::create(&out).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&rgba))
        .map_err(|e| e.to_string())?;
    println!("wrote {} ({width}x{height})", out.display());
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
