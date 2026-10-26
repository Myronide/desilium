//! The shell process: owns the UI host, the models and the single event loop.

mod context;
mod demo;
mod events;
mod launcher_glue;
mod quick;
mod stats;
mod system;
mod timers;
mod views;

use std::{
    collections::HashMap,
    ffi::OsString,
    path::PathBuf,
    time::{Duration, Instant},
};

use calloop::{EventLoop, Interest, Mode, PostAction, generic::Generic};
use desilium_apps::{Catalog, Environment, IconResolver, TerminalSpec, UsageStats};
use desilium_config::{ConfigStore, ReloadOutcome};
use desilium_theme::Theme;
use desilium_ui::wayland::LayerHost;
use wayland_client::Connection;

use crate::{
    bar::Bar,
    cli::Args,
    dock::Dock,
    error::ShellError,
    launcher::Launcher,
    model::{
        battery::Battery,
        clock::{ClockText, HourCycle, format_clock, until_next_change},
        dock::DockItem,
        pins::{DOCK_FILE_NAME, DockConfig},
        windows::{WindowId, WindowModel, Workspaces},
    },
    services::{
        appearance::resolve_theme,
        apps::Starter,
        battery::{POWER_SUPPLY_DIR, read_battery},
        protocol::{OutputInfo, ShellProtocol},
        watch::DirWatch,
    },
    tooltip::Tooltip,
};
use timers::{Deadlines, Due};

/// Icon theme used until the configuration offers a choice.
const ICON_THEME: &str = "hicolor";
/// Environment variable that overrides [`ICON_THEME`].
const ICON_THEME_VAR: &str = "DESILIUM_ICON_THEME";
/// Catalog, icon theme and battery are re-checked this often.
const HOUSEKEEPING: Duration = Duration::from_secs(60);
/// Quiet time after the last configuration file event before reloading.
const CONFIG_DEBOUNCE: Duration = Duration::from_millis(100);
/// Hover time before a tooltip appears.
const TOOLTIP_DELAY: Duration = Duration::from_millis(400);
/// The clock timer fires this long after the boundary so the new text is never stale.
const CLOCK_SLACK: Duration = Duration::from_millis(20);
/// Event passes per wake-up before yielding (actions can cause further events).
const MAX_EVENT_PASSES: usize = 8;

/// A tooltip waiting for its delay.
pub(crate) struct PendingTip {
    pub surface: desilium_ui::wayland::SurfaceId,
    pub text: String,
    pub rect: desilium_ui::Rect,
}

/// Everything the shell process owns.
pub struct Shell {
    quick: crate::quick::QuickSettings,
    quick_focused: bool,
    controls: Option<crate::services::controls::Worker>,
    host: LayerHost,
    proto: Option<ShellProtocol>,
    args: Args,
    theme: Theme,
    store: ConfigStore,
    config_dir: PathBuf,
    catalog: Catalog,
    icons: IconResolver,
    usage: UsageStats,
    starter: Starter,
    pins: DockConfig,
    windows: WindowModel,
    workspaces: Workspaces,
    output: Option<OutputInfo>,
    battery: Option<Battery>,
    clock: ClockText,
    bar: Bar,
    dock: Dock,
    launcher: Launcher,
    tooltip: Tooltip,
    dock_items: Vec<DockItem>,
    minimize_sent: HashMap<WindowId, (i32, i32, i32, i32)>,
    pending_tip: Option<PendingTip>,
    launcher_focused: bool,
    icon_scale: u32,
    watch: Option<DirWatch>,
    config_files_changed: bool,
    deadlines: Deadlines,
    demo_pending: bool,
    demo_steps: std::collections::VecDeque<(Duration, demo::Step)>,
    quit: bool,
}

impl Shell {
    /// Connects, loads configuration and creates all surfaces.
    pub fn new(args: Args) -> Result<Self, ShellError> {
        let connection = Connection::connect_to_env()?;
        let mut host = LayerHost::from_connection(connection.clone())?;
        let proto = ShellProtocol::connect(&connection)?;
        if proto.is_none() {
            tracing::warn!(
                "desilium_shell_manager_v1 is not offered: running without windows, workspaces and effects"
            );
        }
        let config_dir = args
            .config_dir
            .clone()
            .unwrap_or_else(ConfigStore::default_dir);
        let store = ConfigStore::open(&config_dir).store;
        let theme = resolve_theme(&store.get().appearance, args.theme);
        let env = Environment::from_env();
        let catalog = crate::services::bundled::catalog(
            &env,
            std::env::current_exe()
                .ok()
                .as_deref()
                .and_then(std::path::Path::parent),
        );
        let icon_theme = std::env::var(ICON_THEME_VAR).unwrap_or_else(|_| ICON_THEME.to_owned());
        let icons = IconResolver::new(&icon_theme, &env);
        let usage = UsageStats::load(UsageStats::default_path(&env.state_home));
        let terminal = store
            .get()
            .compositor
            .terminal
            .clone()
            .map(TerminalSpec::custom)
            .or_else(|| TerminalSpec::detect(&env));
        let starter = Starter::new(std::env::var_os("WAYLAND_DISPLAY"), terminal);
        let pins = system::load_pins(&config_dir, &catalog);
        let battery = read_battery(std::path::Path::new(POWER_SUPPLY_DIR));
        let local = system::local_now();
        let clock = format_clock(&local, HourCycle::H24, false);
        let initial = views::initial_views(
            &catalog,
            &pins,
            &clock,
            battery,
            &mut IconResolver::new(&icon_theme, &env),
        );
        let bar = Bar::new(&mut host, &theme, initial.bar)?;
        let dock = Dock::new(&mut host, &theme, initial.dock)?;
        let launcher = Launcher::new(&theme)?;
        let tooltip = Tooltip::new(&theme)?;
        let watch = DirWatch::new(&config_dir)
            .map_err(|error| tracing::warn!(%error, "live configuration reload is disabled"))
            .ok();
        let mut shell = Self {
            quick: crate::quick::QuickSettings::new(&theme)?,
            quick_focused: false,
            controls: None,
            host,
            proto,
            theme,
            store,
            config_dir,
            catalog,
            icons,
            usage,
            starter,
            pins,
            windows: WindowModel::default(),
            workspaces: Workspaces::default(),
            output: None,
            battery,
            clock,
            bar,
            dock,
            launcher,
            tooltip,
            dock_items: Vec::new(),
            minimize_sent: HashMap::new(),
            pending_tip: None,
            launcher_focused: false,
            icon_scale: 1,
            watch,
            config_files_changed: false,
            deadlines: Deadlines::default(),
            demo_pending: args.demo_hover.is_some(),
            demo_steps: if args.demo_launcher_cycle {
                demo::launcher_cycle()
            } else {
                Default::default()
            },
            args,
            quit: false,
        };
        shell.apply_effects();
        shell.arm_clock(local);
        shell
            .deadlines
            .set(Due::Housekeeping, Instant::now() + HOUSEKEEPING);
        if let Some(limit) = shell.args.exit_after {
            shell.deadlines.set(Due::Exit, Instant::now() + limit);
        }
        shell.refresh_bar();
        shell.refresh_dock();
        shell.start_demo();
        Ok(shell)
    }

    fn arm_clock(&mut self, local: crate::model::clock::LocalTime) {
        let wait = until_next_change(&local, false) + CLOCK_SLACK;
        self.deadlines.set(Due::Clock, Instant::now() + wait);
    }

    /// Logical output size: from the shell protocol, else from the output's own report.
    pub(crate) fn output_size(&self) -> Option<(i32, i32)> {
        self.output
            .map(|output| (output.width, output.height))
            .or_else(|| self.host.output_logical_size())
    }

    /// Asks the compositor for blur and rounded corners behind our surfaces.
    fn apply_effects(&mut self) {
        let Some(proto) = &self.proto else {
            return;
        };
        let surfaces = [
            (Some(self.bar.id()), views::BAR_BLUR, [0; 4]),
            (
                Some(self.dock.id()),
                views::DOCK_BLUR,
                [if self.store.get().shell.mode == desilium_config_types::PanelMode::Taskbar {
                    8
                } else {
                    crate::dock::RADIUS
                }; 4],
            ),
            (
                self.launcher.id(),
                views::LAUNCHER_BLUR,
                [views::LAUNCHER_RADIUS; 4],
            ),
        ];
        for (id, blur, radii) in surfaces {
            let surface = id.and_then(|id| self.host.wl_surface(id));
            if let Some(surface) = surface {
                proto.set_surface_effects(&surface, blur, radii);
            }
        }
    }

    /// Handles everything pending on the socket and in the models.
    pub fn pump(&mut self) {
        if let Err(error) = self.host.dispatch(Instant::now()) {
            tracing::warn!(%error, "compositor connection lost, exiting");
            self.quit = true;
            return;
        }
        self.drain_protocol();
        for _ in 0..MAX_EVENT_PASSES {
            let events = self.host.take_events();
            if events.is_empty() {
                break;
            }
            for event in events {
                self.handle_host_event(event);
            }
        }
        self.check_scale();
        self.run_demo_hover();
        if let Err(error) = self.host.connection().flush() {
            tracing::warn!(%error, "flush failed, exiting");
            self.quit = true;
        }
    }

    fn drain_protocol(&mut self) {
        let Some(proto) = &mut self.proto else {
            return;
        };
        if let Err(error) = proto.dispatch_pending() {
            tracing::warn!(%error, "shell protocol failed, exiting");
            self.quit = true;
            return;
        }
        let update = proto.take_update();
        if let Some(output) = update.output {
            self.output = Some(output);
        }
        if let Some(workspaces) = update.workspaces {
            self.workspaces = workspaces;
        }
        let output_changed = update.output.is_some();
        let windows_changed = update.windows.is_some();
        if let Some(windows) = update.windows {
            self.windows.replace(windows);
        }
        if windows_changed || update.workspaces.is_some() || output_changed {
            self.refresh_bar();
            self.refresh_dock();
        }
        if output_changed && self.quick.is_open() {
            let output = self.output_size().unwrap_or((1280, 800));
            if let Err(error) =
                self.quick
                    .reposition(&mut self.host, &self.store.get().shell, output)
            {
                tracing::warn!(%error,"popup resize failed");
            }
        }
        for name in update.shortcuts {
            self.handle_shortcut(&name);
        }
    }

    fn handle_shortcut(&mut self, name: &str) {
        if name == desilium_protocols::shortcut::TOGGLE_LAUNCHER {
            self.toggle_launcher();
        }
    }

    /// Re-renders the clock; arms the next boundary.
    fn tick_clock(&mut self) {
        let local = system::local_now();
        let cycle = if self.store.get().shell.clock_24h {
            HourCycle::H24
        } else {
            HourCycle::H12
        };
        self.clock = format_clock(&local, cycle, false);
        self.arm_clock(local);
        self.refresh_bar();
        if self.store.get().shell.mode == desilium_config_types::PanelMode::Taskbar {
            self.refresh_dock();
        }
    }

    fn housekeeping(&mut self) {
        let mut changed = self.catalog.refresh_if_changed();
        changed |= self.icons.revalidate();
        let battery = read_battery(std::path::Path::new(POWER_SUPPLY_DIR));
        let battery_changed = battery != self.battery;
        self.battery = battery;
        if changed {
            self.refresh_dock();
        }
        if changed || battery_changed {
            self.refresh_bar();
        }
        self.deadlines
            .set(Due::Housekeeping, Instant::now() + HOUSEKEEPING);
    }

    /// The inotify descriptor became readable.
    ///
    /// Returns false when the watch is broken and its source should be removed.
    fn config_fd_ready(&mut self) -> bool {
        let Some(watch) = &mut self.watch else {
            return false;
        };
        match watch.drain() {
            Ok(names) => {
                self.config_files_touched(&names);
                true
            }
            Err(error) => {
                tracing::warn!(%error, "configuration watch stopped");
                self.watch = None;
                false
            }
        }
    }

    /// Notes that a file in the configuration directory changed; reload after a quiet period.
    fn config_files_touched(&mut self, names: &[OsString]) {
        let relevant = names
            .iter()
            .any(|name| name == "config.toml" || name == DOCK_FILE_NAME);
        if !relevant {
            return;
        }
        self.config_files_changed = true;
        self.deadlines
            .set(Due::Config, Instant::now() + CONFIG_DEBOUNCE);
    }

    fn reload_config(&mut self) {
        if !std::mem::take(&mut self.config_files_changed) {
            return;
        }
        if let Some(ReloadOutcome::Applied { .. }) = self.store.reload_if_changed() {
            let terminal = self
                .store
                .get()
                .compositor
                .terminal
                .clone()
                .map(TerminalSpec::custom)
                .or_else(|| TerminalSpec::detect(&Environment::from_env()));
            self.starter.set_terminal(terminal);
            self.apply_theme();
            self.tick_clock();
            self.refresh_dock();
            self.apply_effects();
        }
        let pins = system::load_pins(&self.config_dir, &self.catalog);
        if pins != self.pins {
            self.pins = pins;
            self.refresh_dock();
        }
    }

    fn apply_theme(&mut self) {
        let theme = resolve_theme(&self.store.get().appearance, self.args.theme);
        if theme == self.theme {
            return;
        }
        for id in [self.bar.id(), self.dock.id()] {
            self.host.update(id, |ui| ui.set_theme(&theme));
        }
        self.launcher.set_theme(&mut self.host, &theme);
        self.tooltip.set_theme(&mut self.host, &theme);
        self.quick.theme(&mut self.host, &theme);
        self.theme = theme;
    }

    fn fire(&mut self, due: Due) {
        match due {
            Due::Clock => self.tick_clock(),
            Due::Tooltip => self.show_pending_tooltip(),
            Due::Housekeeping => self.housekeeping(),
            Due::Config => self.reload_config(),
            Due::Exit => self.quit = true,
            Due::Demo => self.demo_step(),
        }
    }

    fn run_due(&mut self) {
        for due in self.deadlines.take_due(Instant::now()) {
            self.fire(due);
        }
    }
}

/// Runs the shell until the compositor goes away or `--exit-after` elapses.
pub fn run(args: Args) -> Result<(), ShellError> {
    let open_launcher = args.open_launcher;
    let query = args.launcher_query.clone().unwrap_or_default();
    let mut shell = Shell::new(args)?;
    let mut event_loop: EventLoop<Shell> =
        EventLoop::try_new().map_err(|error| ShellError::EventLoop(error.to_string()))?;
    let handle = event_loop.handle();
    let (control_tx, control_rx) = calloop::channel::channel();
    shell.controls = crate::services::controls::Worker::spawn(control_tx)
        .map_err(|error| tracing::warn!(%error,"system controls are unavailable"))
        .ok();
    handle
        .insert_source(control_rx, |event, _, shell: &mut Shell| {
            if let calloop::channel::Event::Msg(state) = event {
                shell.quick_updated(state);
            }
        })
        .map_err(|error| ShellError::EventLoop(error.to_string()))?;

    let wayland_fd = shell
        .host
        .poll_fd()
        .try_clone_to_owned()
        .map_err(|error| ShellError::EventLoop(error.to_string()))?;
    handle
        .insert_source(
            Generic::new(wayland_fd, Interest::READ, Mode::Level),
            |_, _, shell: &mut Shell| {
                shell.pump();
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| ShellError::EventLoop(error.to_string()))?;
    if let Some(watch) = &shell.watch {
        let fd = watch
            .fd()
            .try_clone_to_owned()
            .map_err(|error| ShellError::EventLoop(error.to_string()))?;
        handle
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                |_, _, shell: &mut Shell| {
                    Ok(if shell.config_fd_ready() {
                        PostAction::Continue
                    } else {
                        PostAction::Remove
                    })
                },
            )
            .map_err(|error| ShellError::EventLoop(error.to_string()))?;
    }
    let started = Instant::now();
    let cpu_start = stats::cpu_seconds();
    shell.pump();
    if open_launcher {
        shell.open_launcher(&query);
    }
    while !shell.quit {
        shell.pump();
        shell.run_due();
        if shell.quit {
            break;
        }
        let now = Instant::now();
        let timeout = [shell.deadlines.timeout(now), shell.host.next_timeout(now)]
            .into_iter()
            .flatten()
            .min();
        event_loop
            .dispatch(timeout, &mut shell)
            .map_err(|error| ShellError::EventLoop(error.to_string()))?;
    }
    if shell.args.exit_after.is_some() {
        eprintln!(
            "desilium-shell: {:.1}s wall, {:.3}s cpu, {} frames committed, peak RSS {:.0} MiB",
            started.elapsed().as_secs_f64(),
            stats::cpu_seconds() - cpu_start,
            shell.host.frame_count(),
            stats::peak_rss_mib()
        );
    }
    Ok(())
}
