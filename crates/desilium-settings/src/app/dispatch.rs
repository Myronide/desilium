//! Document events to behaviour.

use std::time::Instant;

use desilium_ui::{DragPhase, UiEvent};

use super::{App, Retry};
use crate::{
    changes::{self, ChangeOutcome, Recovery},
    i18n::{tr, tr_args},
    model::{AccentEditor, HIGHLIGHT_LIFETIME, Level, TOAST_LIFETIME, Toast},
    refresh::Refresh,
    registry::{self, Page, SettingDef, Value, parse_number},
    search::search,
};

/// "id=value" action arguments.
fn split_arg(arg: &str) -> Option<(&str, &str)> {
    arg.split_once('=')
}

impl App {
    /// Handles one document event.
    pub fn handle_event(&mut self, event: &UiEvent, now: Instant) -> Refresh {
        match event {
            UiEvent::Action(action) => {
                self.handle_action(&action.action, action.arg.as_deref(), now)
            }
            UiEvent::TextChanged { key, value, .. } => self.handle_text(key.as_deref(), value),
            UiEvent::Submit { action, value } => self.handle_submit(action, value, now),
            // Slider drags need the track's rectangle, which only the window host can measure; it
            // calls `range_drag` itself.
            UiEvent::Drag(_) => Refresh::default(),
        }
    }

    fn handle_action(&mut self, name: &str, arg: Option<&str>, now: Instant) -> Refresh {
        let arg = arg.unwrap_or_default();
        let previous = self.model.open_select.take();
        if name == "select-toggle" {
            return self.toggle_select(arg, previous);
        }
        let refresh = self.run_action(name, arg, now);
        if previous.is_some() {
            return refresh.merge(Refresh::main());
        }
        refresh
    }

    fn run_action(&mut self, name: &str, arg: &str, now: Instant) -> Refresh {
        match name {
            "nav" => self.navigate(arg),
            "goto" => self.goto(arg, now),
            "set" => self.set_from_arg(arg, now),
            "clear" => self.apply_by_id(arg, Value::Unset, now),
            "step" => self.step(arg, now),
            "reset" => self.reset_ids(&[arg], arg, now),
            "reset-section" => self.reset_section(arg, now),
            "reset-page" => self.reset_page(arg, now),
            "undo" => self.undo(now),
            "accent-apply" => self.apply_accent(now),
            "search-clear" => self.clear_search(),
            "notice-dismiss" => self.dismiss_notice(),
            "notice-reload" => self.reload_external(now),
            "notice-retry" => self.retry_last(now),
            "rebind" | "rebind-cancel" | "rebind-swap" | "shortcut-reset" => {
                self.rebind_action(name, arg, now)
            }
            _ if name.starts_with("store-") => self.store_action(name, arg, now),
            _ => Refresh::default(),
        }
    }

    fn handle_text(&mut self, key: Option<&str>, value: &str) -> Refresh {
        let Some(key) = key else {
            return Refresh::default();
        };
        let field = Refresh {
            field: Some((key.to_owned(), !value.is_empty())),
            ..Refresh::default()
        };
        match key {
            "search" => {
                self.model.query = value.to_owned();
                self.model.close_popups();
                Refresh::page().merge(field)
            }
            "accent-hex" => {
                self.model.accent_editor = Some(AccentEditor {
                    text: value.to_owned(),
                    error: None,
                });
                field
            }
            "store-search" => {
                self.model.store.draft = value.to_owned();
                field
            }
            _ => field,
        }
    }

    fn handle_submit(&mut self, action: &str, value: &str, now: Instant) -> Refresh {
        if action == "search" {
            return match search(&self.model.query).first() {
                Some(hit) => self.goto(hit.def.id, now),
                None => Refresh::default(),
            };
        }
        if action == "store-search" {
            self.model.store.draft = value.to_owned();
            return self.store_search();
        }
        if action == "accent-apply" {
            self.model.accent_editor = Some(AccentEditor {
                text: value.to_owned(),
                error: None,
            });
            return self.apply_accent(now);
        }
        match action.strip_prefix("text:") {
            Some(id) => self.apply_by_id(id, Value::Text(value.to_owned()), now),
            None => Refresh::default(),
        }
    }

    fn navigate(&mut self, slug: &str) -> Refresh {
        let Some(page) = Page::from_slug(slug) else {
            return Refresh::default();
        };
        self.model.page = page;
        self.model.query.clear();
        self.model.accent_editor = None;
        self.model.rebind = None;
        if page == Page::Themes {
            self.store_enter();
        }
        Refresh {
            clear_search: true,
            ..Refresh::page()
        }
    }

    fn goto(&mut self, id: &str, now: Instant) -> Refresh {
        let Some(def) = registry::find(id) else {
            return Refresh::default();
        };
        self.model.page = def.page;
        self.model.query.clear();
        self.model.highlight = Some((def.id, now + HIGHLIGHT_LIFETIME));
        Refresh {
            clear_search: true,
            ..Refresh::page()
        }
    }

    fn set_from_arg(&mut self, arg: &str, now: Instant) -> Refresh {
        let Some((id, text)) = split_arg(arg) else {
            return Refresh::default();
        };
        let Some(value) = registry::find(id).and_then(|def| def.value_from_text(text)) else {
            return Refresh::default();
        };
        self.apply_by_id(id, value, now)
    }

    /// A slider of setting `id` is being dragged: `fraction` is where the pointer is along its
    /// track (below 0 or above 1 outside it). The thumb follows the pointer; the value is stored
    /// once, when the button is released.
    pub fn range_drag(
        &mut self,
        id: &str,
        phase: DragPhase,
        fraction: f64,
        now: Instant,
    ) -> Refresh {
        let Some(def) = registry::find(id) else {
            return Refresh::default();
        };
        let target = match def.control {
            registry::Control::Stepper(range) => range.value_at(fraction),
            registry::Control::Slider(slider) => slider.stop_at(fraction),
            _ => return Refresh::default(),
        };
        match phase {
            DragPhase::Start | DragPhase::Move => {
                self.model.range_preview = Some((def.id, target));
                Refresh {
                    range: Some(def.id),
                    ..Refresh::default()
                }
            }
            DragPhase::End => {
                self.model.range_preview = None;
                self.apply_by_id(def.id, Value::Number(target), now)
            }
        }
    }

    fn step(&mut self, arg: &str, now: Instant) -> Refresh {
        let Some((id, text)) = split_arg(arg) else {
            return Refresh::default();
        };
        let (Some(def), Ok(delta)) = (registry::find(id), parse_number(text)) else {
            return Refresh::default();
        };
        let Value::Number(current) = (def.get)(self.store.get()) else {
            return Refresh::default();
        };
        self.apply_by_id(id, Value::Number(current + delta), now)
    }

    pub(super) fn apply_by_id(&mut self, id: &str, value: Value, now: Instant) -> Refresh {
        let Some(def) = registry::find(id) else {
            return Refresh::default();
        };
        match changes::set_value(&mut self.store, def, value.clone()) {
            Ok(outcome) => self.finish_change(outcome, now),
            Err(error) => {
                let retry = Retry {
                    setting: def.id,
                    value,
                };
                self.fail(&error, Some(retry));
                Refresh::main()
            }
        }
    }

    /// Records a successful change: toast with Undo, repair notice, theme and thumbnails.
    pub(super) fn finish_change(&mut self, outcome: ChangeOutcome, now: Instant) -> Refresh {
        self.retry = None;
        self.model.accent_editor = None;
        if !outcome.changed {
            return Refresh::main();
        }
        self.model.toast = Some(Toast {
            message: outcome.summary,
            undo: outcome.undo,
            expires: now + TOAST_LIFETIME,
        });
        self.model.notice = None;
        if !outcome.repairs.is_empty() {
            let text = outcome
                .repairs
                .iter()
                .map(|repair| repair.message.clone())
                .collect::<Vec<_>>()
                .join(" ");
            let notice = tr_args(
                "ui.notice.adjusted",
                "Adjusted for you: {text}",
                &[("text", &text)],
            );
            self.notice(Level::Info, notice, None);
        }
        Refresh::main().merge(self.sync())
    }

    fn reset_ids(&mut self, ids: &[&str], label: &str, now: Instant) -> Refresh {
        let defs: Vec<&SettingDef> = ids.iter().filter_map(|id| registry::find(id)).collect();
        let label = defs
            .first()
            .filter(|_| defs.len() == 1)
            .map_or_else(|| label.to_owned(), |def| def.title_text());
        self.reset_defs(&defs, &label, now)
    }

    fn reset_section(&mut self, arg: &str, now: Instant) -> Refresh {
        let Some((slug, title)) = arg.split_once('/') else {
            return Refresh::default();
        };
        let Some(page) = Page::from_slug(slug) else {
            return Refresh::default();
        };
        let section = registry::sections(page)
            .into_iter()
            .find(|s| s.title == title);
        match section {
            Some(section) => self.reset_defs(&section.settings, &section.title_text(), now),
            None => Refresh::default(),
        }
    }

    fn reset_page(&mut self, slug: &str, now: Instant) -> Refresh {
        let Some(page) = Page::from_slug(slug) else {
            return Refresh::default();
        };
        let defs: Vec<&SettingDef> = registry::for_page(page).collect();
        self.reset_defs(&defs, &page.title(), now)
    }

    fn reset_defs(&mut self, defs: &[&SettingDef], label: &str, now: Instant) -> Refresh {
        match changes::reset_settings(&mut self.store, defs, label) {
            Ok(outcome) => self.finish_change(outcome, now),
            Err(error) => {
                self.fail(&error, None);
                Refresh::main()
            }
        }
    }

    fn undo(&mut self, now: Instant) -> Refresh {
        let Some(toast) = self.model.toast.take() else {
            return Refresh::default();
        };
        let Some(target) = toast.undo else {
            return Refresh::toast();
        };
        match changes::undo(&mut self.store, &target) {
            Ok(()) => {
                self.model.toast = Some(Toast {
                    message: tr("ui.change.undone", "Change undone"),
                    undo: None,
                    expires: now + TOAST_LIFETIME,
                });
                self.model.notice = None;
                self.model.rebind = None;
                self.model.accent_editor = None;
                Refresh::main().merge(self.sync())
            }
            Err(error) => {
                let failure = changes::friendly_store(&error);
                self.notice(Level::Error, failure.message, Some(failure.recovery));
                Refresh::main()
            }
        }
    }

    /// Clicking the button of the open dropdown closes it; any other opens that one.
    fn toggle_select(&mut self, id: &str, previous: Option<&'static str>) -> Refresh {
        let Some(def) = registry::find(id) else {
            return Refresh::default();
        };
        if previous != Some(def.id) {
            self.model.open_select = Some(def.id);
        }
        Refresh::main()
    }

    fn apply_accent(&mut self, now: Instant) -> Refresh {
        let text = self
            .model
            .accent_editor
            .as_ref()
            .map(|editor| editor.text.clone())
            .unwrap_or_default();
        let Some(def) = registry::find("appearance.accent") else {
            return Refresh::default();
        };
        match def.normalize(Value::Text(text.clone())) {
            Ok(value) => self.apply_by_id(def.id, value, now),
            Err(invalid) => {
                self.model.accent_editor = Some(AccentEditor {
                    text,
                    error: Some(invalid.0),
                });
                Refresh {
                    focus_hex: true,
                    ..Refresh::main()
                }
            }
        }
    }

    fn clear_search(&mut self) -> Refresh {
        self.model.query.clear();
        Refresh {
            clear_search: true,
            ..Refresh::page()
        }
    }

    fn dismiss_notice(&mut self) -> Refresh {
        self.model.notice = None;
        self.retry = None;
        Refresh::main()
    }

    fn retry_last(&mut self, now: Instant) -> Refresh {
        self.model.notice = None;
        match self.retry.take() {
            Some(retry) => self.apply_by_id(retry.setting, retry.value, now),
            None => Refresh::main(),
        }
    }

    /// Picks up an edit of the settings file made by another program.
    pub fn reload_external(&mut self, now: Instant) -> Refresh {
        use desilium_config::ReloadOutcome;
        match self.store.reload_if_changed() {
            None => Refresh::default(),
            Some(ReloadOutcome::Applied { recovery }) => {
                self.model.toast = Some(Toast {
                    message: tr(
                        "ui.notice.updated_outside",
                        "Settings were updated outside this window",
                    ),
                    undo: None,
                    expires: now + TOAST_LIFETIME,
                });
                self.model.notice = None;
                if !recovery.is_empty() {
                    let text = recovery
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ");
                    let notice = tr_args(
                        "ui.notice.some_adjusted",
                        "Some values were adjusted: {text}",
                        &[("text", &text)],
                    );
                    self.notice(Level::Info, notice, None);
                }
                self.model.rebind = None;
                self.model.accent_editor = None;
                Refresh::main().merge(self.sync())
            }
            Some(ReloadOutcome::Rejected { reason }) => {
                let notice = tr_args(
                    "ui.notice.rejected",
                    "The settings file was changed but cannot be used ({reason}). \
                     Your last good settings stay active.",
                    &[("reason", &reason.to_string())],
                );
                self.notice(Level::Error, notice, Some(Recovery::Dismiss));
                Refresh::main()
            }
        }
    }

    /// Expires the toast and highlight when their time is up.
    pub fn tick(&mut self, now: Instant) -> Refresh {
        let mut refresh = Refresh::default();
        if self
            .model
            .toast
            .as_ref()
            .is_some_and(|toast| toast.expires <= now)
        {
            self.model.toast = None;
            refresh = refresh.merge(Refresh::toast());
        }
        if self.model.highlight.is_some_and(|(_, until)| until <= now) {
            self.model.highlight = None;
            refresh = refresh.merge(Refresh::main());
        }
        refresh
    }
}
