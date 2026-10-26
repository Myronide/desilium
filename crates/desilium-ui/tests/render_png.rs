//! Widget-sized renders with real tokens and icons: writes PNGs for eyeballing and checks pixels.
//! Needs no display or GPU. Output goes to `$CARGO_TARGET_TMPDIR`; run with `--nocapture` for timings.

use std::{fs, path::PathBuf, time::Instant};

use desilium_theme::Theme;
use desilium_ui::{SurfaceOptions, UiSurface, Updates, app_icon_html, icon_svg};

const SHARED_CSS: &str = include_str!("../examples/shared.css");
const WARM_FRAMES: u32 = 20;

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

fn write_png(name: &str, bgra: &[u8], width: u32, height: u32) {
    let rgba = desilium_ui::bgra_premultiplied_to_rgba(bgra);
    let file = fs::File::create(out_dir().join(name)).unwrap();
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rgba)
        .unwrap();
}

fn pixel(buf: &[u8], stride: usize, x: usize, y: usize) -> [u8; 4] {
    let at = y * stride + x * 4;
    [buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]
}

fn render(surface: &mut UiSurface) -> Vec<u8> {
    let stride = surface.min_stride();
    let mut buf = vec![0; stride * surface.buffer_size().1 as usize];
    surface.render(&mut buf, stride).unwrap();
    buf
}

/// Average warm-frame cost when one text node changes per frame.
fn warm_ms(surface: &mut UiSurface, selector: &str) -> f64 {
    let stride = surface.min_stride();
    let mut buf = vec![0; stride * surface.buffer_size().1 as usize];
    surface.render(&mut buf, stride).unwrap();
    let start = Instant::now();
    for i in 0..WARM_FRAMES {
        surface
            .apply(Updates::new().text(selector, &format!("{:02}:{:02}", i / 60, i % 60)))
            .unwrap();
        surface.render(&mut buf, stride).unwrap();
    }
    start.elapsed().as_secs_f64() * 1000.0 / f64::from(WARM_FRAMES)
}

fn widget_html() -> String {
    let rows: String = [("wifi", "Wi-Fi", "Home network", true), ("bluetooth", "Bluetooth", "Off", false), ("moon", "Night light", "Sunset", false)]
        .iter()
        .map(|(icon, title, sub, on)| {
            format!(
                "<div class='tile{}' data-action=toggle data-arg={icon}>{}<div><div class=t>{title}</div><div class=s>{sub}</div></div></div>",
                if *on { " on" } else { "" },
                icon_svg(icon, 20, "")
            )
        })
        .collect();
    format!(
        "<div class='card glass panel'><div class=head>{}<span id=title>Quick settings</span></div>{rows}</div>",
        app_icon_html("settings", "sm")
    )
}

const WIDGET_CSS: &str = "
.card { width: 460px; margin: 10px; padding: 12px; display: flex; flex-direction: column; gap: 8px; color: var(--text); }
.head { display: flex; align-items: center; gap: 8px; font-weight: var(--fw-semibold); height: 28px; }
.tile { display: flex; align-items: center; gap: 12px; height: 56px; padding: 0 12px; border-radius: var(--radius-lg); background: var(--surface-3); }
.tile:hover { background: var(--surface-4); }
.tile.on { background: var(--accent); color: var(--on-accent); }
.t { font-weight: var(--fw-medium); } .s { font-size: var(--fs-small); opacity: .75; }
";

#[test]
fn widget_480x300_renders_with_tokens_and_icons() {
    let mut s = UiSurface::new(
        &widget_html(),
        &[SHARED_CSS, WIDGET_CSS],
        SurfaceOptions::new(480, 300),
    )
    .unwrap();
    let buf = render(&mut s);
    write_png("ui-widget-dark.png", &buf, 480, 300);
    let stride = s.min_stride();
    assert!(
        pixel(&buf, stride, 0, 0)[3] < 40,
        "only a faint shadow reaches the corner"
    );
    let card = pixel(&buf, stride, 240, 20);
    assert!(card[3] > 200, "glass card is mostly opaque: {card:?}");
    let on_tile = pixel(&buf, stride, 400, 100);
    assert_eq!(
        on_tile,
        [0xFF, 0x61, 0x7B, 0xFF],
        "accent tile is BGRA #7B61FF"
    );
    eprintln!(
        "widget 480x300 warm frame: {:.2} ms",
        warm_ms(&mut s, "#title")
    );

    let mut light = Theme::light();
    light.palette.accent = desilium_theme::Color::rgb(0x00A06A);
    s.set_theme(&light);
    let buf = render(&mut s);
    write_png("ui-widget-light-green.png", &buf, 480, 300);
    assert_eq!(pixel(&buf, stride, 400, 100), [0x6A, 0xA0, 0x00, 0xFF]);
}

#[test]
fn top_bar_1920x32_renders_and_is_cheap() {
    let html = format!(
        "<div class=topbar style='position:relative'><div class=left><div class=tb-btn>{}<span>Desilium</span></div></div>\
         <div class='tb-btn tb-clock' id=clock>14:32</div><div class=right><div class=tb-group>{}{}</div></div></div>",
        icon_svg("search", 16, "s16"),
        icon_svg("wifi", 16, "s16"),
        icon_svg("battery", 16, "s16"),
    );
    let mut s = UiSurface::new(&html, &[SHARED_CSS], SurfaceOptions::new(1920, 32)).unwrap();
    let buf = render(&mut s);
    write_png("ui-bar-dark.png", &buf, 1920, 32);
    let stride = s.min_stride();
    let bar = pixel(&buf, stride, 960, 5);
    assert!(
        bar[3] > 100 && bar[3] < 200,
        "bar background is translucent: {bar:?}"
    );
    eprintln!(
        "bar 1920x32 warm frame: {:.2} ms",
        warm_ms(&mut s, "#clock")
    );
}

#[test]
fn fractional_scale_renders_crisp_buffer_sizes() {
    let mut s = UiSurface::new(
        "<div style='width:10px;height:10px;background:#fff'></div>",
        &["html,body{margin:0}"],
        SurfaceOptions::new(101, 33),
    )
    .unwrap();
    s.set_scale(1.5).unwrap();
    assert_eq!(
        s.buffer_size(),
        (152, 50),
        "round(logical * scale), as wp_fractional_scale requires"
    );
    let buf = render(&mut s);
    assert_eq!(pixel(&buf, s.min_stride(), 14, 14), [255, 255, 255, 255]);
    assert_eq!(pixel(&buf, s.min_stride(), 16, 14), [0, 0, 0, 0]);
}
