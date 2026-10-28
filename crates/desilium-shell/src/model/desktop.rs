//! The desktop's icons: what lies in the desktop folder, how it is ordered, laid out and named.
//!
//! Pure data and rules; reading and changing the folder is `services::desktop`.

use std::path::PathBuf;

/// Edge of an icon.
pub(crate) const ICON_PX: u32 = 48;
/// Most icons shown; a folder with more is cut off rather than slowing the shell down.
pub(crate) const MAX_ITEMS: usize = 512;
/// Longest file name Linux file systems accept, in bytes.
const MAX_NAME_BYTES: usize = 255;
/// Characters of a label before it is shortened with an ellipsis.
const LABEL_CHARS: usize = 22;

/// What an icon stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A `.desktop` application launcher. Only a `trusted` one (executable bit set) is started
    /// by a double click: a launcher that arrived by download must not run on a click.
    Launcher { trusted: bool },
    /// A folder.
    Folder,
    /// Any other file.
    File,
}

/// One icon on the desktop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeskItem {
    /// Where it lives.
    pub path: PathBuf,
    /// Its file name, which is what renaming changes.
    pub file_name: String,
    /// The name shown: the `Name=` of a launcher, else the file name.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// `Icon=` of a launcher.
    pub icon: Option<String>,
}

/// Folders first, then everything else, each group by name without regard to case.
pub(crate) fn sort(items: &mut [DeskItem]) {
    items.sort_by_cached_key(|item| {
        (
            item.kind != Kind::Folder,
            item.name.to_lowercase(),
            item.file_name.clone(),
        )
    });
}

/// The label under an icon, shortened so two lines are enough.
pub(crate) fn short_label(name: &str) -> String {
    if name.chars().count() <= LABEL_CHARS {
        return name.to_owned();
    }
    let kept: String = name.chars().take(LABEL_CHARS - 1).collect();
    format!("{kept}…")
}

/// Why a name cannot be used for a file or folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum NameError {
    /// Nothing but blanks.
    #[error("the name is empty")]
    Empty,
    /// Contains a slash, which would put the file somewhere else.
    #[error("a name cannot contain /")]
    Slash,
    /// `.` and `..` mean directories.
    #[error("that name is reserved")]
    Reserved,
    /// Longer than a file system accepts.
    #[error("the name is too long")]
    TooLong,
    /// Contains a control character.
    #[error("the name contains a control character")]
    Control,
}

/// The name without surrounding blanks, or why it cannot be used.
pub(crate) fn validate_name(name: &str) -> Result<&str, NameError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(NameError::Empty);
    }
    if name.contains('/') {
        return Err(NameError::Slash);
    }
    if name == "." || name == ".." {
        return Err(NameError::Reserved);
    }
    if name.len() > MAX_NAME_BYTES {
        return Err(NameError::TooLong);
    }
    if name.chars().any(char::is_control) {
        return Err(NameError::Control);
    }
    Ok(name)
}

/// `stem` plus `extension` if that name is free, else `stem 2`, `stem 3`, ... plus `extension`.
pub(crate) fn unique_name(stem: &str, extension: &str, exists: impl Fn(&str) -> bool) -> String {
    let first = format!("{stem}{extension}");
    if !exists(&first) {
        return first;
    }
    (2u32..)
        .map(|number| format!("{stem} {number}{extension}"))
        .find(|candidate| !exists(candidate))
        .unwrap_or(first)
}

/// Icon names for a plain file, best first, from its extension. The last ones are generic so
/// that every file has some icon.
pub(crate) fn file_icon_names(file_name: &str) -> Vec<&'static str> {
    let extension = file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let specific = match extension.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "tif" | "tiff" | "avif" => {
            "image-x-generic"
        }
        "mp3" | "flac" | "ogg" | "oga" | "opus" | "wav" | "m4a" | "aac" => "audio-x-generic",
        "mp4" | "mkv" | "webm" | "avi" | "mov" | "m4v" => "video-x-generic",
        "pdf" => "application-pdf",
        "zip" | "tar" | "gz" | "xz" | "bz2" | "zst" | "7z" | "rar" | "deb" | "rpm" => {
            "package-x-generic"
        }
        "odt" | "doc" | "docx" | "rtf" => "x-office-document",
        "ods" | "xls" | "xlsx" | "csv" => "x-office-spreadsheet",
        "odp" | "ppt" | "pptx" => "x-office-presentation",
        "sh" | "bash" | "py" | "pl" | "rb" => "text-x-script",
        "html" | "htm" => "text-html",
        _ => "text-x-generic",
    };
    let mut names = vec![specific];
    if specific != "text-x-generic" {
        names.push("text-x-generic");
    }
    names.push("unknown");
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(name: &str, kind: Kind) -> DeskItem {
        DeskItem {
            path: PathBuf::from(format!("/d/{name}")),
            file_name: name.to_owned(),
            name: name.to_owned(),
            kind,
            icon: None,
        }
    }

    #[test]
    fn folders_come_first_then_names_without_regard_to_case() {
        let mut items = vec![
            item("zeta.txt", Kind::File),
            item("Beta", Kind::Folder),
            item("alpha.txt", Kind::File),
            item("app.desktop", Kind::Launcher { trusted: true }),
            item("alpha", Kind::Folder),
        ];
        sort(&mut items);
        let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            ["alpha", "Beta", "alpha.txt", "app.desktop", "zeta.txt"]
        );
    }

    #[test]
    fn long_labels_are_shortened_by_characters_not_bytes() {
        assert_eq!(short_label("Notes.txt"), "Notes.txt");
        let long = "ä".repeat(40);
        let short = short_label(&long);
        assert_eq!(short.chars().count(), LABEL_CHARS);
        assert!(short.ends_with('…'));
    }

    #[test]
    fn names_are_trimmed_and_unsafe_ones_refused() {
        assert_eq!(validate_name("  Photos "), Ok("Photos"));
        assert_eq!(validate_name("  "), Err(NameError::Empty));
        assert_eq!(validate_name("a/b"), Err(NameError::Slash));
        assert_eq!(validate_name("../x"), Err(NameError::Slash));
        assert_eq!(validate_name(".."), Err(NameError::Reserved));
        assert_eq!(validate_name("."), Err(NameError::Reserved));
        assert_eq!(validate_name("a\nb"), Err(NameError::Control));
        assert_eq!(validate_name(&"x".repeat(256)), Err(NameError::TooLong));
        assert_eq!(validate_name(&"x".repeat(255)).map(str::len), Ok(255));
        assert_eq!(validate_name(".hidden"), Ok(".hidden"));
    }

    #[test]
    fn new_names_count_up_until_one_is_free() {
        let taken = ["New Folder", "New Folder 2"];
        assert_eq!(
            unique_name("New Folder", "", |n| taken.contains(&n)),
            "New Folder 3"
        );
        assert_eq!(unique_name("New Folder", "", |_| false), "New Folder");
        let files = ["New File.txt"];
        assert_eq!(
            unique_name("New File", ".txt", |n| files.contains(&n)),
            "New File 2.txt"
        );
    }

    #[test]
    fn files_get_a_specific_icon_name_and_generic_ones_behind_it() {
        assert_eq!(
            file_icon_names("cat.PNG"),
            ["image-x-generic", "text-x-generic", "unknown"]
        );
        assert_eq!(file_icon_names("notes.txt"), ["text-x-generic", "unknown"]);
        assert_eq!(file_icon_names("README"), ["text-x-generic", "unknown"]);
        assert_eq!(file_icon_names("paper.pdf")[0], "application-pdf");
        assert_eq!(file_icon_names("backup.tar.zst")[0], "package-x-generic");
    }
}
