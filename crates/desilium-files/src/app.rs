use crate::{
    backend::{self, Command, Event, Worker},
    location::Location,
    model::{Browser, Sort},
    places::Places,
};
use desilium_ui::{Key, KeyEvent, UiEvent};
use std::path::PathBuf;

pub enum Dialog {
    Question {
        message: String,
        choices: Vec<String>,
        reply: std::sync::mpsc::SyncSender<Option<i32>>,
    },
    NewFolder(String),
    Rename(String),
    Connect(String),
    Trash,
    Authenticate(backend::auth::Challenge, backend::auth::Credentials),
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Refresh {
    None,
    Items,
    All,
}
pub struct App {
    pub browser: Browser,
    pub home: PathBuf,
    pub places: Places,
    pub places_path: PathBuf,
    pub places_healthy: bool,
    pub dialog: Option<Dialog>,
    pub clipboard: Option<(Location, bool)>,
    pub worker: Worker,
}
impl App {
    pub fn new(home: PathBuf, start: Location, places_path: PathBuf, worker: Worker) -> Self {
        let loaded = Places::load(&places_path);
        let healthy = loaded.is_ok();
        let mut browser = Browser::new(start);
        let places = match loaded {
            Ok(places) => places,
            Err(error) => {
                browser.notice = error;
                Places::default()
            }
        };
        Self {
            browser,
            home,
            places,
            places_path,
            places_healthy: healthy,
            dialog: None,
            clipboard: None,
            worker,
        }
    }
    pub fn load(&mut self) {
        self.browser.generation = self.browser.generation.wrapping_add(1);
        self.browser.busy = true;
        let command = Command::List {
            location: self.browser.current.clone(),
            generation: self.browser.generation,
        };
        self.submit(command);
    }
    pub fn navigate(&mut self, location: Location) {
        self.browser.navigate(location);
        self.browser.entries.clear();
        self.load();
    }
    fn submit(&mut self, command: Command) {
        if let Err(error) = self.worker.send(command) {
            self.browser.notice = error;
            self.browser.busy = false;
        }
    }
    pub fn backend(&mut self, event: Event) -> Refresh {
        match event {
            Event::Listed {
                location,
                generation,
                result,
            } => {
                if generation != self.browser.generation || location != self.browser.current {
                    return Refresh::None;
                }
                self.browser.busy = false;
                let selected = self.browser.selection().map(|entry| entry.location.clone());
                match result {
                    Ok(entries) => {
                        self.browser.loaded = true;
                        self.browser.entries = entries;
                        self.browser.page = self
                            .browser
                            .page
                            .min(self.browser.visible().len().saturating_sub(1) / 100);
                        self.browser.selected = selected.and_then(|location| {
                            self.browser
                                .entries
                                .iter()
                                .position(|entry| entry.location == location)
                        });
                    }
                    Err(error) => {
                        self.browser.loaded = false;
                        self.browser.entries.clear();
                        self.browser.selected = None;
                        self.browser.notice = error;
                    }
                }
            }
            Event::Devices(devices) => self.browser.devices = devices,
            Event::Navigate(location) => self.navigate(location),
            Event::Refresh => {
                if !self.browser.busy {
                    self.load();
                }
            }
            Event::Finished(result) => {
                self.browser.notice = match result {
                    Ok(message) => message,
                    Err(error) => error,
                };
                self.browser.busy = false;
                self.load();
            }
            Event::Question {
                message,
                choices,
                reply,
            } => {
                self.dialog = Some(Dialog::Question {
                    message,
                    choices,
                    reply,
                })
            }
            Event::Authenticate(challenge) => {
                let credentials = backend::auth::Credentials {
                    username: challenge.username.clone(),
                    domain: challenge.domain.clone(),
                    password: String::new(),
                };
                self.dialog = Some(Dialog::Authenticate(challenge, credentials));
            }
        }
        Refresh::All
    }
    pub fn event(&mut self, event: &UiEvent) -> Refresh {
        match event {
            UiEvent::Action(action) => {
                self.action(&action.action, action.arg.as_deref().unwrap_or_default())
            }
            UiEvent::Submit { action, value } if action == "location" => {
                self.go_text(value);
                Refresh::All
            }
            UiEvent::Submit { action, .. } if action == "dialog" => {
                self.confirm();
                Refresh::All
            }
            UiEvent::TextChanged { key, value, .. } => {
                self.text(key.as_deref().unwrap_or_default(), value)
            }
            _ => Refresh::None,
        }
    }
    fn text(&mut self, key: &str, value: &str) -> Refresh {
        if key == "search" {
            self.browser.query = value.into();
            self.browser.page = 0;
            return Refresh::Items;
        }
        match &mut self.dialog {
            Some(Dialog::NewFolder(text) | Dialog::Rename(text) | Dialog::Connect(text))
                if key == "dialog" =>
            {
                *text = value.into()
            }
            Some(Dialog::Authenticate(_, credentials)) => match key {
                "username" => credentials.username = value.into(),
                "domain" => credentials.domain = value.into(),
                "password" => credentials.password = value.into(),
                _ => {}
            },
            _ => {}
        }
        Refresh::None
    }
    pub fn action(&mut self, action: &str, arg: &str) -> Refresh {
        match action {
            "go" => self.go_text(arg),
            "back" => {
                if self.browser.back() {
                    self.load();
                }
            }
            "forward" => {
                if self.browser.forward() {
                    self.load();
                }
            }
            "up" => {
                if let Some(parent) = self.browser.current.parent() {
                    self.navigate(parent);
                }
            }
            "refresh" => {
                self.load();
                self.submit(Command::Devices);
            }
            "hidden" => self.browser.show_hidden = !self.browser.show_hidden,
            "page-next" => {
                self.browser.page = (self.browser.page + 1)
                    .min(self.browser.visible().len().saturating_sub(1) / 100)
            }
            "page-prev" => self.browser.page = self.browser.page.saturating_sub(1),
            "sort" => {
                self.browser.page = 0;
                self.browser.sort = match arg {
                    "size" => Sort::Size,
                    "modified" => Sort::Modified,
                    _ => Sort::Name,
                }
            }
            "select" => {
                let index = arg
                    .parse()
                    .ok()
                    .filter(|index| self.browser.entries.get(*index).is_some());
                // Clicking the selected row again opens it (there is no double click).
                if index.is_some() && index == self.browser.selected {
                    self.open(arg);
                } else {
                    self.browser.selected = index;
                }
            }
            "open" => self.open(arg),
            "pin" => self.pin(),
            "unpin" => self.unpin(arg),
            "reset-places" => self.reset_places(),
            "mount" => {
                self.browser.busy = true;
                self.submit(Command::Mount(arg.into()));
            }
            "unmount" => {
                self.browser.busy = true;
                self.submit(Command::Unmount(arg.into()));
            }
            "connect" => self.dialog = Some(Dialog::Connect("sftp://".into())),
            "new-folder" => self.dialog = Some(Dialog::NewFolder(String::new())),
            "rename" => {
                if let Some(entry) = self.browser.selection() {
                    self.dialog = Some(Dialog::Rename(entry.name.clone()));
                }
            }
            "trash" => {
                if self.browser.selection().is_some() {
                    self.dialog = Some(Dialog::Trash);
                }
            }
            "copy" | "cut" => {
                if let Some(entry) = self.browser.selection() {
                    self.clipboard = Some((entry.location.clone(), action == "cut"));
                    self.browser.notice = "Ready to paste into a folder.".into();
                }
            }
            "paste" => self.paste(),
            "answer" => {
                if let Some(Dialog::Question { reply, choices, .. }) = self.dialog.take() {
                    let choice = arg
                        .parse::<usize>()
                        .ok()
                        .filter(|index| *index < choices.len())
                        .and_then(|index| i32::try_from(index).ok());
                    let _ = reply.send(choice);
                }
            }
            "confirm" => self.confirm(),
            "cancel" => {
                self.cancel_dialog();
            }
            "cancel-job" => {
                self.worker.cancel();
                self.cancel_dialog();
            }
            "dismiss" => self.browser.notice.clear(),
            _ => {}
        }
        Refresh::All
    }
    fn open(&mut self, arg: &str) {
        let index = arg.parse::<usize>().ok().or(self.browser.selected);
        let entry = index
            .and_then(|index| self.browser.entries.get(index))
            .cloned();
        if let Some(entry) = entry {
            if entry.directory {
                self.navigate(entry.location);
            } else {
                self.submit(Command::Open(entry.location));
            }
        }
    }
    fn go_text(&mut self, text: &str) {
        let text = text.trim();
        if text.is_empty() {
            self.browser.notice = "Enter a folder path or server address.".into();
            return;
        }
        let relative = !text.contains("://") && !text.starts_with(['/', '~']);
        let parsed = if relative {
            let base = self
                .browser
                .current
                .path()
                .unwrap_or_else(|| self.home.clone());
            Location::from_path(&base.join(text))
        } else {
            Location::parse(text, &self.home)
        };
        match parsed {
            Ok(location) => self.navigate(location),
            Err(error) => self.browser.notice = error,
        }
    }
    fn pin(&mut self) {
        if !self.browser.loaded {
            self.browser.notice = "Open a readable folder before pinning it.".into();
            return;
        }
        if !self.places_healthy {
            self.browser.notice =
                "Reset saved places before adding pins; your damaged file is preserved.".into();
            return;
        }
        let before = self.places.clone();
        let name = self
            .browser
            .current
            .path()
            .and_then(|path| {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| self.browser.current.label());
        self.places.toggle(&self.browser.current, name);
        if let Err(error) = self.places.save(&self.places_path) {
            self.places = before;
            self.browser.notice = error;
        }
    }
    fn unpin(&mut self, uri: &str) {
        let before = self.places.clone();
        self.places.pins.retain(|pin| pin.uri != uri);
        if let Err(error) = self.places.save(&self.places_path) {
            self.places = before;
            self.browser.notice = error;
        }
    }
    fn reset_places(&mut self) {
        let backup = self.places_path.with_extension("backup.toml");
        if backup.exists() {
            self.browser.notice =
                "A saved-places backup already exists; it was left untouched.".into();
            return;
        }
        match std::fs::rename(&self.places_path, &backup) {
            Ok(()) => {
                self.places = Places::default();
                self.places_healthy = true;
                self.browser.notice =
                    "Saved places reset. The original file was kept as places.backup.toml.".into();
            }
            Err(error) => self.browser.notice = error.to_string(),
        }
    }
    fn paste(&mut self) {
        let Some((source, cut)) = self.clipboard.clone() else {
            return;
        };
        self.browser.busy = true;
        self.submit(Command::Transfer {
            source,
            folder: self.browser.current.clone(),
            cut,
        });
    }
    fn confirm(&mut self) {
        let Some(dialog) = self.dialog.take() else {
            return;
        };
        let command = match dialog {
            Dialog::Question { reply, .. } => {
                let _ = reply.send(None);
                return;
            }
            Dialog::Connect(address) => {
                self.go_text(&address);
                return;
            }
            Dialog::NewFolder(name) => Command::CreateFolder(self.browser.current.clone(), name),
            Dialog::Rename(name) => {
                let Some(entry) = self.browser.selection() else {
                    return;
                };
                Command::Rename(entry.location.clone(), name)
            }
            Dialog::Trash => {
                let Some(entry) = self.browser.selection() else {
                    return;
                };
                Command::Trash(entry.location.clone())
            }
            Dialog::Authenticate(challenge, credentials) => {
                let _ = challenge.reply.send(Some(credentials));
                return;
            }
        };
        self.browser.busy = true;
        self.submit(command);
    }
    fn cancel_dialog(&mut self) {
        match self.dialog.take() {
            Some(Dialog::Authenticate(challenge, _)) => {
                let _ = challenge.reply.send(None);
            }
            Some(Dialog::Question { reply, .. }) => {
                let _ = reply.send(None);
            }
            _ => {}
        }
    }
    pub fn key(&mut self, event: &KeyEvent, typing: bool) -> Refresh {
        if event.key == Key::Escape {
            self.cancel_dialog();
            return Refresh::All;
        }
        if typing {
            return Refresh::None;
        }
        if matches!(event.key, Key::ArrowUp | Key::ArrowDown) && !event.modifiers.alt {
            let rows = self.browser.visible();
            let current = rows
                .iter()
                .position(|(index, _)| Some(*index) == self.browser.selected);
            let at = match (current, event.key == Key::ArrowDown) {
                (Some(index), true) => (index + 1).min(rows.len().saturating_sub(1)),
                (Some(index), false) => index.saturating_sub(1),
                _ => 0,
            };
            self.browser.selected = rows.get(at).map(|(index, _)| *index);
            self.browser.page = at / 100;
            return Refresh::Items;
        }
        let action = match (&event.key, event.modifiers.ctrl, event.modifiers.alt) {
            (Key::ArrowLeft, _, true) => "back",
            (Key::ArrowRight, _, true) => "forward",
            (Key::ArrowUp, _, true) => "up",
            (Key::Character(key), true, _) if key.eq_ignore_ascii_case("h") => "hidden",
            (Key::Character(key), true, _) if key.eq_ignore_ascii_case("c") => "copy",
            (Key::Character(key), true, _) if key.eq_ignore_ascii_case("x") => "cut",
            (Key::Character(key), true, _) if key.eq_ignore_ascii_case("v") => "paste",
            (Key::Character(key), true, _) if key.eq_ignore_ascii_case("r") => "refresh",
            (Key::Enter, _, _) => "open",
            (Key::Delete, _, _) => "trash",
            _ => return Refresh::None,
        };
        self.action(action, "")
    }
}
