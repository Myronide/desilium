use crate::{
    app::{App, Refresh},
    backend::{Command, Event, Worker},
    location::Location,
    places::Places,
    view,
};
use calloop::{EventLoop, Interest, Mode, PostAction, channel, generic::Generic};
use desilium_ui::{
    SurfaceOptions, UiSurface,
    wayland::{WindowConfig, WindowEvent, WindowId, XdgWindowHost},
};
use std::os::fd::AsFd;
use std::{io, path::PathBuf, time::Instant};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Ui(#[from] desilium_ui::UiError),
    #[error(transparent)]
    Io(#[from] io::Error),
}
struct Runtime {
    app: App,
    host: XdgWindowHost,
    window: WindowId,
    pending: Vec<Event>,
    store: desilium_config::ConfigStore,
    watch: Option<inotify::Inotify>,
    quit: bool,
}

pub fn run(start: Option<String>) -> Result<(), Error> {
    let home = gio::glib::home_dir();
    let location =
        Location::parse(start.as_deref().unwrap_or("~"), &home).map_err(Error::Message)?;
    let (sender, receiver) = channel::channel();
    let worker = Worker::spawn(sender).map_err(Error::Message)?;
    let mut app = App::new(home, location, Places::default_path(), worker);
    app.load();
    app.worker.send(Command::Devices).map_err(Error::Message)?;
    let mut host = XdgWindowHost::connect()?;
    let mut options = SurfaceOptions::new(1120, 760);
    let store =
        desilium_config::ConfigStore::open(desilium_config::ConfigStore::default_dir()).store;
    options.theme = theme(&store.get().appearance);
    let ui = UiSurface::new(&view::document(&app), &[view::CSS], options)?;
    let mut config = WindowConfig::new("Doria — Files", "org.desilium.Doria", (1120, 760));
    config.min_size = Some((860, 520));
    let window = host.add_window(config, ui)?;
    let mut state = Runtime {
        app,
        host,
        window,
        pending: Vec::new(),
        store,
        watch: None,
        quit: false,
    };
    let mut event_loop: EventLoop<Runtime> =
        EventLoop::try_new().map_err(|error| Error::Message(error.to_string()))?;
    let fd = state.host.poll_fd().try_clone_to_owned()?;
    event_loop
        .handle()
        .insert_source(Generic::new(fd, Interest::READ, Mode::Level), |_, _, _| {
            Ok(PostAction::Continue)
        })
        .map_err(|error| Error::Message(error.to_string()))?;
    event_loop
        .handle()
        .insert_source(receiver, |event, _, state: &mut Runtime| {
            if let channel::Event::Msg(event) = event {
                state.pending.push(event);
            }
        })
        .map_err(|error| Error::Message(error.to_string()))?;
    state.install_config_watch(&event_loop.handle())?;
    state.pump()?;
    while !state.quit {
        event_loop
            .dispatch(state.host.next_timeout(Instant::now()), &mut state)
            .map_err(|error| Error::Message(error.to_string()))?;
        state.pump()?;
    }
    Ok(())
}
impl Runtime {
    fn install_config_watch(
        &mut self,
        handle: &calloop::LoopHandle<'static, Runtime>,
    ) -> Result<(), Error> {
        let directory = desilium_config::ConfigStore::default_dir();
        use std::os::unix::fs::DirBuilderExt;
        let opened = (|| -> io::Result<inotify::Inotify> {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(directory)?;
            let watch = inotify::Inotify::init()?;
            watch.watches().add(
                desilium_config::ConfigStore::default_dir(),
                inotify::WatchMask::MOVED_TO
                    | inotify::WatchMask::CLOSE_WRITE
                    | inotify::WatchMask::CREATE,
            )?;
            Ok(watch)
        })();
        let watch = match opened {
            Ok(watch) => watch,
            Err(error) => {
                self.app.browser.notice =
                    format!("Live appearance updates are unavailable: {error}");
                return Ok(());
            }
        };
        let fd = watch.as_fd().try_clone_to_owned()?;
        self.watch = Some(watch);
        handle
            .insert_source(
                Generic::new(fd, Interest::READ, Mode::Level),
                |_, _, state: &mut Runtime| {
                    state.reload_theme();
                    Ok(PostAction::Continue)
                },
            )
            .map_err(|error| Error::Message(error.to_string()))?;
        Ok(())
    }
    fn reload_theme(&mut self) {
        if let Some(watch) = &mut self.watch {
            let mut buffer = [0; 4096];
            let _ = watch.read_events(&mut buffer);
        }
        if let Some(desilium_config::ReloadOutcome::Applied { .. }) = self.store.reload_if_changed()
        {
            let theme = theme(&self.store.get().appearance);
            self.host.update(self.window, |ui| ui.set_theme(&theme));
        }
    }
    fn pump(&mut self) -> Result<(), Error> {
        self.host.dispatch(Instant::now())?;
        let mut refresh = Refresh::None;
        for event in self.host.take_events() {
            let next = match event {
                WindowEvent::Ui { event, .. } => self.app.event(&event),
                WindowEvent::Key { event, .. } => {
                    if event.modifiers.ctrl
                        && matches!(&event.key,desilium_ui::Key::Character(key) if key.eq_ignore_ascii_case("l") || key.eq_ignore_ascii_case("f"))
                    {
                        let selector = if matches!(&event.key,desilium_ui::Key::Character(key) if key.eq_ignore_ascii_case("l"))
                        {
                            "#address"
                        } else {
                            "#search"
                        };
                        self.host.update(self.window, |ui| ui.focus(selector));
                        continue;
                    }
                    let typing = self
                        .host
                        .surface(self.window)
                        .is_some_and(UiSurface::wants_text_input);
                    self.app.key(&event, typing)
                }
                WindowEvent::CloseRequested(_) => {
                    self.quit = true;
                    Refresh::None
                }
                _ => Refresh::None,
            };
            refresh = merge(refresh, next);
        }
        for event in std::mem::take(&mut self.pending) {
            refresh = merge(refresh, self.app.backend(event));
        }
        self.draw(refresh)?;
        Ok(())
    }
    fn draw(&mut self, refresh: Refresh) -> Result<(), Error> {
        let app = &self.app;
        let result = match refresh {
            Refresh::None => return Ok(()),
            Refresh::All => self.host.update(self.window, |ui| {
                ui.replace_children("#root", &view::html(app))
            }),
            Refresh::Items => self.host.update(self.window, |ui| {
                ui.replace_children("#listing", &view::listing(app))?;
                ui.replace_children("#status", &view::status(app))
            }),
        };
        if let Some(result) = result {
            result?;
        }
        Ok(())
    }
}
fn merge(left: Refresh, right: Refresh) -> Refresh {
    if left == Refresh::All || right == Refresh::All {
        return Refresh::All;
    }
    if left == Refresh::Items || right == Refresh::Items {
        return Refresh::Items;
    }
    Refresh::None
}
fn theme(appearance: &desilium_config::AppearanceSettings) -> desilium_theme::Theme {
    let mode = if appearance.theme == "desilium-light" {
        desilium_theme::Mode::Light
    } else {
        desilium_theme::Mode::Dark
    };
    appearance
        .accent
        .as_deref()
        .and_then(|text| desilium_theme::Color::from_hex(text).ok())
        .map_or_else(
            || desilium_theme::Theme::for_mode(mode),
            |color| desilium_theme::Theme::derive_from_accent(color, mode),
        )
}
/// A deterministic UI render with a caller-supplied directory, used by screenshot tests.
pub fn render(path: PathBuf, output: PathBuf) -> Result<(), Error> {
    let (sender, _receiver) = channel::channel();
    let worker = Worker::spawn(sender).map_err(Error::Message)?;
    let location = Location::from_path(&path).map_err(Error::Message)?;
    let mut app = App::new(
        path.clone(),
        location.clone(),
        path.join(".doria-unused-places"),
        worker,
    );
    app.browser.entries = crate::backend::files::list(&location, &gio::Cancellable::new())
        .map_err(|error| Error::Message(error.to_string()))?;
    let mut surface = UiSurface::new(
        &view::document(&app),
        &[view::CSS],
        SurfaceOptions::new(1120, 760),
    )?;
    let mut pixels = vec![0; 1120 * 760 * 4];
    surface.render(&mut pixels, 1120 * 4)?;
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let file = std::fs::File::create(output)?;
    let mut encoder = png::Encoder::new(file, 1120, 760);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .map_err(|error| Error::Message(error.to_string()))?
        .write_image_data(&pixels)
        .map_err(|error| Error::Message(error.to_string()))?;
    Ok(())
}
