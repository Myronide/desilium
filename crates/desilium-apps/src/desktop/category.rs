//! Maps the freedesktop `Categories=` list onto the launcher's main groups.

/// Top-level groups the launcher and dock present. `Other` collects everything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MainCategory {
    /// Audio, video and media players.
    AudioVideo,
    /// IDEs, compilers and debugging tools.
    Development,
    /// Educational software.
    Education,
    /// Games.
    Game,
    /// Image viewers, editors and drawing tools.
    Graphics,
    /// Browsers, mail and messaging.
    Network,
    /// Office suites and document tools.
    Office,
    /// Scientific software.
    Science,
    /// Control panels and settings.
    Settings,
    /// System tools and monitors.
    System,
    /// Small utilities.
    Utility,
    /// Entries without a recognised main category.
    Other,
}

/// Which freedesktop category names map to which main group.
const MAPPING: &[(&str, MainCategory)] = &[
    ("AudioVideo", MainCategory::AudioVideo),
    ("Audio", MainCategory::AudioVideo),
    ("Video", MainCategory::AudioVideo),
    ("Development", MainCategory::Development),
    ("Education", MainCategory::Education),
    ("Game", MainCategory::Game),
    ("Graphics", MainCategory::Graphics),
    ("Network", MainCategory::Network),
    ("Office", MainCategory::Office),
    ("Science", MainCategory::Science),
    ("Settings", MainCategory::Settings),
    ("DesktopSettings", MainCategory::Settings),
    ("System", MainCategory::System),
    ("Utility", MainCategory::Utility),
];

impl MainCategory {
    /// Heading shown above the group in the launcher.
    pub fn label(self) -> &'static str {
        match self {
            Self::AudioVideo => "Audio & Video",
            Self::Development => "Development",
            Self::Education => "Education",
            Self::Game => "Games",
            Self::Graphics => "Graphics",
            Self::Network => "Internet",
            Self::Office => "Office",
            Self::Science => "Science",
            Self::Settings => "Settings",
            Self::System => "System",
            Self::Utility => "Utilities",
            Self::Other => "Other",
        }
    }

    /// Group of the first category in file order that has a main group, else `Other`.
    pub fn from_categories(categories: &[String]) -> Self {
        categories
            .iter()
            .find_map(|category| {
                MAPPING
                    .iter()
                    .find(|(name, _)| *name == category.as_str())
                    .map(|(_, group)| *group)
            })
            .unwrap_or(Self::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats(list: &[&str]) -> Vec<String> {
        list.iter().map(|item| (*item).to_owned()).collect()
    }

    #[test]
    fn maps_known_categories() {
        assert_eq!(
            MainCategory::from_categories(&cats(&["Network", "WebBrowser"])),
            MainCategory::Network
        );
        assert_eq!(
            MainCategory::from_categories(&cats(&["Audio"])),
            MainCategory::AudioVideo
        );
        assert_eq!(
            MainCategory::from_categories(&cats(&["DesktopSettings"])),
            MainCategory::Settings
        );
    }

    #[test]
    fn first_mapped_category_in_file_order_wins() {
        let found = MainCategory::from_categories(&cats(&["GNOME", "Office", "Utility"]));
        assert_eq!(found, MainCategory::Office);
    }

    #[test]
    fn unknown_or_empty_lists_are_other() {
        assert_eq!(
            MainCategory::from_categories(&cats(&["GTK", "Qt"])),
            MainCategory::Other
        );
        assert_eq!(MainCategory::from_categories(&[]), MainCategory::Other);
    }
}
