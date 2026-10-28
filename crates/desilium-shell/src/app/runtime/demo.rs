//! Scripted launcher open/close/reopen for headless runs, where nobody can press a key.

use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use crate::app::{Shell, runtime::timers::Due};

/// One step of the script.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Step {
    /// Open the launcher with this query.
    Open(String),
    /// Close it.
    Close,
}

/// Steps with the delay after the previous one.
pub(crate) fn launcher_cycle() -> VecDeque<(Duration, Step)> {
    let second = Duration::from_secs(1);
    VecDeque::from([
        (second * 2, Step::Open(String::new())),
        (second, Step::Close),
        (second, Step::Open("co".to_owned())),
    ])
}

impl Shell {
    /// Runs the step that is due and arms the next one.
    pub(in crate::app) fn demo_step(&mut self) {
        let Some((_, step)) = self.demo_steps.pop_front() else {
            return;
        };
        tracing::info!(?step, "demo step");
        match step {
            Step::Open(query) => self.open_launcher(&query),
            Step::Close => self.close_launcher(),
        }
        if let Some((delay, _)) = self.demo_steps.front() {
            self.deadlines.set(Due::Demo, Instant::now() + *delay);
        }
    }

    /// Arms the first step of the script, if any.
    pub(in crate::app) fn start_demo(&mut self) {
        if let Some((delay, _)) = self.demo_steps.front() {
            self.deadlines.set(Due::Demo, Instant::now() + *delay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_opens_closes_and_reopens() {
        let steps: Vec<_> = launcher_cycle().into_iter().map(|(_, s)| s).collect();
        assert_eq!(
            steps,
            [
                Step::Open(String::new()),
                Step::Close,
                Step::Open("co".into())
            ]
        );
    }
}
