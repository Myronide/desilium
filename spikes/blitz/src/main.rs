//! Offscreen Blitz render spike: HTML file -> PNG via the CPU vello renderer.
use std::{fs, path::PathBuf, sync::Arc, time::Instant};

use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{DocumentConfig, QualName, local_name, ns};
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::{Bytes, NetHandler, NetProvider, Request},
    shell::{ColorScheme, Viewport},
};

/// Serves `file://` URLs synchronously from disk.
struct FileProvider;

impl NetProvider for FileProvider {
    fn fetch(&self, _doc_id: usize, request: Request, handler: Box<dyn NetHandler>) {
        let Ok(path) = request.url.to_file_path() else {
            eprintln!("unsupported url {}", request.url);
            return;
        };
        match fs::read(&path) {
            Ok(bytes) => handler.bytes(request.url.to_string(), Bytes::from(bytes)),
            Err(err) => eprintln!("read {}: {err}", path.display()),
        }
    }
}

/// Workaround for Blitz: inline `<svg><use href="#id"/></svg>` cannot see symbols defined elsewhere
/// in the document, so each `<use>` is replaced by the symbol body before parsing.
fn expand_use(html: &str) -> String {
    let mut out = html.to_string();
    let marker = "<use href=\"#";
    while let Some(pos) = out.find(marker) {
        let rest = &out[pos + marker.len()..];
        let Some(end_name) = rest.find('"') else { break };
        let name = rest[..end_name].to_string();
        let Some(end_tag) = rest.find("/>") else { break };
        let tag_end = pos + marker.len() + end_tag + 2;
        let sym_open = format!("<symbol id=\"{name}\"");
        let body = out.find(&sym_open).and_then(|s| {
            let from = s + out[s..].find('>')? + 1;
            let to = from + out[from..].find("</symbol>")?;
            Some(out[from..to].to_string())
        });
        let group = format!(
            "<g fill=\"none\" stroke=\"currentColor\" stroke-width=\"1.5\" stroke-linecap=\"round\" stroke-linejoin=\"round\">{}</g>",
            body.unwrap_or_default()
        );
        out.replace_range(pos..tag_end, &group);
    }
    out.replace("<svg class=\"ic", "<svg viewBox=\"0 0 24 24\" class=\"ic")
}

fn rss_mib() -> f64 {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kib = status
        .lines()
        .find(|l| l.starts_with("VmHWM"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(0.0);
    kib / 1024.0
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn render_with<R: ImageRenderer>(doc: &mut HtmlDocument, w: u32, h: u32, scale: f64) -> Vec<u8> {
    let mut renderer = R::new(w, h);
    let mut buf = Vec::new();
    renderer.render_to_vec(
        |scene| blitz_paint::paint_scene(scene, doc, scale, w, h, 0, 0),
        &mut buf,
    );
    buf
}

/// CPU (vello_cpu) by default; wgpu (vello) when `SPIKE_GPU` is set.
fn render(doc: &mut HtmlDocument, w: u32, h: u32, scale: f64) -> Vec<u8> {
    if std::env::var_os("SPIKE_GPU").is_some() {
        return render_with::<anyrender_vello::VelloImageRenderer>(doc, w, h, scale);
    }
    render_with::<VelloCpuImageRenderer>(doc, w, h, scale)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: blitz-spike <file.html> <out.png> <WxH> [dark|light] [scale] [accent-hex]");
        std::process::exit(2);
    }
    let path = fs::canonicalize(&args[1]).expect("html path");
    let (w, h) = args[3].split_once('x').expect("WxH");
    let (w, h): (u32, u32) = (w.parse().expect("w"), h.parse().expect("h"));
    let theme = args.get(4).map_or("dark", String::as_str);
    let scale: f64 = args.get(5).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let accent = args.get(6);

    let t0 = Instant::now();
    let html = fs::read_to_string(&path)
        .expect("read")
        .replacen("data-theme=\"dark\"", &format!("data-theme=\"{theme}\""), 1);
    let html = if std::env::var_os("SPIKE_EXPAND_USE").is_some() { expand_use(&html) } else { html };
    let scheme = if theme == "dark" { ColorScheme::Dark } else { ColorScheme::Light };
    let config = DocumentConfig {
        viewport: Some(Viewport::new((w as f64 * scale) as u32, (h as f64 * scale) as u32, scale as f32, scheme)),
        base_url: Some(format!("file://{}", path.display())),
        net_provider: Some(Arc::new(FileProvider)),
        ..Default::default()
    };
    let mut doc = HtmlDocument::from_html(&html, config);
    doc.handle_messages();
    let t_parse = ms(t0);

    let t1 = Instant::now();
    doc.resolve(0.0);
    let t_resolve = ms(t1);

    let t2 = Instant::now();
    let buf = render(&mut doc, (w as f64 * scale) as u32, (h as f64 * scale) as u32, scale);
    let t_paint = ms(t2);

    // Second frame: warm caches (fonts, glyphs) to show steady-state cost.
    let t3 = Instant::now();
    doc.resolve(16.0);
    let _ = render(&mut doc, (w as f64 * scale) as u32, (h as f64 * scale) as u32, scale);
    let t_warm = ms(t3);

    let mut t_retheme = f64::NAN;
    if let Some(hex) = accent {
        let t4 = Instant::now();
        let root = doc.root_element().id;
        let name = QualName::new(None, ns!(), local_name!("style"));
        doc.mutate().set_attribute(root, name, &format!("--accent:{hex};--accent-hover:{hex};--accent-press:{hex}"));
        doc.resolve(32.0);
        let _ = render(&mut doc, (w as f64 * scale) as u32, (h as f64 * scale) as u32, scale);
        t_retheme = ms(t4);
        let buf2 = render(&mut doc, (w as f64 * scale) as u32, (h as f64 * scale) as u32, scale);
        write_png(&PathBuf::from(&args[2]).with_extension("accent.png"), &buf2, (w as f64 * scale) as u32, (h as f64 * scale) as u32);
    }

    if let Ok(pos) = std::env::var("SPIKE_HOVER") {
        let (hx, hy) = pos.split_once(',').expect("x,y");
        let (hx, hy): (f32, f32) = (hx.parse().expect("x"), hy.parse().expect("y"));
        let th = Instant::now();
        let hit = doc.hit(hx, hy).map(|r| r.node_id);
        let t_hit = ms(th);
        let tc = Instant::now();
        let changed = doc.set_hover_to(hx, hy);
        doc.resolve(48.0);
        let hover_buf = render(&mut doc, (w as f64 * scale) as u32, (h as f64 * scale) as u32, scale);
        println!("hit({hx},{hy}) -> {hit:?} in {t_hit:.3} ms; hover changed={changed}; hover restyle+layout+paint {:.1} ms", ms(tc));
        write_png(&PathBuf::from(&args[2]).with_extension("hover.png"), &hover_buf, (w as f64 * scale) as u32, (h as f64 * scale) as u32);
    }

    write_png(&PathBuf::from(&args[2]), &buf, (w as f64 * scale) as u32, (h as f64 * scale) as u32);
    println!(
        "{} {w}x{h} {theme}: parse+load {t_parse:.1} ms | style+layout {t_resolve:.1} ms | paint+raster {t_paint:.1} ms | warm 2nd frame {t_warm:.1} ms | accent-change frame {t_retheme:.1} ms | peak RSS {:.0} MiB | total {:.1} ms",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
        rss_mib(),
        ms(t0)
    );
}

fn write_png(path: &PathBuf, rgba: &[u8], w: u32, h: u32) {
    let file = fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("hdr").write_image_data(rgba).expect("data");
}
