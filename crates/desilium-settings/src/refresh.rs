//! What part of the document an event handler changed, and how to bring the document up to date.

use std::time::Instant;

use desilium_ui::{UiError, UiSurface, Updates};

use crate::{
    app::App,
    view::{main_html, nav_html, toast_html},
};

/// Parts of the document that need to be rebuilt. Cheapest first.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Refresh {
    /// The theme (accent, light/dark) changed: re-derive the document tokens.
    pub theme: bool,
    /// Sidebar selection changed.
    pub nav: bool,
    /// Header, banner and page content changed.
    pub main: bool,
    /// Only the "last change" row changed.
    pub toast: bool,
    /// Empty the search field.
    pub clear_search: bool,
    /// Put the caret into the search field.
    pub focus_search: bool,
    /// Put the caret into the custom colour field after rebuilding.
    pub focus_hex: bool,
    /// A text field became empty (`false`) or non-empty (`true`): toggles its placeholder.
    pub field: Option<(String, bool)>,
}

impl Refresh {
    /// Everything.
    pub fn all() -> Self {
        Self {
            theme: true,
            nav: true,
            main: true,
            ..Self::default()
        }
    }

    /// Page content and sidebar.
    pub fn page() -> Self {
        Self {
            nav: true,
            main: true,
            ..Self::default()
        }
    }

    /// Page content only.
    pub fn main() -> Self {
        Self {
            main: true,
            ..Self::default()
        }
    }

    /// Only the toast row.
    pub fn toast() -> Self {
        Self {
            toast: true,
            ..Self::default()
        }
    }

    /// Combines two refreshes.
    pub fn merge(self, other: Self) -> Self {
        Self {
            theme: self.theme || other.theme,
            nav: self.nav || other.nav,
            main: self.main || other.main,
            toast: self.toast || other.toast,
            clear_search: self.clear_search || other.clear_search,
            focus_search: self.focus_search || other.focus_search,
            focus_hex: self.focus_hex || other.focus_hex,
            field: other.field.or(self.field),
        }
    }

    /// True when nothing needs to be done.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Brings `ui` up to date with the app state.
    pub fn apply(&self, app: &App, ui: &mut UiSurface, now: Instant) -> Result<(), UiError> {
        if self.theme {
            ui.set_theme(app.theme());
        }
        let ctx = app.ctx(now);
        let mut updates = Updates::new();
        if self.nav {
            updates = updates.children("#nav", &nav_html(&ctx));
        }
        if self.main {
            updates = updates.children("#main", &main_html(&ctx));
        } else if self.toast {
            updates = updates.children(
                "#toast",
                &toast_html(
                    ctx.model.toast.as_ref(),
                    app.model().page.has_preview()
                        || app.model().page == crate::registry::Page::Desktop,
                ),
            );
        }
        ui.apply(updates)?;
        ui.set_class("#app", "compact", ctx.model.compact)?;
        self.apply_input_state(ui)
    }

    fn apply_input_state(&self, ui: &mut UiSurface) -> Result<(), UiError> {
        if self.clear_search {
            ui.set_input_value("#search", "")?;
            ui.set_class("#searchbox", "filled", false)?;
        }
        if self.focus_search {
            ui.focus("#search")?;
        }
        if self.focus_hex {
            ui.focus("#hex")?;
        }
        if let Some((key, filled)) = &self.field {
            ui.set_class(&crate::view::field_selector(key), "filled", *filled)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_is_a_union() {
        let merged = Refresh::toast().merge(Refresh::page());
        assert!(merged.toast && merged.nav && merged.main && !merged.theme);
        assert!(Refresh::default().is_empty());
        assert!(!merged.is_empty());
    }
}
