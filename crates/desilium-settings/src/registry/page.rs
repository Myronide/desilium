//! Pages and the sidebar navigation.

use crate::i18n::{keys, tr};

/// A settings page that exists today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    /// Theme, accent, wallpaper, motion, pointer.
    Appearance,
    /// Virtual desktops and applications.
    Desktop,
    /// Keyboard shortcuts.
    Keyboard,
    /// Pointer speed, scrolling and clicking of mice.
    Mouse,
    Panel,
    /// Download and select icon themes, cursor themes and colour schemes from the KDE Store.
    Themes,
    /// Version and system information.
    About,
}

impl Page {
    /// All pages in sidebar order.
    pub const ALL: [Page; 7] = [
        Page::Appearance,
        Page::Themes,
        Page::Desktop,
        Page::Panel,
        Page::Keyboard,
        Page::Mouse,
        Page::About,
    ];

    /// Page heading and sidebar label in the current language.
    pub fn title(self) -> String {
        tr(&keys::page(self.slug()), self.source_title())
    }

    /// Sentence under the heading in the current language.
    pub fn subtitle(self) -> String {
        tr(&keys::page_subtitle(self.slug()), self.source_subtitle())
    }

    /// English heading, the source of the translation.
    fn source_title(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Desktop => "Desktop",
            Page::Keyboard => "Keyboard",
            Page::Mouse => "Mouse",
            Page::Panel => "Panel & Dock",
            Page::Themes => "Get new themes",
            Page::About => "About",
        }
    }

    /// English sentence under the heading, the source of the translation.
    fn source_subtitle(self) -> &'static str {
        match self {
            Page::Appearance => "Make Desilium look the way you want. Changes apply instantly.",
            Page::Desktop => "Window focus, snapping, virtual desktops and the programs you start.",
            Page::Keyboard => "Key repeat and shortcuts for windows and desktops.",
            Page::Mouse => "Pointer speed, scrolling and clicking.",
            Page::Panel => "Choose a dock or taskbar and make it your own.",
            Page::Themes => {
                "Download icon themes, cursor themes and colour schemes from the KDE Store."
            }
            Page::About => "What is running on this computer.",
        }
    }

    /// Sidebar icon name.
    pub fn icon(self) -> &'static str {
        match self {
            Page::Appearance => "palette",
            Page::Desktop => "layers",
            Page::Keyboard => "keyboard",
            Page::Mouse => "mouse",
            Page::Panel => "layers",
            Page::Themes => "sparkles",
            Page::About => "info",
        }
    }

    /// Stable identifier used on the command line and in element ids.
    pub fn slug(self) -> &'static str {
        match self {
            Page::Appearance => "appearance",
            Page::Desktop => "desktop",
            Page::Keyboard => "keyboard",
            Page::Mouse => "mouse",
            Page::Panel => "panel",
            Page::Themes => "themes",
            Page::About => "about",
        }
    }

    /// Parses a slug (case-insensitive).
    pub fn from_slug(slug: &str) -> Option<Page> {
        let slug = slug.trim().to_ascii_lowercase();
        Page::ALL.into_iter().find(|page| page.slug() == slug)
    }

    /// True when the page shows the live preview card.
    pub fn has_preview(self) -> bool {
        self == Page::Appearance
    }
}

/// One line of the sidebar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavEntry {
    /// A page that exists.
    Page(Page),
    /// A planned page: shown disabled with a tooltip.
    Planned {
        /// Stable name, the `<slug>` of its translation key `page.<slug>`.
        slug: &'static str,
        /// English label.
        label: &'static str,
        /// Icon name.
        icon: &'static str,
    },
    /// A thin divider between groups.
    Separator,
}

/// Sidebar layout.
pub const NAV: [NavEntry; 11] = [
    NavEntry::Page(Page::Appearance),
    NavEntry::Page(Page::Themes),
    NavEntry::Page(Page::Desktop),
    NavEntry::Page(Page::Panel),
    NavEntry::Page(Page::Keyboard),
    NavEntry::Page(Page::Mouse),
    NavEntry::Separator,
    NavEntry::Planned {
        slug: "notifications",
        label: "Notifications",
        icon: "bell",
    },
    NavEntry::Planned {
        slug: "privacy",
        label: "Privacy & Security",
        icon: "lock",
    },
    NavEntry::Separator,
    NavEntry::Page(Page::About),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_round_trip() {
        for page in Page::ALL {
            assert_eq!(Page::from_slug(page.slug()), Some(page));
        }
        assert_eq!(Page::from_slug(" Appearance "), Some(Page::Appearance));
        assert_eq!(Page::from_slug("nope"), None);
    }

    #[test]
    fn nav_lists_every_page_once() {
        for page in Page::ALL {
            let count = NAV.iter().filter(|e| **e == NavEntry::Page(page)).count();
            assert_eq!(count, 1, "{page:?}");
        }
    }

    #[test]
    fn page_icons_exist() {
        for page in Page::ALL {
            assert!(desilium_ui::has_icon(page.icon()), "{page:?}");
        }
    }
}
