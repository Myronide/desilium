//! The "Get new themes" page: tabs, search, paging, install and "use".
//!
//! Requests go to the store worker and come back through [`App::on_store`]; nothing here waits
//! for the network.

use std::{
    process::{Command, Stdio},
    thread,
    time::Instant,
};

use desilium_config_types::KDE_THEME;

use super::App;
use crate::{
    changes::{self, Recovery},
    i18n::{tr, tr_args},
    model::{Level, TOAST_LIFETIME, Toast},
    refresh::Refresh,
    registry::{Page, Value},
    store::{
        Done, Job, Kind, Load, StoreWorker,
        install::{installed_by_id, installed_names},
        ocs::site_url,
    },
};

impl App {
    /// Hands the app the background threads of the store; the page starts loading when it is
    /// the one on screen.
    pub fn set_store_worker(&mut self, worker: StoreWorker) {
        self.themes_worker = Some(worker);
        if self.model.page == Page::Themes {
            self.store_enter();
        }
    }

    /// Reads what is installed (names of the tab's kind, and which store entry each came from).
    pub(super) fn refresh_installed(&mut self) {
        let kind = self.model.store.kind;
        self.model.store.installed = installed_names(kind, &self.themes_dirs);
        self.model.store.by_id = installed_by_id(&self.themes_dirs);
    }

    /// The page was opened: load the first results once.
    pub(super) fn store_enter(&mut self) {
        self.refresh_installed();
        if self.model.store.load == Load::Idle {
            self.store_load();
        }
    }

    /// Asks the worker for the current tab, search and page.
    fn store_load(&mut self) {
        let state = &mut self.model.store;
        state.seq += 1;
        let job = Job::Search {
            seq: state.seq,
            kind: state.kind,
            query: state.query.clone(),
            page: state.page,
        };
        match &self.themes_worker {
            Some(worker) if worker.submit(job) => state.load = Load::Loading,
            Some(_) => {
                state.load = Load::Failed(tr(
                    "ui.store.err_stopped",
                    "The download helper stopped. Restart the settings.",
                ));
            }
            // Screenshots and tests have no network helper.
            None => state.load = Load::Idle,
        }
    }

    fn store_reset_results(&mut self) {
        let state = &mut self.model.store;
        state.page = 0;
        state.listing = Default::default();
        state.load = Load::Idle;
    }

    /// An action of the page.
    pub(super) fn store_action(&mut self, name: &str, arg: &str, now: Instant) -> Refresh {
        match name {
            "store-tab" => {
                let Some(kind) = Kind::from_slug(arg) else {
                    return Refresh::default();
                };
                if kind != self.model.store.kind {
                    self.model.store.kind = kind;
                    self.model.store.query.clear();
                    self.model.store.draft.clear();
                    self.store_reset_results();
                    self.refresh_installed();
                    self.store_load();
                }
                Refresh::main()
            }
            "store-page" => {
                let state = &mut self.model.store;
                match arg {
                    "next" if state.has_next() => state.page += 1,
                    "prev" if state.has_previous() => state.page -= 1,
                    _ => return Refresh::default(),
                }
                self.store_load();
                Refresh::main()
            }
            "store-reload" => {
                self.store_load();
                Refresh::main()
            }
            "store-install" => self.store_install(arg),
            "store-use" => self.store_use(arg, now),
            "store-site" => {
                self.open_site(self.model.store.kind);
                Refresh::main()
            }
            _ => Refresh::default(),
        }
    }

    /// Enter in the search field of the page.
    pub(super) fn store_search(&mut self) -> Refresh {
        let query = self.model.store.draft.trim().to_owned();
        self.model.store.query = query;
        self.model.store.page = 0;
        self.store_load();
        Refresh::main()
    }

    fn store_install(&mut self, arg: &str) -> Refresh {
        let state = &self.model.store;
        let Some(entry) = arg
            .parse::<usize>()
            .ok()
            .and_then(|index| state.listing.entries.get(index))
            .cloned()
        else {
            return Refresh::default();
        };
        if state.installing.is_some() {
            return Refresh::default();
        }
        let kind = state.kind;
        let queued = self.themes_dirs.can_install()
            && self.themes_worker.as_ref().is_some_and(|worker| {
                worker.submit(Job::Install {
                    kind,
                    entry: entry.clone(),
                })
            });
        if queued {
            self.model.store.installing = Some(entry.id);
            self.model.notice = None;
        } else {
            let message = tr(
                "ui.store.err_cannot_install",
                "Installing is not possible right now (no home directory or no helper).",
            );
            self.notice(Level::Error, message, Some(Recovery::Dismiss));
        }
        Refresh::main()
    }

    /// Selects an installed theme.
    fn store_use(&mut self, name: &str, now: Instant) -> Refresh {
        let state = &self.model.store;
        // Only names found on disk can be chosen, whatever the action's argument says.
        if !state.installed.iter().any(|installed| installed == name) {
            return Refresh::default();
        }
        match state.kind {
            Kind::Icons => {
                self.apply_by_id("appearance.icon_theme", Value::Text(name.to_owned()), now)
            }
            Kind::Cursors => {
                self.apply_by_id("appearance.cursor_theme", Value::Text(name.to_owned()), now)
            }
            Kind::Colours => {
                let summary = tr_args(
                    "ui.store.scheme_set",
                    "Colour scheme set to {name}",
                    &[("name", name)],
                );
                let scheme = name.to_owned();
                let edited = changes::edit_appearance(&mut self.store, summary, |appearance| {
                    appearance.theme = KDE_THEME.to_owned();
                    appearance.kde_scheme = Some(scheme);
                });
                match edited {
                    Ok(outcome) => self.finish_change(outcome, now),
                    Err(error) => {
                        self.fail(&error, None);
                        Refresh::main()
                    }
                }
            }
        }
    }

    /// Opens the store's web page of `kind` in the browser (`xdg-open`, fixed arguments).
    fn open_site(&mut self, kind: Kind) {
        let spawned = Command::new("xdg-open")
            .arg(site_url(kind))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        match spawned {
            Ok(mut child) => {
                thread::spawn(move || {
                    let _ = child.wait();
                });
            }
            Err(error) => {
                let message = tr_args(
                    "ui.store.err_browser",
                    "The browser could not be started ({error}).",
                    &[("error", &error.to_string())],
                );
                self.notice(Level::Error, message, Some(Recovery::Dismiss));
            }
        }
    }

    /// A background job of the store finished.
    pub fn on_store(&mut self, done: Done, now: Instant) -> Refresh {
        match done {
            Done::Listing { seq, outcome } => {
                if seq != self.model.store.seq {
                    // An answer to a search that was replaced in the meantime.
                    return Refresh::default();
                }
                match outcome {
                    Ok(listing) => {
                        self.model.store.listing = listing;
                        self.model.store.load = Load::Ready;
                    }
                    Err(error) => {
                        tracing::warn!(%error, "theme store search failed");
                        self.model.store.load = Load::Failed(error.message());
                    }
                }
                self.refresh_installed();
                Refresh::main()
            }
            Done::Installed { kind, id, outcome } => {
                if self.model.store.installing.as_deref() == Some(id.as_str()) {
                    self.model.store.installing = None;
                }
                match outcome {
                    Ok(names) => {
                        self.refresh_installed();
                        if kind == Kind::Cursors {
                            self.model.cursor_themes.extend(names.iter().cloned());
                            self.model.cursor_themes.sort();
                            self.model.cursor_themes.dedup();
                        }
                        self.model.toast = Some(Toast {
                            message: tr_args(
                                "ui.store.installed",
                                "Installed {names}",
                                &[("names", &names.join(", "))],
                            ),
                            undo: None,
                            expires: now + TOAST_LIFETIME,
                        });
                    }
                    Err(error) => {
                        tracing::warn!(%error, "theme store install failed");
                        self.notice(Level::Error, error.message(), Some(Recovery::Dismiss));
                    }
                }
                Refresh::main()
            }
        }
    }
}
