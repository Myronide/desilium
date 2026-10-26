//! Pages and the sidebar navigation.

/// A settings page that exists today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    /// Theme, accent, wallpaper, motion, pointer.
    Appearance,
    /// Virtual desktops and applications.
    Desktop,
    /// Keyboard shortcuts.
    Keyboard,
    /// Version and system information.
    About,
}

impl Page {
    /// All pages in sidebar order.
    pub const ALL: [Page; 4] = [Page::Appearance, Page::Desktop, Page::Keyboard, Page::About];

    /// Page heading and sidebar label.
    pub fn title(self) -> &'static str {
        match self {
            Page::Appearance => "Appearance",
            Page::Desktop => "Desktop",
            Page::Keyboard => "Keyboard",
            Page::About => "About",
        }
    }

    /// Sentence under the heading.
    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Appearance => "Make Desilium look the way you want. Changes apply instantly.",
            Page::Desktop => "Virtual desktops and the programs the desktop starts for you.",
            Page::Keyboard => "The shortcuts that drive windows and desktops.",
            Page::About => "What is running on this computer.",
        }
    }

    /// Sidebar icon name.
    pub fn icon(self) -> &'static str {
        match self {
            Page::Appearance => "palette",
            Page::Desktop => "layers",
            Page::Keyboard => "keyboard",
            Page::About => "info",
        }
    }

    /// Stable identifier used on the command line and in element ids.
    pub fn slug(self) -> &'static str {
        match self {
            Page::Appearance => "appearance",
            Page::Desktop => "desktop",
            Page::Keyboard => "keyboard",
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
        /// Label.
        label: &'static str,
        /// Icon name.
        icon: &'static str,
    },
    /// A thin divider between groups.
    Separator,
}

/// Sidebar layout.
pub const NAV: [NavEntry; 8] = [
    NavEntry::Page(Page::Appearance),
    NavEntry::Page(Page::Desktop),
    NavEntry::Page(Page::Keyboard),
    NavEntry::Separator,
    NavEntry::Planned {
        label: "Notifications",
        icon: "bell",
    },
    NavEntry::Planned {
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
