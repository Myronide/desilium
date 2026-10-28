//! Builders for catalog keys; the scheme itself is documented in [`super`].

/// Fields of a setting that carry text.
pub const SETTING_FIELDS: [&str; 4] = ["title", "description", "keywords", "placeholder"];

/// Lowercase words of `text` joined by `_`: "Accent color" becomes `accent_color`.
pub fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    out.trim_end_matches('_').to_owned()
}

/// `page.<slug>`: sidebar label and page heading.
pub fn page(slug: &str) -> String {
    format!("page.{slug}")
}

/// `page.<slug>.subtitle`: the sentence under the heading.
pub fn page_subtitle(slug: &str) -> String {
    format!("page.{slug}.subtitle")
}

/// `section.<page>.<slug>`: card title; `title` is the English source title.
pub fn section(page_slug: &str, title: &str) -> String {
    format!("section.{page_slug}.{}", slug(title))
}

/// `setting.<setting id>.<field>`.
pub fn setting(id: &str, field: &str) -> String {
    format!("setting.{id}.{field}")
}

/// `choice.<setting id>.<value>`: label of one option of a segmented control or dropdown.
pub fn choice(id: &str, value: &str) -> String {
    format!("choice.{id}.{value}")
}

/// `action.<action name>`: label of a shortcut action, named as in the configuration file.
pub fn action(name: &str) -> String {
    format!("action.{name}")
}

/// Checks that `key` follows the naming scheme; the error says what is wrong.
pub fn check(key: &str) -> Result<(), String> {
    let parts: Vec<&str> = key.split('.').collect();
    let clean = parts.iter().all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    });
    if !clean {
        return Err("segments are lowercase letters, digits, '_' or '-', separated by '.'".into());
    }
    let shape_ok = match parts.as_slice() {
        ["page", _] | ["action", _] => true,
        ["page", _, "subtitle"] => true,
        ["section", _, _] | ["ui", _, _] => true,
        // A setting id is `group.name` or, for nested tables, `group.table.name`.
        ["setting", _, _, field] | ["setting", _, _, _, field] => SETTING_FIELDS.contains(field),
        ["choice", _, _, _] | ["choice", _, _, _, _] => true,
        _ => false,
    };
    if shape_ok {
        Ok(())
    } else {
        Err(
            "does not match page.<slug>[.subtitle], section.<page>.<slug>, \
             setting.<id>.<field>, choice.<id>.<value>, action.<name> or ui.<area>.<name>"
                .into(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_lowercase_words() {
        assert_eq!(slug("Accent color"), "accent_color");
        assert_eq!(slug("Panel & Dock"), "panel_dock");
        assert_eq!(slug("  Key repeat! "), "key_repeat");
    }

    #[test]
    fn builders_follow_the_documented_scheme() {
        assert_eq!(page("panel"), "page.panel");
        assert_eq!(
            section("appearance", "Accent color"),
            "section.appearance.accent_color"
        );
        assert_eq!(
            setting("appearance.theme", "title"),
            "setting.appearance.theme.title"
        );
        assert_eq!(
            choice("shell.panel_visibility", "auto-hide"),
            "choice.shell.panel_visibility.auto-hide"
        );
        assert_eq!(action("snap-left"), "action.snap-left");
    }

    #[test]
    fn the_checker_accepts_the_scheme_and_refuses_everything_else() {
        for good in [
            "page.about",
            "page.about.subtitle",
            "section.panel.layout",
            "setting.shell.mode.description",
            "choice.shell.mode.dock",
            "action.quit",
            "ui.toast.undo",
        ] {
            assert_eq!(check(good), Ok(()), "{good}");
        }
        for bad in [
            "Page.about",
            "page",
            "page.a b",
            "setting.shell.mode",
            "setting.shell.mode.colour",
            "ui.toast",
            "misc.x.y",
            "ui..x",
        ] {
            assert!(check(bad).is_err(), "{bad}");
        }
    }
}
