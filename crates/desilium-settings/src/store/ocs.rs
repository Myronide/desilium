//! The KDE Store's Open Collaboration Services (OCS) API: request URLs and answers.
//!
//! Everything here is pure text in, text out. The endpoint, the category numbers and the field
//! names were written from memory and have not been checked against the live service (see
//! `docs/theme-store.md`); the parser therefore accepts every shape of answer it knows of and
//! skips entries it cannot use instead of failing the whole list.

use serde_json::Value;

use super::StoreError;

/// Base of the OCS API of the KDE Store (Pling).
pub const API: &str = "https://api.kde-look.org/ocs/v1";
/// The store's web site, for the "Open store website" button.
pub const SITE: &str = "https://store.kde.org";
/// Entries per page.
pub const PAGE_SIZE: u32 = 10;
/// Longest download link accepted.
const MAX_URL_LEN: usize = 2048;
/// Longest name or author shown.
const MAX_TEXT_LEN: usize = 80;
/// The API numbers its downloads `downloadlink1`, `downloadlink2`, and so on.
const MAX_FILES: u32 = 12;

/// What a store category holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Full icon themes.
    Icons,
    /// Cursor themes.
    Cursors,
    /// KDE colour schemes.
    Colours,
}

impl Kind {
    /// All kinds in tab order.
    pub const ALL: [Kind; 3] = [Kind::Icons, Kind::Cursors, Kind::Colours];

    /// Category number of the store (UNVERIFIED: icons 132, cursors 107, colour schemes 112).
    pub fn category(self) -> u32 {
        match self {
            Kind::Icons => 132,
            Kind::Cursors => 107,
            Kind::Colours => 112,
        }
    }

    /// Stable name used in action arguments.
    pub fn slug(self) -> &'static str {
        match self {
            Kind::Icons => "icons",
            Kind::Cursors => "cursors",
            Kind::Colours => "colours",
        }
    }

    /// Parses [`slug`](Self::slug).
    pub fn from_slug(slug: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.slug() == slug)
    }
}

/// How the store orders a list (`sortmode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Newest first.
    Newest,
    /// By name.
    Alphabetical,
    /// Best rated first.
    Highest,
    /// Most downloaded first.
    #[default]
    Downloads,
}

impl Sort {
    fn mode(self) -> &'static str {
        match self {
            Sort::Newest => "new",
            Sort::Alphabetical => "alpha",
            Sort::Highest => "high",
            Sort::Downloads => "down",
        }
    }
}

/// One file of an entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    /// File name as the store shows it.
    pub name: String,
    /// `https` link to the file.
    pub url: String,
    /// Size in kilobytes, when the store says.
    pub size_kb: Option<u64>,
}

impl RemoteFile {
    /// Whether the name looks like something the installer can unpack or copy.
    pub fn is_installable(&self) -> bool {
        let name = self.name.to_ascii_lowercase();
        [
            ".tar.gz", ".tgz", ".tar.xz", ".txz", ".tar.bz2", ".tbz2", ".tar.zst", ".tar", ".zip",
            ".colors",
        ]
        .iter()
        .any(|suffix| name.ends_with(suffix))
    }
}

/// One theme in the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Store id (digits only; it goes into URLs).
    pub id: String,
    /// Title.
    pub name: String,
    /// Author's login.
    pub author: String,
    /// Download count.
    pub downloads: u64,
    /// Score, 0 to 100.
    pub score: u32,
    /// The entry's page in the store.
    pub detail_page: Option<String>,
    /// Files, as far as the list carries them (a detail request may add them).
    pub files: Vec<RemoteFile>,
}

impl Entry {
    /// The file to install: the first installable one, preferring `.colors` for colour schemes.
    pub fn best_file(&self, kind: Kind) -> Option<&RemoteFile> {
        let installable = |file: &&RemoteFile| file.is_installable();
        if kind == Kind::Colours
            && let Some(file) = self
                .files
                .iter()
                .find(|file| file.name.to_ascii_lowercase().ends_with(".colors"))
        {
            return Some(file);
        }
        self.files.iter().find(installable)
    }
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Listing {
    /// How many entries match in all pages (0 when the store does not say).
    pub total: u64,
    /// This page.
    pub entries: Vec<Entry>,
}

/// Percent-encodes everything but unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// The list request: `kind` filtered by `query`, page `page` (counted from 0).
pub fn search_url(kind: Kind, query: &str, sort: Sort, page: u32) -> String {
    format!(
        "{API}/content/data?categories={}&search={}&sortmode={}&page={page}&pagesize={PAGE_SIZE}&format=json",
        kind.category(),
        encode(query.trim()),
        sort.mode(),
    )
}

/// The detail request of one entry; `None` unless `id` is all digits.
pub fn detail_url(id: &str) -> Option<String> {
    is_id(id).then(|| format!("{API}/content/data/{id}?format=json"))
}

/// The entry's page for the "Open store website" button of a kind.
pub fn site_url(kind: Kind) -> String {
    format!("{SITE}/browse?cat={}&ord=rating", kind.category())
}

fn is_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 12 && id.bytes().all(|byte| byte.is_ascii_digit())
}

/// `https://host/...` without user info, whitespace or control characters.
pub fn is_https(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host_part = rest.split(['/', '?', '#']).next().unwrap_or_default();
    url.len() <= MAX_URL_LEN
        && !host_part.is_empty()
        && !host_part.contains('@')
        && !url.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn number(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn text_of(value: Option<&Value>) -> Option<String> {
    let text = match value? {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let text: String = text.chars().filter(|c| !c.is_control()).collect();
    (!text.is_empty()).then(|| text.chars().take(MAX_TEXT_LEN).collect())
}

/// The items of an answer: `data` at the top, or `ocs.data.content`; one item or a list.
fn items(root: &Value) -> Vec<&Value> {
    let node = root
        .get("data")
        .or_else(|| root.pointer("/ocs/data/content"))
        .or_else(|| root.pointer("/ocs/data"));
    match node {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(object @ Value::Object(map)) if map.contains_key("id") => vec![object],
        Some(Value::Object(map)) => map.values().filter(|v| v.is_object()).collect(),
        _ => Vec::new(),
    }
}

fn files_of(item: &Value) -> Vec<RemoteFile> {
    (1..=MAX_FILES)
        .filter_map(|n| {
            let url = text_of(item.get(format!("downloadlink{n}")))?;
            if !is_https(&url) {
                return None;
            }
            let name = text_of(item.get(format!("downloadname{n}")))
                .or_else(|| {
                    url.split(['?', '#'])
                        .next()
                        .and_then(|path| path.rsplit('/').next())
                        .filter(|segment| !segment.is_empty())
                        .map(str::to_owned)
                })
                .unwrap_or_else(|| format!("download{n}"));
            let size_kb = item.get(format!("downloadsize{n}")).and_then(number);
            Some(RemoteFile { name, url, size_kb })
        })
        .collect()
}

fn entry_of(item: &Value) -> Option<Entry> {
    let id = text_of(item.get("id"))?;
    if !is_id(&id) {
        return None;
    }
    Some(Entry {
        name: text_of(item.get("name"))?,
        author: text_of(item.get("personid"))
            .or_else(|| text_of(item.get("username")))
            .unwrap_or_default(),
        downloads: item.get("downloads").and_then(number).unwrap_or(0),
        score: item
            .get("score")
            .and_then(number)
            .map_or(0, |score| score.min(100) as u32),
        detail_page: text_of(item.get("detailpage")).filter(|url| is_https(url)),
        files: files_of(item),
        id,
    })
}

/// Reads a list or a detail answer. A refusal by the store is an error; entries that lack an id
/// or a name are skipped.
pub fn parse_listing(text: &str) -> Result<Listing, StoreError> {
    let root: Value =
        serde_json::from_str(text).map_err(|error| StoreError::BadResponse(error.to_string()))?;
    let status = text_of(root.get("status"))
        .or_else(|| text_of(root.pointer("/ocs/meta/status")))
        .unwrap_or_default();
    if status.eq_ignore_ascii_case("failed") {
        let message = text_of(root.get("message"))
            .or_else(|| text_of(root.pointer("/ocs/meta/message")))
            .unwrap_or_else(|| "failed".to_owned());
        return Err(StoreError::Rejected(message));
    }
    let entries: Vec<Entry> = items(&root).into_iter().filter_map(entry_of).collect();
    let total = root
        .get("totalitems")
        .or_else(|| root.pointer("/ocs/meta/totalitems"))
        .and_then(number)
        .unwrap_or(entries.len() as u64);
    Ok(Listing { total, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"{
        "status": "ok", "statuscode": 100, "message": "",
        "totalitems": 37, "itemsperpage": 10,
        "data": [
            {"id": "1234", "name": "Papirus <Icons>", "personid": "varlesh",
             "downloads": "5120", "score": "87", "detailpage": "https://www.pling.com/p/1234",
             "downloadlink1": "https://dl.opendesktop.org/api/files/download/id/1/papirus.tar.xz",
             "downloadname1": "papirus.tar.xz", "downloadsize1": "4096",
             "downloadlink2": "http://insecure.example/a.zip", "downloadname2": "a.zip"},
            {"id": "x1", "name": "bad id"},
            {"id": "77", "name": ""},
            {"id": "99", "name": "Plain", "username": "someone", "downloads": 7, "score": 250}
        ]
    }"#;

    #[test]
    fn urls_carry_category_search_sort_and_page() {
        let url = search_url(Kind::Icons, " blue sky & more ", Sort::Downloads, 2);
        assert!(url.starts_with("https://api.kde-look.org/ocs/v1/content/data?"));
        assert!(url.contains("categories=132"));
        assert!(url.contains("search=blue%20sky%20%26%20more&"));
        assert!(url.contains("sortmode=down"));
        assert!(url.contains("page=2&pagesize=10&format=json"));
        assert!(search_url(Kind::Cursors, "", Sort::Newest, 0).contains("categories=107"));
        assert!(search_url(Kind::Colours, "", Sort::Highest, 0).contains("categories=112"));
        assert!(site_url(Kind::Icons).starts_with("https://store.kde.org/browse?cat=132"));
    }

    #[test]
    fn detail_urls_need_a_numeric_id() {
        assert!(detail_url("1234").is_some());
        for bad in ["", "12/../3", "12a", "1234567890123"] {
            assert_eq!(detail_url(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_list_is_read_and_unusable_entries_are_skipped() {
        let listing = parse_listing(LIST).expect("listing");
        assert_eq!(listing.total, 37);
        assert_eq!(listing.entries.len(), 2, "{listing:?}");
        let first = &listing.entries[0];
        assert_eq!(
            (first.id.as_str(), first.downloads, first.score),
            ("1234", 5120, 87)
        );
        assert_eq!(first.name, "Papirus <Icons>", "escaping is the view's job");
        assert_eq!(first.author, "varlesh");
        assert_eq!(
            first.files.len(),
            1,
            "the http link is dropped: {:?}",
            first.files
        );
        assert_eq!(first.files[0].size_kb, Some(4096));
        assert!(first.best_file(Kind::Icons).is_some());
        let second = &listing.entries[1];
        assert_eq!((second.author.as_str(), second.score), ("someone", 100));
        assert!(second.files.is_empty());
    }

    #[test]
    fn the_other_known_envelope_and_single_items_work_too() {
        let wrapped = r#"{"ocs": {"meta": {"status": "ok", "totalitems": "1"},
            "data": {"content": [{"id": "5", "name": "A"}]}}}"#;
        let listing = parse_listing(wrapped).expect("wrapped");
        assert_eq!((listing.total, listing.entries.len()), (1, 1));
        let single =
            r#"{"data": {"id": "6", "name": "B", "downloadlink1": "https://x.org/b.zip"}}"#;
        let listing = parse_listing(single).expect("single");
        assert_eq!(
            listing.entries[0].files[0].name, "b.zip",
            "named after the link"
        );
    }

    #[test]
    fn a_refusal_and_garbage_are_errors() {
        let refused = r#"{"status": "failed", "statuscode": 999, "message": "no such category"}"#;
        assert!(
            matches!(parse_listing(refused), Err(StoreError::Rejected(m)) if m.contains("category"))
        );
        assert!(matches!(
            parse_listing("<html>"),
            Err(StoreError::BadResponse(_))
        ));
        assert_eq!(parse_listing("{}").expect("empty").entries.len(), 0);
    }

    #[test]
    fn only_clean_https_links_pass() {
        assert!(is_https("https://dl.example.org/a/b.tar.gz?x=1"));
        for bad in [
            "http://example.org/a",
            "ftp://example.org/a",
            "https://",
            "https://user:pw@example.org/a",
            "https://example.org/a b",
            "https://example.org/a\nb",
            "file:///etc/passwd",
        ] {
            assert!(!is_https(bad), "{bad:?}");
        }
        assert!(!is_https(&format!("https://x.org/{}", "a".repeat(3000))));
    }

    #[test]
    fn installable_files_and_the_best_pick() {
        let file = |name: &str| RemoteFile {
            name: name.to_owned(),
            url: "https://x.org/f".to_owned(),
            size_kb: None,
        };
        let entry = Entry {
            id: "1".into(),
            name: "n".into(),
            author: String::new(),
            downloads: 0,
            score: 0,
            detail_page: None,
            files: vec![
                file("README.txt"),
                file("theme.tar.gz"),
                file("Scheme.colors"),
            ],
        };
        assert_eq!(
            entry.best_file(Kind::Icons).map(|f| f.name.as_str()),
            Some("theme.tar.gz")
        );
        assert_eq!(
            entry.best_file(Kind::Colours).map(|f| f.name.as_str()),
            Some("Scheme.colors")
        );
        assert!(!file("notes.pdf").is_installable());
    }

    #[test]
    fn kinds_round_trip_through_their_slugs() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_slug(kind.slug()), Some(kind));
        }
        assert_eq!(Kind::from_slug("themes"), None);
    }
}
