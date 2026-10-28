//! What the "Get new themes" page shows: the tab, the search, the current page of results and
//! what is installed.

use std::collections::BTreeMap;

use super::{Kind, Listing, ocs::PAGE_SIZE};

/// Where a search stands.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Load {
    /// Nothing asked yet.
    #[default]
    Idle,
    /// A request is on its way.
    Loading,
    /// The request failed; the text is for the user.
    Failed(String),
    /// The results are in.
    Ready,
}

/// State of the theme store page.
#[derive(Debug, Clone)]
pub struct StoreState {
    /// The tab.
    pub kind: Kind,
    /// What is typed in the search field.
    pub draft: String,
    /// The search the results belong to.
    pub query: String,
    /// The shown page, counted from 0.
    pub page: u32,
    /// Where the request stands.
    pub load: Load,
    /// The shown page of results.
    pub listing: Listing,
    /// Number of the latest request; answers to older ones are dropped.
    pub seq: u64,
    /// Store id of the entry being downloaded.
    pub installing: Option<String>,
    /// Names installed on this computer for the tab, sorted.
    pub installed: Vec<String>,
    /// Store id to the theme names installed from it.
    pub by_id: BTreeMap<String, Vec<String>>,
}

impl Default for StoreState {
    fn default() -> Self {
        Self {
            kind: Kind::Icons,
            draft: String::new(),
            query: String::new(),
            page: 0,
            load: Load::Idle,
            listing: Listing::default(),
            seq: 0,
            installing: None,
            installed: Vec::new(),
            by_id: BTreeMap::new(),
        }
    }
}

impl StoreState {
    /// Whether a further page exists after the shown one.
    pub fn has_next(&self) -> bool {
        let shown = u64::from(self.page + 1) * u64::from(PAGE_SIZE);
        if self.listing.total > 0 {
            shown < self.listing.total
        } else {
            self.listing.entries.len() as u32 >= PAGE_SIZE
        }
    }

    /// Whether an earlier page exists.
    pub fn has_previous(&self) -> bool {
        self.page > 0
    }

    /// Number of pages (at least 1).
    pub fn pages(&self) -> u32 {
        let total = self.listing.total.max(self.listing.entries.len() as u64);
        u32::try_from(total.div_ceil(u64::from(PAGE_SIZE)))
            .unwrap_or(u32::MAX)
            .max(1)
    }

    /// Theme names installed from store entry `id`.
    pub fn installed_from(&self, id: &str) -> &[String] {
        self.by_id.get(id).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_total(total: u64, shown: usize, page: u32) -> StoreState {
        let entry = super::super::Entry {
            id: "1".into(),
            name: "n".into(),
            author: String::new(),
            downloads: 0,
            score: 0,
            detail_page: None,
            files: Vec::new(),
        };
        StoreState {
            page,
            listing: Listing {
                total,
                entries: vec![entry; shown],
            },
            ..StoreState::default()
        }
    }

    #[test]
    fn paging_follows_the_total() {
        let first = with_total(25, 10, 0);
        assert!(first.has_next() && !first.has_previous());
        assert_eq!(first.pages(), 3);
        let last = with_total(25, 5, 2);
        assert!(!last.has_next() && last.has_previous());
    }

    #[test]
    fn without_a_total_a_full_page_means_there_may_be_more() {
        assert!(with_total(0, 10, 0).has_next());
        assert!(!with_total(0, 4, 0).has_next());
        assert_eq!(StoreState::default().pages(), 1);
    }
}
