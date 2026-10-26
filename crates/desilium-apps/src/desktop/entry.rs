//! Typed view of one `[Desktop Entry]` file and its `[Desktop Action]` groups.

use std::path::{Path, PathBuf};

use super::ini::{self, Group, IniFile};

const DESKTOP_ENTRY_GROUP: &str = "Desktop Entry";
const ACTION_GROUP_PREFIX: &str = "Desktop Action ";
const DESKTOP_SUFFIX: &str = ".desktop";
/// Actions beyond this count are ignored; real files have a handful at most.
const MAX_ACTIONS: usize = 16;

/// Desktop file id such as `org.example.App.desktop`, validated on construction.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DesktopId(String);

impl DesktopId {
    /// Accepts a non-empty name ending in `.desktop` without separators or control characters.
    pub fn new(raw: &str) -> Option<Self> {
        let valid = raw.len() > DESKTOP_SUFFIX.len()
            && raw.ends_with(DESKTOP_SUFFIX)
            && !raw.contains(['/', '\0'])
            && !raw.chars().any(char::is_control);
        valid.then(|| Self(raw.to_owned()))
    }

    /// The id as written, including the `.desktop` suffix.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The id without its `.desktop` suffix.
    pub fn stem(&self) -> &str {
        self.0.strip_suffix(DESKTOP_SUFFIX).unwrap_or(&self.0)
    }
}

/// A `[Desktop Action x]` entry: an extra command offered by the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopAction {
    /// Action identifier from the `Actions=` list.
    pub id: String,
    /// Localized display name.
    pub name: String,
    /// Icon name or absolute path, if given.
    pub icon: Option<String>,
    /// Unescaped Exec line for this action.
    pub exec: String,
}

/// Why a desktop file cannot become an application entry.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EntryError {
    /// There is no `[Desktop Entry]` group.
    #[error("no [Desktop Entry] group")]
    MissingGroup,
    /// `Type` is missing or not `Application` (links and directories are not apps).
    #[error("entry is not Type=Application")]
    NotApplication,
    /// The entry has no localized or unlocalized `Name`.
    #[error("entry has no Name")]
    MissingName,
    /// The entry has no usable `Exec` line.
    #[error("entry has no Exec")]
    MissingExec,
    /// The file itself is not valid INI.
    #[error(transparent)]
    Ini(#[from] ini::IniError),
}

/// One application as described by its desktop file. Values are already unescaped
/// and localized; visibility is decided by the catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopEntry {
    /// File id, e.g. `org.example.App.desktop`.
    pub id: DesktopId,
    /// Path of the file this entry was read from.
    pub path: PathBuf,
    /// Localized display name.
    pub name: String,
    /// Localized generic name such as "Web Browser".
    pub generic_name: Option<String>,
    /// Localized one-line description.
    pub comment: Option<String>,
    /// Localized search keywords.
    pub keywords: Vec<String>,
    /// Icon name or absolute path.
    pub icon: Option<String>,
    /// Unescaped Exec line of the main action.
    pub exec: String,
    /// Program that must exist for the entry to be shown.
    pub try_exec: Option<String>,
    /// Working directory from `Path=`, when absolute.
    pub working_dir: Option<PathBuf>,
    /// Whether the command must run inside a terminal emulator.
    pub terminal: bool,
    /// Raw `Categories=` list.
    pub categories: Vec<String>,
    /// Raw `MimeType=` list.
    pub mime_types: Vec<String>,
    /// `NoDisplay=true`: usable and matchable, but not offered in the launcher.
    pub no_display: bool,
    /// `Hidden=true`: the entry is deleted for this id (the catalog drops it).
    pub hidden: bool,
    /// `OnlyShowIn=`: when non-empty, the desktop must be one of these.
    pub only_show_in: Vec<String>,
    /// `NotShowIn=`: the entry is hidden on these desktops.
    pub not_show_in: Vec<String>,
    /// `StartupWMClass=`: the window class windows of this app report.
    pub startup_wm_class: Option<String>,
    /// Desktop actions that exist in the file.
    pub actions: Vec<DesktopAction>,
}

impl DesktopEntry {
    /// Builds an entry from a parsed file. `locales` selects localized keys.
    pub fn from_ini(
        id: DesktopId,
        path: PathBuf,
        file: &IniFile,
        locales: &[String],
    ) -> Result<Self, EntryError> {
        let main = file
            .group(DESKTOP_ENTRY_GROUP)
            .ok_or(EntryError::MissingGroup)?;
        if main.raw("Type").map(str::trim) != Some("Application") {
            return Err(EntryError::NotApplication);
        }
        let name = localized_string(main, "Name", locales).ok_or(EntryError::MissingName)?;
        let exec = main
            .raw("Exec")
            .map(ini::unescape)
            .filter(|exec| !exec.trim().is_empty())
            .ok_or(EntryError::MissingExec)?;
        Ok(Self {
            id,
            path,
            name,
            exec,
            generic_name: localized_string(main, "GenericName", locales),
            comment: localized_string(main, "Comment", locales),
            keywords: main
                .localized("Keywords", locales)
                .map(ini::split_list)
                .unwrap_or_default(),
            icon: localized_string(main, "Icon", locales),
            try_exec: main
                .raw("TryExec")
                .map(ini::unescape)
                .filter(|v| !v.is_empty()),
            working_dir: main
                .raw("Path")
                .map(ini::unescape)
                .filter(|dir| Path::new(dir).is_absolute())
                .map(PathBuf::from),
            terminal: flag(main, "Terminal"),
            categories: list(main, "Categories"),
            mime_types: list(main, "MimeType"),
            no_display: flag(main, "NoDisplay"),
            hidden: flag(main, "Hidden"),
            only_show_in: list(main, "OnlyShowIn"),
            not_show_in: list(main, "NotShowIn"),
            startup_wm_class: main
                .raw("StartupWMClass")
                .map(ini::unescape)
                .filter(|class| !class.is_empty()),
            actions: parse_actions(file, main, locales),
        })
    }

    /// Whether `OnlyShowIn` and `NotShowIn` allow this entry on one of `desktops`.
    pub fn shown_in(&self, desktops: &[String]) -> bool {
        let matches = |list: &[String]| list.iter().any(|item| desktops.contains(item));
        (self.only_show_in.is_empty() || matches(&self.only_show_in)) && !matches(&self.not_show_in)
    }

    /// Looks up a desktop action by id.
    pub fn action(&self, id: &str) -> Option<&DesktopAction> {
        self.actions.iter().find(|action| action.id == id)
    }
}

fn localized_string(group: &Group, key: &str, locales: &[String]) -> Option<String> {
    group
        .localized(key, locales)
        .map(ini::unescape)
        .filter(|value| !value.is_empty())
}

fn list(group: &Group, key: &str) -> Vec<String> {
    group.raw(key).map(ini::split_list).unwrap_or_default()
}

fn flag(group: &Group, key: &str) -> bool {
    group.raw(key).map(str::trim) == Some("true")
}

fn parse_actions(file: &IniFile, main: &Group, locales: &[String]) -> Vec<DesktopAction> {
    list(main, "Actions")
        .iter()
        .take(MAX_ACTIONS)
        .filter_map(|action_id| action_from(file, action_id, locales))
        .collect()
}

fn action_from(file: &IniFile, id: &str, locales: &[String]) -> Option<DesktopAction> {
    let group = file.group(&format!("{ACTION_GROUP_PREFIX}{id}"))?;
    let name = localized_string(group, "Name", locales)?;
    let exec = group
        .raw("Exec")
        .map(ini::unescape)
        .filter(|exec| !exec.trim().is_empty())?;
    Some(DesktopAction {
        id: id.to_owned(),
        name,
        icon: localized_string(group, "Icon", locales),
        exec,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_entry(text: &str, locales: &[&str]) -> Result<DesktopEntry, EntryError> {
        let file = ini::parse(text)?;
        let locales: Vec<String> = locales.iter().map(|l| (*l).to_owned()).collect();
        let id = DesktopId::new("test.desktop").expect("valid id");
        DesktopEntry::from_ini(id, PathBuf::from("/x/test.desktop"), &file, &locales)
    }

    #[test]
    fn id_validation() {
        assert!(DesktopId::new("a.desktop").is_some());
        assert!(DesktopId::new(".desktop").is_none());
        assert!(DesktopId::new("a.txt").is_none());
        assert!(DesktopId::new("a/b.desktop").is_none());
        assert_eq!(
            DesktopId::new("a.desktop").map(|id| id.stem().to_owned()),
            Some("a".to_owned())
        );
    }

    #[test]
    fn reads_localized_and_escaped_fields() {
        let text = "[Desktop Entry]\nType=Application\nName=Editor\nName[de]=Editor DE\n\
                    Comment=Edit\\stext\nKeywords=text;code;\nKeywords[de]=Text;\n\
                    Exec=editor %F\nIcon=editor\nTerminal=true\nCategories=Utility;TextEditor;\n";
        let entry = parse_entry(text, &["de"]).expect("valid entry");
        assert_eq!(entry.name, "Editor DE");
        assert_eq!(entry.comment.as_deref(), Some("Edit text"));
        assert_eq!(entry.keywords, vec!["Text".to_owned()]);
        assert!(entry.terminal);
        assert_eq!(entry.categories.len(), 2);
    }

    #[test]
    fn rejects_non_applications_and_incomplete_entries() {
        assert_eq!(
            parse_entry("[Desktop Entry]\nType=Link\nName=x\nExec=x\n", &[]),
            Err(EntryError::NotApplication)
        );
        assert_eq!(
            parse_entry("[Desktop Entry]\nType=Application\nExec=x\n", &[]),
            Err(EntryError::MissingName)
        );
        assert_eq!(
            parse_entry("[Desktop Entry]\nType=Application\nName=x\n", &[]),
            Err(EntryError::MissingExec)
        );
        assert_eq!(
            parse_entry("Name=x\n", &[]),
            Err(EntryError::Ini(ini::IniError::KeyOutsideGroup(1)))
        );
        assert_eq!(
            parse_entry("# nothing\n", &[]),
            Err(EntryError::MissingGroup)
        );
    }

    #[test]
    fn parses_actions_and_drops_incomplete_ones() {
        let text = "[Desktop Entry]\nType=Application\nName=App\nExec=app\nActions=new;broken;\n\
                    [Desktop Action new]\nName=New window\nExec=app --new\n\
                    [Desktop Action broken]\nName=No exec\n";
        let entry = parse_entry(text, &[]).expect("valid entry");
        assert_eq!(entry.actions.len(), 1);
        assert_eq!(
            entry.action("new").map(|a| a.exec.as_str()),
            Some("app --new")
        );
    }

    #[test]
    fn visibility_lists_follow_desktop_names() {
        let text = "[Desktop Entry]\nType=Application\nName=App\nExec=app\nOnlyShowIn=GNOME;\n";
        let entry = parse_entry(text, &[]).expect("valid entry");
        assert!(!entry.shown_in(&["Desilium".to_owned()]));
        assert!(entry.shown_in(&["GNOME".to_owned(), "Desilium".to_owned()]));

        let text = "[Desktop Entry]\nType=Application\nName=App\nExec=app\nNotShowIn=Desilium;\n";
        let entry = parse_entry(text, &[]).expect("valid entry");
        assert!(!entry.shown_in(&["Desilium".to_owned()]));
    }

    #[test]
    fn relative_path_key_is_dropped() {
        let text = "[Desktop Entry]\nType=Application\nName=App\nExec=app\nPath=relative\n";
        let entry = parse_entry(text, &[]).expect("valid entry");
        assert_eq!(entry.working_dir, None);
    }
}
