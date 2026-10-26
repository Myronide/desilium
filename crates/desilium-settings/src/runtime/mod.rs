//! Start-up and the Wayland event loop around [`App`].
//!
//! One thread. Sources: the Wayland socket, an inotify watch on the settings
//! directory and a channel from the thumbnail worker. Timers are not sources:
//! the loop computes the next deadline (toast expiry, highlight, key repeat,
//! reload debounce) and sleeps exactly that long, so an idle window costs nothing.

use std::{
    io,
    time::{Duration, Instant},
};

use calloop::{EventLoop, Interest, Mode, PostAction, channel, generic::Generic};
use desilium_config::{ConfigStore, Recovery};
use desilium_ui::{
    HoverTarget, Rect, UiAction, UiError, UiEvent, UiSurface, Updates,
    wayland::{WindowConfig, WindowEvent, WindowId, XdgWindowHost},
};

use crate::{
    app::{App, AppOptions},
    model::{Level, is_compact},
    refresh::Refresh,
    surface::build_surface,
    wallpapers::{ThumbResult, ThumbWorker},
};
use cli::{Cli, split_action};
use cursors::{find_themes, icon_dirs};
use seed::apply_sets;
use watch::ConfigWatcher;

pub mod cli;
pub mod cursors;
pub mod seed;
pub mod watch;

const APP_ID: &str = "org.desilium.Settings";
const WINDOW_TITLE: &str = "Settings";
const MIN_SIZE: (u32, u32) = (720, 520);
/// An editor saves a file in several steps; wait for the dust to settle before reloading.
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(100);
const TOOLTIP_GAP: f64 = 6.0;
/// Rough tooltip width per character, used to keep it inside the window.
const TOOLTIP_CHAR_WIDTH: f64 = 7.5;

/// Why the app stopped.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// Bad `--set` value or similar start-up problem.
    #[error("{0}")]
    Startup(String),
    /// The compositor connection or the document failed.
    #[error("{0}")]
    Ui(#[from] UiError),
    /// The event loop failed.
    #[error("event loop: {0}")]
    Loop(String),
    /// A system call failed.
    #[error(transparent)]
    Io(#[from] io::Error),
}

struct Shell {
    host: XdgWindowHost,
    app: App,
    window: WindowId,
    thumbs: Vec<ThumbResult>,
    watcher: Option<ConfigWatcher>,
    reload_at: Option<Instant>,
    quit: bool,
}

/// Opens the window and runs until it is closed.
pub fn run(cli: Cli) -> Result<(), RunError> {
    let config_dir = cli
        .config_dir
        .clone()
        .unwrap_or_else(ConfigStore::default_dir);
    let opened = ConfigStore::open(&config_dir);
    let mut store = opened.store;
    apply_sets(&mut store, &cli.sets).map_err(RunError::Startup)?;

    let mut event_loop: EventLoop<Shell> =
        EventLoop::try_new().map_err(|error| RunError::Loop(error.to_string()))?;
    let (thumb_tx, thumb_rx) = channel::channel::<ThumbResult>();
    let mut options = AppOptions::from_environment(config_dir.clone());
    options.page = cli.page;
    options.query = cli.search.clone();
    options.cursor_themes = find_themes(&icon_dirs());
    if let Some(dir) = &cli.wallpaper_dir {
        options.wallpaper_env.override_dir = Some(dir.clone().into_os_string());
    }
    let worker = ThumbWorker::spawn(options.cache_dir.clone(), move |result| {
        let _ = thumb_tx.send(result);
    });
    let mut app = App::new(store, options, Some(worker));
    announce_recovery(&mut app, &opened.recovery);
    let now = Instant::now();
    for text in &cli.actions {
        run_action(&mut app, text, now);
    }

    if cli.no_window {
        return Ok(());
    }
    let mut host = XdgWindowHost::connect().map_err(friendly_connect_error)?;
    let ui = build_surface(&mut app, cli.size, 1.0, now)?;
    let mut window_config = WindowConfig::new(WINDOW_TITLE, APP_ID, cli.size);
    window_config.min_size = Some(MIN_SIZE);
    let window = host.add_window(window_config, ui)?;

    let handle = event_loop.handle();
    let wayland_fd = host.poll_fd().try_clone_to_owned()?;
    insert_readiness(&handle, wayland_fd, |_| {})?;
    let mut shell = Shell {
        host,
        app,
        window,
        thumbs: Vec::new(),
        watcher: start_watcher(&config_dir),
        reload_at: None,
        quit: false,
    };
    if let Some(watcher) = &shell.watcher {
        let fd = watcher.fd().try_clone_to_owned()?;
        insert_readiness(&handle, fd, Shell::on_watch)?;
    }
    handle
        .insert_source(thumb_rx, |event, _, shell: &mut Shell| {
            if let channel::Event::Msg(result) = event {
                shell.thumbs.push(result);
            }
        })
        .map_err(|error| RunError::Loop(error.to_string()))?;
    shell.main_loop(&mut event_loop)
}

fn friendly_connect_error(error: UiError) -> RunError {
    RunError::Startup(format!(
        "cannot connect to the Wayland display ({error}). Start Desilium, or any Wayland compositor, first."
    ))
}

fn start_watcher(dir: &std::path::Path) -> Option<ConfigWatcher> {
    match ConfigWatcher::new(dir.to_path_buf()) {
        Ok(watcher) => Some(watcher),
        Err(error) => {
            tracing::warn!(%error, "cannot watch the settings file; outside edits need a restart");
            None
        }
    }
}

fn insert_readiness(
    handle: &calloop::LoopHandle<'static, Shell>,
    fd: std::os::fd::OwnedFd,
    on_ready: impl Fn(&mut Shell) + 'static,
) -> Result<(), RunError> {
    handle
        .insert_source(
            Generic::new(fd, Interest::READ, Mode::Level),
            move |_, _, shell| {
                on_ready(shell);
                Ok(PostAction::Continue)
            },
        )
        .map(|_| ())
        .map_err(|error| RunError::Loop(error.to_string()))
}

/// Tells the user about anything the store repaired while opening.
fn announce_recovery(app: &mut App, recovery: &[Recovery]) {
    if recovery.is_empty() {
        return;
    }
    let text = recovery
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    app.show_notice(
        Level::Info,
        format!("Your settings file needed repairs: {text}"),
    );
}

/// `--do ACTION[:ARG]`.
fn run_action(app: &mut App, text: &str, now: Instant) {
    let (action, arg) = split_action(text);
    let event = UiEvent::Action(UiAction {
        action: action.to_owned(),
        arg: arg.map(str::to_owned),
        element_id: None,
        rect: Rect::default(),
    });
    app.handle_event(&event, now);
}

impl Shell {
    fn on_watch(&mut self) {
        let Some(watcher) = self.watcher.as_mut() else {
            return;
        };
        match watcher.drain() {
            Ok(true) => self.reload_at = Some(Instant::now() + RELOAD_DEBOUNCE),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(%error, "settings watch stopped");
                self.watcher = None;
            }
        }
    }

    fn main_loop(&mut self, event_loop: &mut EventLoop<Shell>) -> Result<(), RunError> {
        self.pump(Instant::now())?;
        while !self.quit {
            let timeout = self.next_timeout(Instant::now());
            event_loop
                .dispatch(timeout, self)
                .map_err(|error| RunError::Loop(error.to_string()))?;
            self.pump(Instant::now())?;
        }
        Ok(())
    }

    fn next_timeout(&self, now: Instant) -> Option<Duration> {
        let until = |at: Instant| at.saturating_duration_since(now);
        [
            self.host.next_timeout(now),
            self.app.next_deadline().map(until),
            self.reload_at.map(until),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn pump(&mut self, now: Instant) -> Result<(), RunError> {
        self.host.dispatch(now)?;
        let mut refresh = Refresh::default();
        for event in self.host.take_events() {
            refresh = refresh.merge(self.on_window_event(event, now));
        }
        for result in std::mem::take(&mut self.thumbs) {
            refresh = refresh.merge(self.app.on_thumb(result));
        }
        if self.reload_at.is_some_and(|at| at <= now) {
            self.reload_at = None;
            refresh = refresh.merge(self.app.reload_external(now));
        }
        refresh = refresh.merge(self.app.tick(now));
        self.apply(&refresh, now);
        self.host.set_key_capture(self.app.wants_key_capture());
        Ok(())
    }

    fn on_window_event(&mut self, event: WindowEvent, now: Instant) -> Refresh {
        match event {
            WindowEvent::Ui { event, .. } => self.app.handle_event(&event, now),
            WindowEvent::Hover { target, .. } => {
                self.show_tooltip(target);
                Refresh::default()
            }
            WindowEvent::Key { event, .. } => {
                let typing = self
                    .host
                    .surface(self.window)
                    .is_some_and(UiSurface::wants_text_input);
                self.app.on_key(&event, typing, now)
            }
            WindowEvent::CloseRequested(_) => {
                self.quit = true;
                Refresh::default()
            }
            WindowEvent::Resized { size, .. } => self.on_resize(size.0),
        }
    }

    fn on_resize(&mut self, width: u32) -> Refresh {
        let compact = is_compact(width);
        if self.app.model().compact == compact {
            return Refresh::default();
        }
        self.app.model_mut().compact = compact;
        Refresh::main()
    }

    fn apply(&mut self, refresh: &Refresh, now: Instant) {
        if refresh.is_empty() {
            return;
        }
        let app = &self.app;
        let applied = self
            .host
            .update(self.window, |ui| refresh.apply(app, ui, now));
        match applied {
            Some(Ok(())) => {}
            Some(Err(error)) => tracing::warn!(%error, "could not update the window"),
            None => return,
        }
        if refresh.nav {
            let page = self.app.model().page.title();
            if let Err(error) = self
                .host
                .set_title(self.window, &format!("{page} - {WINDOW_TITLE}"))
            {
                tracing::warn!(%error, "could not set the window title");
            }
        }
    }

    fn show_tooltip(&mut self, target: Option<HoverTarget>) {
        let width = self
            .host
            .surface(self.window)
            .map_or(0.0, |ui| f64::from(ui.size().0));
        let updates = tooltip_updates(target.as_ref(), width);
        if let Some(Err(error)) = self.host.update(self.window, |ui| ui.apply(updates)) {
            tracing::debug!(%error, "tooltip update failed");
        }
    }
}

/// Document updates that show (or hide) the tooltip next to `target`.
fn tooltip_updates(target: Option<&HoverTarget>, window_width: f64) -> Updates {
    let Some((text, rect)) = target.and_then(|t| Some((t.tooltip.as_deref()?, t.rect))) else {
        return Updates::new().class("#tip", "show", false);
    };
    let estimated = text.chars().count() as f64 * TOOLTIP_CHAR_WIDTH + 20.0;
    let left = rect.x.min(window_width - estimated).max(4.0);
    let top = rect.y + rect.height + TOOLTIP_GAP;
    Updates::new()
        .text("#tip", text)
        .attr("#tip", "style", &format!("left:{left:.0}px;top:{top:.0}px"))
        .class("#tip", "show", true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hover(tooltip: Option<&str>, rect: Rect) -> HoverTarget {
        HoverTarget {
            tooltip: tooltip.map(str::to_owned),
            rect,
            element_id: None,
            action: None,
        }
    }

    #[test]
    fn tooltip_hides_without_a_tooltip_text() {
        let target = hover(None, Rect::default());
        assert_eq!(tooltip_updates(Some(&target), 800.0).len(), 1);
        assert_eq!(tooltip_updates(None, 800.0).len(), 1);
    }

    #[test]
    fn tooltip_shows_below_the_element_and_stays_inside_the_window() {
        let rect = Rect {
            x: 780.0,
            y: 100.0,
            width: 20.0,
            height: 20.0,
        };
        let updates = tooltip_updates(Some(&hover(Some("Coming soon"), rect)), 800.0);
        assert_eq!(updates.len(), 3);
    }
}
