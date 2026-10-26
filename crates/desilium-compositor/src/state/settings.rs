//! Runtime settings: the configuration store and the single path that applies it.
//!
//! [`DesiliumState::apply_config`] is the only place where a configuration takes
//! effect. Startup, the file watcher and [`DesiliumState::update_config`] all end there.
//! The appearance is exposed through [`DesiliumState::appearance`] and read lazily
//! by the subsystems at render time.
#![allow(dead_code)] // appearance consumers (animations, cursor, wallpaper) land in later steps

mod resolve;
mod watch;

use std::path::PathBuf;

use desilium_config::{CommitReport, ConfigError, ConfigStore, ReloadOutcome};
use desilium_config_types::{AppearanceSettings, CompositorSettings, DesiliumConfig};
use desilium_theme::Mode;
use smithay::reexports::calloop::LoopHandle;
use tracing::{info, warn};

use super::DesiliumState;
use super::input::ShortcutTable;
use resolve::resolve_appearance;

/// Configuration state owned by the compositor.
#[derive(Default)]
pub struct SettingsRuntime {
    /// Last configuration applied to the compositor.
    config: DesiliumConfig,
    /// Persistent store. `None` when configuration is disabled (in-memory defaults only).
    store: Option<ConfigStore>,
    /// Directory of the store; watched for external edits.
    config_dir: Option<PathBuf>,
    /// `--theme dark|light`; wins over the configured theme id.
    theme_override: Option<Mode>,
    /// Effective appearance after resolution.
    appearance: AppearanceSettings,
}

impl DesiliumState {
    /// The appearance in effect: theme id, accent, wallpaper, animations and cursor.
    pub fn appearance(&self) -> &AppearanceSettings {
        &self.settings_runtime.appearance
    }

    /// Loads the configuration from `config_dir` and applies it.
    ///
    /// `None` keeps everything in memory and performs no file I/O. Never fails:
    /// the store repairs or quarantines broken files and logs each repair.
    pub fn init_settings(&mut self, config_dir: Option<PathBuf>, theme_override: Option<Mode>) {
        self.settings_runtime.theme_override = theme_override;
        let config = match &config_dir {
            Some(dir) => {
                let outcome = ConfigStore::open(dir);
                let config = outcome.store.get().clone();
                self.settings_runtime.store = Some(outcome.store);
                info!(dir = %dir.display(), "configuration store opened");
                config
            }
            None => {
                info!("configuration disabled; using built-in defaults");
                DesiliumConfig::default()
            }
        };
        self.settings_runtime.config_dir = config_dir;
        self.apply_config(&config);
    }

    /// Watches the configuration directory so external edits apply live.
    ///
    /// Does nothing without a store. If inotify is unavailable the compositor keeps
    /// running with the configuration it loaded at startup.
    pub fn watch_config(&self, handle: &LoopHandle<'static, DesiliumState>) {
        let Some(dir) = self.settings_runtime.config_dir.clone() else {
            return;
        };
        let result = watch::install(handle, dir.clone(), |state: &mut DesiliumState| {
            state.reload_from_disk();
        });
        match result {
            Ok(()) => info!(dir = %dir.display(), "watching the configuration directory"),
            Err(error) => warn!(%error, "live configuration updates are disabled"),
        }
    }

    /// Edits the configuration, saves it and applies it.
    ///
    /// Intended for the settings application and IPC. Without a store
    /// (`--no-config`, `--headless`) the change stays in memory.
    pub fn update_config(
        &mut self,
        change: impl FnOnce(&mut DesiliumConfig),
    ) -> Result<CommitReport, ConfigError> {
        let (report, next) = match self.settings_runtime.store.as_mut() {
            Some(store) => {
                let mut tx = store.transaction();
                tx.edit(change);
                let report = tx.commit()?;
                (report, store.get().clone())
            }
            None => edit_in_memory(&self.settings_runtime.config, change),
        };
        if report.changed {
            self.apply_config(&next);
        }
        Ok(report)
    }

    /// Applies `config` to every subsystem. The one place where a configuration takes effect.
    pub fn apply_config(&mut self, config: &DesiliumConfig) {
        self.settings_runtime.config = config.clone();
        self.apply_compositor(&config.compositor);
        self.apply_appearance(&config.appearance);
    }

    fn apply_compositor(&mut self, settings: &CompositorSettings) {
        self.shortcuts = ShortcutTable::from_settings(settings);
        self.set_desktop_count(usize::from(settings.desktops));
        if let Some(keyboard) = self.seat.get_keyboard() {
            keyboard.change_repeat_info(
                settings.keyboard_repeat_rate.into(),
                settings.keyboard_repeat_delay.into(),
            );
        }
        if !settings.snap_enabled {
            self.clear_snap_preview();
        }
        self.settings = settings.clone();
    }

    fn apply_appearance(&mut self, appearance: &AppearanceSettings) {
        let resolved = resolve_appearance(appearance, self.settings_runtime.theme_override);
        for warning in &resolved.warnings {
            warn!("{warning}");
        }
        self.settings_runtime.appearance = resolved.appearance;
        self.set_theme(resolved.theme);
    }

    /// Re-reads the file after a change on disk. A rejected file keeps the last good configuration.
    fn reload_from_disk(&mut self) {
        let Some(store) = self.settings_runtime.store.as_mut() else {
            return;
        };
        let Some(outcome) = store.reload_if_changed() else {
            return;
        };
        match outcome {
            ReloadOutcome::Applied { recovery } => {
                for item in &recovery {
                    warn!("{item}");
                }
                let config = store.get().clone();
                self.apply_config(&config);
            }
            ReloadOutcome::Rejected { reason } => {
                warn!(%reason, "configuration file rejected; keeping the last good configuration");
            }
        }
    }
}

/// Applies `change` to a copy of `current` and validates it. Touches no file.
fn edit_in_memory(
    current: &DesiliumConfig,
    change: impl FnOnce(&mut DesiliumConfig),
) -> (CommitReport, DesiliumConfig) {
    let mut next = current.clone();
    change(&mut next);
    let repairs = desilium_config::validate(&mut next).repairs;
    let report = CommitReport {
        changed: next != *current,
        repairs,
        backup: None,
    };
    (report, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_edit_reports_changes_and_repairs() {
        let current = DesiliumConfig::default();
        let (report, next) = edit_in_memory(&current, |cfg| cfg.appearance.cursor_size = 500);
        assert!(report.changed);
        assert_eq!(next.appearance.cursor_size, 128);
        assert_eq!(report.repairs.len(), 1);
        assert!(report.backup.is_none());
    }

    #[test]
    fn in_memory_no_op_edit_reports_no_change() {
        let current = DesiliumConfig::default();
        let (report, next) = edit_in_memory(&current, |_| {});
        assert!(!report.changed);
        assert_eq!(next, current);
    }
}
