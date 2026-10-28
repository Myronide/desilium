//! Brings KWin in line with a [`Plan`]: only what changed since the last success is written.

mod kconfig;
mod kwin;
mod pointer;

use std::collections::HashMap;

use zbus::blocking::Connection;

pub use kconfig::{KConfigWriter, PROGRAM as KWRITECONFIG, argv};
pub use pointer::{BusDevices, InputDevices, PointerOutcome, Property, apply_pointer, mice};

use crate::error::KwinError;
use crate::plan::{EntryId, Plan, PointerPlan, Value};
use crate::shortcuts::{self, Binding};

/// What one [`Applier::apply`] call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Outcome {
    /// Configuration keys written.
    pub written: usize,
    /// Steps that failed and will be tried again by the next call.
    pub failed: usize,
}

impl Outcome {
    /// Nothing is left to retry.
    pub fn complete(self) -> bool {
        self.failed == 0
    }
}

/// Remembers what was applied so a repeated call only does the difference.
pub struct Applier {
    writer: KConfigWriter,
    connection: Option<Connection>,
    written: HashMap<EntryId, Value>,
    desktops: Option<u8>,
    bindings: Option<Vec<Binding>>,
    reconfigure_due: bool,
    /// The mouse plan that is wanted, whether or not it was applied yet.
    pointer_wanted: Option<PointerPlan>,
    /// The plan and the device list the mice were last set up with successfully.
    pointer_done: Option<(PointerPlan, Vec<String>)>,
}

impl Applier {
    /// An applier that writes with `writer`.
    pub fn new(writer: KConfigWriter) -> Self {
        Self {
            writer,
            connection: None,
            written: HashMap::new(),
            desktops: None,
            bindings: None,
            reconfigure_due: false,
            pointer_wanted: None,
            pointer_done: None,
        }
    }

    /// Looks for mice that appeared since the last pass and sets them up. Cheap when nothing
    /// changed: one D-Bus read. Failures are left for the next pass and only logged.
    pub fn rescan_pointer(&mut self) -> PointerOutcome {
        let mut outcome = Outcome::default();
        let connection = self.connection();
        self.set_pointer(connection.as_ref(), &mut outcome)
    }

    /// Applies the difference between `plan`/`bindings` and what was applied before.
    ///
    /// A step that fails is logged once per call and left for the next call. Failure of one
    /// step never stops the others: a missing `kglobalacceld` must not keep the cursor size
    /// from being applied.
    pub fn apply(&mut self, plan: &Plan, bindings: &[Binding]) -> Outcome {
        let mut outcome = Outcome::default();
        self.write_entries(plan, &mut outcome);
        let connection = self.connection();
        self.reconfigure(connection.as_ref(), &mut outcome);
        self.set_desktops(connection.as_ref(), plan.desktops, &mut outcome);
        self.pointer_wanted = Some(plan.pointer);
        self.set_pointer(connection.as_ref(), &mut outcome);
        self.register_shortcuts(connection.as_ref(), bindings, &mut outcome);
        outcome
    }

    fn write_entries(&mut self, plan: &Plan, outcome: &mut Outcome) {
        for entry in &plan.entries {
            if self.written.get(&entry.id) == Some(&entry.value) {
                continue;
            }
            match self.writer.write(entry) {
                Ok(()) => {
                    self.written.insert(entry.id.clone(), entry.value.clone());
                    outcome.written += 1;
                    self.reconfigure_due = true;
                }
                Err(error) => {
                    tracing::warn!(
                        file = entry.id.file,
                        group = %entry.id.group,
                        key = %entry.id.key,
                        %error,
                        "configuration key was not written"
                    );
                    outcome.failed += 1;
                }
            }
        }
    }

    fn connection(&mut self) -> Option<Connection> {
        if self.connection.is_none() {
            match Connection::session() {
                Ok(connection) => self.connection = Some(connection),
                Err(error) => tracing::warn!(%error, "no session bus"),
            }
        }
        self.connection.clone()
    }

    fn reconfigure(&mut self, connection: Option<&Connection>, outcome: &mut Outcome) {
        if !self.reconfigure_due {
            return;
        }
        match connection.map(kwin::reconfigure) {
            Some(Ok(())) => self.reconfigure_due = false,
            Some(Err(error)) => fail(outcome, "KWin did not reconfigure", &error),
            None => outcome.failed += 1,
        }
    }

    fn set_desktops(&mut self, connection: Option<&Connection>, count: u8, outcome: &mut Outcome) {
        if self.desktops == Some(count) {
            return;
        }
        match connection.map(|c| kwin::set_desktop_count(c, count)) {
            Some(Ok(())) => self.desktops = Some(count),
            Some(Err(error)) => fail(outcome, "virtual desktops were not set", &error),
            None => outcome.failed += 1,
        }
    }

    /// Sets the wanted mouse plan on every mouse unless that was done for the same plan and the
    /// same devices already.
    fn set_pointer(
        &mut self,
        connection: Option<&Connection>,
        outcome: &mut Outcome,
    ) -> PointerOutcome {
        let (Some(plan), Some(connection)) = (self.pointer_wanted, connection) else {
            if self.pointer_wanted.is_some() {
                outcome.failed += 1;
            }
            return PointerOutcome::default();
        };
        let bus = BusDevices(connection);
        let names = match bus.sys_names() {
            Ok(mut names) => {
                names.sort();
                names
            }
            Err(error) => {
                fail(outcome, "input devices could not be listed", &error);
                return PointerOutcome::default();
            }
        };
        if self.pointer_done.as_ref() == Some(&(plan, names.clone())) {
            return PointerOutcome::default();
        }
        let done = apply_pointer(&bus, &names, &plan);
        if done.failed == 0 {
            self.pointer_done = Some((plan, names));
        } else {
            outcome.failed += done.failed;
        }
        done
    }

    fn register_shortcuts(
        &mut self,
        connection: Option<&Connection>,
        bindings: &[Binding],
        outcome: &mut Outcome,
    ) {
        if self.bindings.as_deref() == Some(bindings) {
            return;
        }
        match connection.map(|c| shortcuts::register(c, bindings)) {
            Some(Ok(0)) => self.bindings = Some(bindings.to_vec()),
            Some(Ok(failed)) => outcome.failed += failed,
            Some(Err(error)) => fail(outcome, "shortcuts were not registered", &error),
            None => outcome.failed += 1,
        }
    }
}

fn fail(outcome: &mut Outcome, what: &str, error: &KwinError) {
    tracing::warn!(%error, "{what}");
    outcome.failed += 1;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::ConfigEntry;

    fn counting_writer(dir: &std::path::Path) -> (KConfigWriter, std::path::PathBuf) {
        let log = dir.join("calls");
        let program = dir.join("writer");
        let body = format!("echo call >> '{}'\n", log.display());
        std::fs::write(&program, body).expect("script");
        (KConfigWriter::with_script(program), log)
    }

    fn calls(log: &std::path::Path) -> usize {
        std::fs::read_to_string(log).map_or(0, |text| text.lines().count())
    }

    #[test]
    fn unchanged_entries_are_not_written_twice() {
        let dir = tempfile::tempdir().expect("temp dir");
        let (writer, log) = counting_writer(dir.path());
        let mut applier = Applier::new(writer);
        let first = Plan {
            entries: vec![
                ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "ClickToFocus"),
                ConfigEntry::flag("kwinrc", "Plugins", "blurEnabled", true),
            ],
            desktops: 4,
            pointer: PointerPlan::default(),
        };
        let mut outcome = Outcome::default();
        applier.write_entries(&first, &mut outcome);
        assert_eq!((outcome.written, outcome.failed), (2, 0));
        assert_eq!(calls(&log), 2);

        let mut again = Outcome::default();
        applier.write_entries(&first, &mut again);
        assert_eq!(again.written, 0);
        assert_eq!(calls(&log), 2, "nothing is written when nothing changed");

        let changed = Plan {
            entries: vec![
                ConfigEntry::text("kwinrc", "Windows", "FocusPolicy", "FocusFollowsMouse"),
                ConfigEntry::flag("kwinrc", "Plugins", "blurEnabled", true),
            ],
            desktops: 4,
            pointer: PointerPlan::default(),
        };
        let mut third = Outcome::default();
        applier.write_entries(&changed, &mut third);
        assert_eq!(third.written, 1, "only the changed key");
        assert_eq!(calls(&log), 3);
    }

    #[test]
    fn a_failed_write_is_retried_next_time() {
        let mut applier = Applier::new(KConfigWriter::with_program("/nonexistent/kwriteconfig6"));
        let plan = Plan {
            entries: vec![ConfigEntry::text(
                "kwinrc",
                "Windows",
                "FocusPolicy",
                "ClickToFocus",
            )],
            desktops: 4,
            pointer: PointerPlan::default(),
        };
        let mut outcome = Outcome::default();
        applier.write_entries(&plan, &mut outcome);
        assert_eq!((outcome.written, outcome.failed), (0, 1));
        assert!(!outcome.complete());
        assert!(
            applier.written.is_empty(),
            "failure is not remembered as success"
        );
    }
}
