//! Command line of `desilium-shell`.

use std::{path::PathBuf, time::Duration};

use desilium_theme::Mode;

/// Usage text printed for `--help` and after a parse error.
pub(crate) const USAGE: &str = "\
desilium-shell: top bar, dock and launcher

USAGE:
    desilium-shell [OPTIONS]

OPTIONS:
    --open-launcher           open the launcher right after start (demos, screenshots)
    --launcher-query TEXT     put TEXT into the launcher's search field (with --open-launcher)
    --theme dark|light        override the configured theme
    --config-dir DIR          configuration directory (default: $XDG_CONFIG_HOME/desilium)
    --demo-hover SPOT         hover dock:N (0 = launcher button), bar:clock or bar:status once shown (screenshots)
    --demo-launcher-cycle     open, close and reopen the launcher on a timer (checks re-mapping; demos)
    --exit-after SECS         quit after SECS seconds and print CPU statistics
    -h, --help                print this help
";

/// Element that `--demo-hover` points the synthetic pointer at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DemoHover {
    /// App cell `n` of the dock (0 is the launcher button).
    Dock(usize),
    /// A spot on the bar.
    Bar(BarSpot),
}

/// Bar elements `--demo-hover` can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BarSpot {
    /// The date and time button.
    Clock,
    /// The status group (quick settings).
    Status,
}

/// Parsed options.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Args {
    /// Open the launcher at start.
    pub open_launcher: bool,
    /// Initial launcher query.
    pub launcher_query: Option<String>,
    /// Theme mode override.
    pub theme: Option<Mode>,
    /// Configuration directory override.
    pub config_dir: Option<PathBuf>,
    /// Synthetic hover for screenshots.
    pub demo_hover: Option<DemoHover>,
    /// Open/close/reopen the launcher on a timer.
    pub demo_launcher_cycle: bool,
    /// Quit after this long.
    pub exit_after: Option<Duration>,
    /// `--help` was given.
    pub help: bool,
}

/// A command line the shell cannot use.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ArgsError {
    /// Unknown flag.
    #[error("unknown option {0}")]
    Unknown(String),
    /// A flag that needs a value got none.
    #[error("{0} needs a value")]
    MissingValue(&'static str),
    /// The value of a flag is invalid.
    #[error("invalid value for {flag}: {value}")]
    BadValue {
        /// The flag.
        flag: &'static str,
        /// What was given.
        value: String,
    },
}

fn parse_hover(value: &str) -> Option<DemoHover> {
    let (kind, rest) = value.split_once(':')?;
    match (kind, rest) {
        ("dock", index) => index.parse().ok().map(DemoHover::Dock),
        ("bar", "clock") => Some(DemoHover::Bar(BarSpot::Clock)),
        ("bar", "status") => Some(DemoHover::Bar(BarSpot::Status)),
        _ => None,
    }
}

impl Args {
    /// Parses the arguments after the program name.
    pub(crate) fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, ArgsError> {
        let mut out = Args::default();
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--open-launcher" => out.open_launcher = true,
                "--demo-launcher-cycle" => out.demo_launcher_cycle = true,
                "-h" | "--help" => out.help = true,
                "--launcher-query" => {
                    out.launcher_query = Some(value(&mut args, "--launcher-query")?)
                }
                "--config-dir" => out.config_dir = Some(value(&mut args, "--config-dir")?.into()),
                "--theme" => out.theme = Some(parse_theme(&value(&mut args, "--theme")?)?),
                "--demo-hover" => {
                    let text = value(&mut args, "--demo-hover")?;
                    out.demo_hover = Some(parse_hover(&text).ok_or(ArgsError::BadValue {
                        flag: "--demo-hover",
                        value: text,
                    })?);
                }
                "--exit-after" => {
                    let text = value(&mut args, "--exit-after")?;
                    let seconds: f64 = text
                        .parse()
                        .ok()
                        .filter(|s: &f64| s.is_finite() && *s >= 0.0)
                        .ok_or(ArgsError::BadValue {
                            flag: "--exit-after",
                            value: text,
                        })?;
                    out.exit_after = Some(Duration::from_secs_f64(seconds));
                }
                _ => return Err(ArgsError::Unknown(flag)),
            }
        }
        Ok(out)
    }
}

fn value(args: &mut impl Iterator<Item = String>, flag: &'static str) -> Result<String, ArgsError> {
    args.next().ok_or(ArgsError::MissingValue(flag))
}

fn parse_theme(text: &str) -> Result<Mode, ArgsError> {
    match text {
        "dark" => Ok(Mode::Dark),
        "light" => Ok(Mode::Light),
        _ => Err(ArgsError::BadValue {
            flag: "--theme",
            value: text.to_owned(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, ArgsError> {
        Args::parse(args.iter().map(|a| (*a).to_owned()))
    }

    #[test]
    fn no_arguments_is_the_default() {
        assert_eq!(parse(&[]).unwrap(), Args::default());
    }

    #[test]
    fn all_options() {
        let args = parse(&[
            "--open-launcher",
            "--launcher-query",
            "24*7",
            "--theme",
            "light",
            "--config-dir",
            "/tmp/x",
            "--demo-hover",
            "dock:2",
            "--exit-after",
            "1.5",
        ])
        .unwrap();
        assert!(args.open_launcher);
        assert_eq!(args.launcher_query.as_deref(), Some("24*7"));
        assert_eq!(args.theme, Some(Mode::Light));
        assert_eq!(args.config_dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(args.demo_hover, Some(DemoHover::Dock(2)));
        assert_eq!(args.exit_after, Some(Duration::from_millis(1500)));
    }

    #[test]
    fn errors_are_typed() {
        assert_eq!(parse(&["--nope"]), Err(ArgsError::Unknown("--nope".into())));
        assert_eq!(parse(&["--theme"]), Err(ArgsError::MissingValue("--theme")));
        assert!(matches!(
            parse(&["--theme", "pink"]),
            Err(ArgsError::BadValue { .. })
        ));
        assert!(matches!(
            parse(&["--demo-hover", "side:1"]),
            Err(ArgsError::BadValue { .. })
        ));
        assert!(matches!(
            parse(&["--exit-after", "-3"]),
            Err(ArgsError::BadValue { .. })
        ));
        assert!(matches!(
            parse(&["--exit-after", "x"]),
            Err(ArgsError::BadValue { .. })
        ));
    }

    #[test]
    fn help() {
        assert!(parse(&["-h"]).unwrap().help);
    }
}
