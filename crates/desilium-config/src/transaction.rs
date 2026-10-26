//! Copy-on-write edits that are validated and applied as one unit.

use desilium_config_types::DesiliumConfig;

use crate::error::ConfigError;
use crate::store::ConfigStore;
use crate::validate::{Repair, validate};

/// Result of a successful commit.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitReport {
    /// False when the edited config equalled the active one; nothing was written.
    pub changed: bool,
    /// Values that validation clamped or repaired before the write.
    pub repairs: Vec<Repair>,
    /// Sequence number of the backup holding the previous file, if there was one.
    pub backup: Option<u64>,
}

/// A batch of edits on a private copy of the configuration.
///
/// Dropping a transaction without calling [`Transaction::commit`] discards the
/// edits. The mutable borrow of the store also rules out overlapping transactions.
pub struct Transaction<'a> {
    store: &'a mut ConfigStore,
    working: DesiliumConfig,
}

impl<'a> Transaction<'a> {
    pub(crate) fn new(store: &'a mut ConfigStore) -> Self {
        let working = store.get().clone();
        Self { store, working }
    }

    /// The edited config as it currently stands, before validation.
    pub fn config(&self) -> &DesiliumConfig {
        &self.working
    }

    /// Applies `change` to the working copy.
    pub fn edit(&mut self, change: impl FnOnce(&mut DesiliumConfig)) -> &mut Self {
        change(&mut self.working);
        self
    }

    /// Validates, backs up, writes atomically and notifies listeners.
    pub fn commit(self) -> Result<CommitReport, ConfigError> {
        let Self { store, mut working } = self;
        let repairs = validate(&mut working).repairs;
        let mut report = store.apply(working)?;
        report.repairs = repairs;
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use crate::store::ConfigStore;
    use desilium_config_types::DesiliumConfig;

    #[test]
    fn edit_happens_on_a_copy_until_commit() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut store = ConfigStore::open(dir.path()).store;
        {
            let mut tx = store.transaction();
            tx.edit(|cfg| cfg.appearance.cursor_size = 40);
            assert_eq!(tx.config().appearance.cursor_size, 40);
        }
        assert_eq!(store.get(), &DesiliumConfig::default());
    }
}
