//! The set of applications installed for one environment, rebuilt from the XDG directories.

mod matching;
mod scan;

use std::collections::BTreeMap;

use crate::desktop::{DesktopEntry, MainCategory};
use crate::env::{DirStamp, Environment};

/// One catalog row: the winning desktop file plus whether launcher and dock list it.
#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub(crate) entry: DesktopEntry,
    pub(crate) listed: bool,
}

/// Every application visible through the XDG search path of one environment.
#[derive(Debug, Clone)]
pub struct Catalog {
    env: Environment,
    items: BTreeMap<String, Item>,
    stamps: Vec<DirStamp>,
}

impl Catalog {
    /// Scans all `applications` directories of `env`. Unreadable files are logged and skipped.
    pub fn load(env: &Environment) -> Self {
        let scanned = scan::scan(env);
        Self {
            env: env.clone(),
            items: scanned.items,
            stamps: scanned.stamps,
        }
    }

    /// Rebuilds the catalog when a watched directory changed. Returns true when it did.
    ///
    /// Only directory mtimes are compared, so editing a file in place without touching
    /// its directory is not noticed until the next full load.
    pub fn refresh_if_changed(&mut self) -> bool {
        if !self.stamps.iter().any(DirStamp::is_stale) {
            return false;
        }
        let env = self.env.clone();
        *self = Self::load(&env);
        true
    }

    /// Environment this catalog was built from.
    pub fn environment(&self) -> &Environment {
        &self.env
    }

    /// Entry for a desktop file id (for example `firefox.desktop`), listed or not.
    pub fn get(&self, id: &str) -> Option<&DesktopEntry> {
        self.items.get(id).map(|item| &item.entry)
    }

    /// Whether the launcher and dock should offer this id.
    pub fn is_listed(&self, id: &str) -> bool {
        self.items.get(id).is_some_and(|item| item.listed)
    }

    /// Every known entry that is not hidden, in id order.
    pub fn entries(&self) -> impl Iterator<Item = &DesktopEntry> {
        self.items.values().map(|item| &item.entry)
    }

    /// Entries the launcher and dock offer, in id order.
    pub fn listed(&self) -> impl Iterator<Item = &DesktopEntry> {
        self.items
            .values()
            .filter(|item| item.listed)
            .map(|item| &item.entry)
    }

    /// Listed entries grouped by main category; categories without entries are absent.
    pub fn categories(&self) -> BTreeMap<MainCategory, Vec<&DesktopEntry>> {
        let mut groups: BTreeMap<MainCategory, Vec<&DesktopEntry>> = BTreeMap::new();
        for entry in self.listed() {
            let group = MainCategory::from_categories(&entry.categories);
            groups.entry(group).or_default().push(entry);
        }
        groups
    }

    /// Finds the entry for a window's Wayland `app_id` or X11 class. Hidden and
    /// `NoDisplay` entries are included, because their windows still need an icon.
    ///
    /// Heuristics in order: exact id, case-insensitive id, `StartupWMClass`, the
    /// basename of the Exec program, then the last segment of a reverse-DNS id.
    pub fn match_app_id(&self, app_id: &str) -> Option<&DesktopEntry> {
        matching::find(&self.items, app_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, age_dir, env_with_data_dirs};

    fn fixture() -> (TempDir, Environment) {
        let dir = TempDir::new("catalog");
        dir.write(
            "applications/alpha.desktop",
            "[Desktop Entry]\nType=Application\nName=Alpha\nExec=alpha\nCategories=Office;\n",
        );
        dir.write(
            "applications/hidden-one.desktop",
            "[Desktop Entry]\nType=Application\nName=Gone\nExec=gone\nHidden=true\n",
        );
        dir.write(
            "applications/quiet.desktop",
            "[Desktop Entry]\nType=Application\nName=Quiet\nExec=quiet\nNoDisplay=true\n",
        );
        let env = env_with_data_dirs(&[dir.path()]);
        (dir, env)
    }

    #[test]
    fn hidden_entries_are_dropped_and_no_display_ones_are_not_listed() {
        let (_dir, env) = fixture();
        let catalog = Catalog::load(&env);
        assert!(catalog.get("hidden-one.desktop").is_none());
        assert!(catalog.get("quiet.desktop").is_some());
        assert!(!catalog.is_listed("quiet.desktop"));
        assert!(catalog.is_listed("alpha.desktop"));
        assert_eq!(catalog.listed().count(), 1);
    }

    #[test]
    fn categories_group_listed_entries() {
        let (_dir, env) = fixture();
        let catalog = Catalog::load(&env);
        let groups = catalog.categories();
        assert_eq!(groups.get(&MainCategory::Office).map(Vec::len), Some(1));
        assert!(!groups.contains_key(&MainCategory::Other));
    }

    #[test]
    fn refresh_reports_new_files() {
        let (dir, env) = fixture();
        age_dir(&dir.path().join("applications"));
        let mut catalog = Catalog::load(&env);
        assert!(!catalog.refresh_if_changed());
        dir.write(
            "applications/beta.desktop",
            "[Desktop Entry]\nType=Application\nName=Beta\nExec=beta\n",
        );
        assert!(catalog.refresh_if_changed());
        assert!(catalog.is_listed("beta.desktop"));
    }

    #[test]
    fn missing_directories_produce_an_empty_catalog() {
        let dir = TempDir::new("catalog-empty");
        let env = env_with_data_dirs(&[dir.path()]);
        let catalog = Catalog::load(&env);
        assert_eq!(catalog.entries().count(), 0);
    }
}
