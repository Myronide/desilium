//! Top bar plus dock on the real layer-shell, driven from a calloop event loop.
//!
//! This is the integration pattern the shell uses: one `EventLoop`, the host's
//! fd as a source, a timer-ish wait for the clock. Flags:
//! `--static` (no clock ticks: zero wakeups when idle), `--hover X,Y` and
//! `--click X,Y` (synthetic pointer input on the dock, since headless runs have
//! no input device), `--exit-after SECS`.

use std::{
    os::fd::AsFd,
    time::{Duration, Instant},
};

use calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use desilium_ui::{
    SurfaceOptions, UiEvent, UiSurface, Updates, app_icon_html, icon_svg,
    wayland::{Anchor, HostEvent, LayerConfig, LayerHost, SurfaceId},
};

/// Copy of docs/design/shared.css (the docs folder is not in the Docker build context).
const SHARED_CSS: &str = include_str!("shared.css");
const DEMO_CSS: &str = "
.dockwrap { position: absolute; left: 0; right: 0; bottom: 0; height: 88px; display: flex; justify-content: center; align-items: flex-end; padding-bottom: 14px; }
.dockwrap .dock { position: relative; left: auto; bottom: auto; transform: none; }
.dock-item { cursor: pointer; }
.dock-item:hover .app-icon { transform: translateY(-6px); box-shadow: 0 8px 14px rgba(0,0,0,.35), inset 0 1px 0 rgba(255,255,255,.3); }
.tb-btn[data-action]:hover { background: var(--hover); cursor: pointer; }
.hint { color: var(--text-2); font-weight: var(--fw-regular); }
";
const DOCK_APPS: [(&str, &str); 7] = [
    ("files", "Files"),
    ("web", "Web"),
    ("code", "Code"),
    ("terminal", "Terminal"),
    ("music", "Music"),
    ("photos", "Photos"),
    ("mail", "Mail"),
];
const BAR_HEIGHT: u32 = 32;
const DOCK_SIZE: (u32, u32) = (640, 96);
const FIXED_CLOCK: &str = "14:32";

fn bar_html() -> String {
    format!(
        "<div class=topbar style='position:relative'>\
         <div class=left><div class='tb-btn' data-action=workspaces data-tooltip='Workspaces'><span class=ws-dots><i></i><i class=on></i><i></i><i></i></span></div>\
         <div class=tb-btn>{code}<span id=app>Desilium</span></div><div class='tb-btn dim' id=status>ready</div></div>\
         <div class='tb-btn tb-clock' data-action=calendar><span class=date id=date>Mon 26 Oct</span><span id=clock>{FIXED_CLOCK}</span></div>\
         <div class=right><div class='tb-btn dim hint' id=hint></div><div class=tb-group>{wifi}{vol}{bat}</div></div></div>",
        code = icon_svg("code", 16, "s16"),
        wifi = icon_svg("wifi", 16, "s16"),
        vol = icon_svg("volume", 16, "s16"),
        bat = icon_svg("battery", 16, "s16"),
    )
}

fn dock_html() -> String {
    let items: String = DOCK_APPS
        .iter()
        .map(|(app, label)| {
            format!(
                "<div class=dock-item data-action=launch data-arg={app} data-tooltip={label} id=app-{app}>{}</div>",
                app_icon_html(app, "")
            )
        })
        .collect();
    format!("<div class=dockwrap><div class=dock>{items}</div></div>")
}

struct Demo {
    host: LayerHost,
    bar: SurfaceId,
    dock: SurfaceId,
    seconds: bool,
    last_clock: String,
    loops: u64,
}

impl Demo {
    fn pump(&mut self) {
        if let Err(error) = self.host.dispatch(Instant::now()) {
            eprintln!("ui-demo: {error}");
            std::process::exit(1);
        }
        for event in self.host.take_events() {
            self.handle(event);
        }
    }

    fn handle(&mut self, event: HostEvent) {
        match event {
            HostEvent::Ui {
                event: UiEvent::Action(action),
                ..
            } => {
                let text = format!("{} {}", action.action, action.arg.unwrap_or_default());
                self.host
                    .update(self.bar, |ui| ui.set_text("#status", &text));
            }
            HostEvent::Hover { surface, target } if surface == self.dock => {
                let tip = target.and_then(|t| t.tooltip).unwrap_or_default();
                self.host.update(self.bar, |ui| ui.set_text("#hint", &tip));
            }
            HostEvent::Closed(_) => std::process::exit(0),
            _ => {}
        }
    }

    fn tick_clock(&mut self) {
        if !self.seconds {
            return;
        }
        let clock = clock_text();
        if clock == self.last_clock {
            return;
        }
        let batch = Updates::new().text("#clock", &clock);
        self.host.update(self.bar, |ui| ui.apply(batch));
        self.last_clock = clock;
    }
}

fn clock_text() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

fn flag_value(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    let at = args.iter().position(|a| a == name)?;
    args.get(at + 1).cloned()
}

fn point(value: Option<String>) -> Option<(f64, f64)> {
    let value = value?;
    let (x, y) = value.split_once(',')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn cpu_seconds() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let fields: Vec<&str> = stat
        .rsplit(')')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let ticks = |i: usize| {
        fields
            .get(i)
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0)
    };
    (ticks(11) + ticks(12)) / 100.0
}

fn peak_rss_mib() -> f64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let kib = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(0.0);
    kib / 1024.0
}

fn main() {
    if let Err(message) = run() {
        eprintln!("ui-demo: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut host = LayerHost::connect().map_err(|e| e.to_string())?;

    let bar_ui = UiSurface::new(
        &bar_html(),
        &[SHARED_CSS, DEMO_CSS],
        SurfaceOptions::new(1920, BAR_HEIGHT),
    )
    .map_err(|e| e.to_string())?;
    let mut bar_config = LayerConfig::new("desilium-demo-bar", (0, BAR_HEIGHT));
    bar_config.anchor = Anchor::TOP_BAR;
    bar_config.exclusive_zone = BAR_HEIGHT as i32;
    let bar = host
        .add_surface(bar_config, bar_ui)
        .map_err(|e| e.to_string())?;

    let dock_ui = UiSurface::new(
        &dock_html(),
        &[SHARED_CSS, DEMO_CSS],
        SurfaceOptions::new(DOCK_SIZE.0, DOCK_SIZE.1),
    )
    .map_err(|e| e.to_string())?;
    let mut dock_config = LayerConfig::new("desilium-demo-dock", DOCK_SIZE);
    dock_config.anchor = Anchor::BOTTOM_CENTER;
    let dock = host
        .add_surface(dock_config, dock_ui)
        .map_err(|e| e.to_string())?;

    let mut demo = Demo {
        host,
        bar,
        dock,
        seconds: !std::env::args().any(|a| a == "--static"),
        last_clock: FIXED_CLOCK.to_owned(),
        loops: 0,
    };
    let mut event_loop: EventLoop<Demo> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let fd = demo
        .host
        .poll_fd()
        .try_clone_to_owned()
        .map_err(|e| e.to_string())?;
    event_loop
        .handle()
        .insert_source(
            Generic::new(fd, Interest::READ, Mode::Level),
            |_, _, demo: &mut Demo| {
                demo.pump();
                Ok(PostAction::Continue)
            },
        )
        .map_err(|e| e.to_string())?;

    let started = Instant::now();
    let cpu_start = cpu_seconds();
    let exit_after = flag_value("--exit-after")
        .and_then(|v| v.parse::<f64>().ok())
        .map(Duration::from_secs_f64);
    let synthetic = (point(flag_value("--hover")), point(flag_value("--click")));
    let mut injected = false;
    loop {
        demo.pump();
        if !injected && !demo.host.is_busy() {
            injected = true;
            inject(&mut demo, synthetic);
        }
        let now = Instant::now();
        let clock_wait = demo.seconds.then(|| {
            Duration::from_millis(
                1000 - u64::from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.subsec_millis()),
                ),
            )
        });
        let timeout = [clock_wait, demo.host.next_timeout(now)]
            .into_iter()
            .flatten()
            .min();
        let timeout = match (timeout, exit_after) {
            (Some(t), Some(limit)) => Some(t.min(limit.saturating_sub(started.elapsed()))),
            (None, Some(limit)) => Some(limit.saturating_sub(started.elapsed())),
            (t, None) => t,
        };
        event_loop
            .dispatch(timeout, &mut demo)
            .map_err(|e| e.to_string())?;
        demo.loops += 1;
        demo.tick_clock();
        if exit_after.is_some_and(|limit| started.elapsed() >= limit) {
            break;
        }
    }
    println!(
        "ui-demo: {:.1}s wall, {:.3}s cpu, {} loop iterations, {} frames committed, peak RSS {:.0} MiB",
        started.elapsed().as_secs_f64(),
        cpu_seconds() - cpu_start,
        demo.loops,
        demo.host.frame_count(),
        peak_rss_mib()
    );
    Ok(())
}

type Point = (f64, f64);

fn inject(demo: &mut Demo, (hover, click): (Option<Point>, Option<Point>)) {
    let dock = demo.dock;
    if let Some((x, y)) = hover {
        demo.host.update(dock, |ui| ui.pointer_move(x, y));
    }
    if let Some((x, y)) = click {
        demo.host.update(dock, |ui| {
            ui.pointer_down(x, y, desilium_ui::PointerButton::Primary);
            ui.pointer_up(x, y, desilium_ui::PointerButton::Primary);
        });
    }
    demo.pump();
    let _ = demo.host.poll_fd().as_fd();
}
