//! Command-line flags.

use std::path::PathBuf;

use crate::registry::Page;

/// Default window size in logical pixels.
pub const DEFAULT_SIZE: (u32, u32) = (1280, 820);

/// Usage text for `--help`.
pub const USAGE: &str = "\
desilium-settings: the graphical settings of Desilium

Usage: desilium-settings [options]

Options:
  --page NAME          Start on a page: appearance, desktop, keyboard or about
  --search TEXT        Start with this search
  --config-dir DIR     Use this settings directory instead of ~/.config/desilium
  --wallpaper-dir DIR  Look for wallpapers only here (same as DESILIUM_WALLPAPER_DIR)
  --size WxH           Initial window size (default 1280x820)
  --set KEY=VALUE      Change a setting before the window opens (needs --config-dir,
                       so it can never touch your real settings; repeatable)
  --do ACTION[:ARG]    Run a UI action as if it was clicked, before the first frame
                       (for demos and screenshots; repeatable)
  --no-window          Apply --set and --do, then exit without opening a window
                       (scripting and tests)
  --help               Show this text
  --version            Show the version
";

/// What the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    /// First page.
    pub page: Page,
    /// First search text.
    pub search: Option<String>,
    /// Settings directory override.
    pub config_dir: Option<PathBuf>,
    /// Wallpaper directory override.
    pub wallpaper_dir: Option<PathBuf>,
    /// Initial window size.
    pub size: (u32, u32),
    /// `KEY=VALUE` pairs applied at startup.
    pub sets: Vec<String>,
    /// `ACTION[:ARG]` strings run before the first frame.
    pub actions: Vec<String>,
    /// Exit after applying `--set` and `--do` instead of opening a window.
    pub no_window: bool,
}

/// Outcome of parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Run the app.
    Run(Cli),
    /// Print the usage text.
    Help,
    /// Print the version.
    Version,
}

/// A flag that cannot be used, phrased for the terminal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CliError(pub String);

fn error(message: impl Into<String>) -> CliError {
    CliError(message.into())
}

/// Parses `args` (without the program name).
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Parsed, CliError> {
    let mut cli = Cli {
        page: Page::Appearance,
        search: None,
        config_dir: None,
        wallpaper_dir: None,
        size: DEFAULT_SIZE,
        sets: Vec::new(),
        actions: Vec::new(),
        no_window: false,
    };
    let mut args = args.into_iter();
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| error(format!("{flag} needs a value")))
        };
        match flag.as_str() {
            "--help" | "-h" => return Ok(Parsed::Help),
            "--version" | "-V" => return Ok(Parsed::Version),
            "--page" => {
                let name = value()?;
                cli.page = Page::from_slug(&name)
                    .ok_or_else(|| error(format!("unknown page \"{name}\"")))?;
            }
            "--search" => cli.search = Some(value()?),
            "--config-dir" => cli.config_dir = Some(PathBuf::from(value()?)),
            "--wallpaper-dir" => cli.wallpaper_dir = Some(PathBuf::from(value()?)),
            "--size" => cli.size = parse_size(&value()?)?,
            "--set" => cli.sets.push(value()?),
            "--do" => cli.actions.push(value()?),
            "--no-window" => cli.no_window = true,
            other => return Err(error(format!("unknown option {other} (try --help)"))),
        }
    }
    if !cli.sets.is_empty() && cli.config_dir.is_none() {
        return Err(error(
            "--set needs --config-dir, so it can never change your real settings",
        ));
    }
    Ok(Parsed::Run(cli))
}

fn parse_size(text: &str) -> Result<(u32, u32), CliError> {
    let bad = || error(format!("size \"{text}\" must look like 1280x820"));
    let (width, height) = text.split_once('x').ok_or_else(bad)?;
    let width: u32 = width.parse().map_err(|_| bad())?;
    let height: u32 = height.parse().map_err(|_| bad())?;
    if width == 0 || height == 0 {
        return Err(bad());
    }
    Ok((width, height))
}

/// Splits `ACTION[:ARG]`.
pub fn split_action(text: &str) -> (&str, Option<&str>) {
    match text.split_once(':') {
        Some((action, arg)) => (action, Some(arg)),
        None => (text, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<Parsed, CliError> {
        parse(args.iter().map(|arg| (*arg).to_owned()))
    }

    fn run(args: &[&str]) -> Cli {
        match parse_args(args) {
            Ok(Parsed::Run(cli)) => cli,
            other => panic!("expected Run, got {other:?}"),
        }
    }

    #[test]
    fn defaults() {
        let cli = run(&[]);
        assert_eq!(cli.page, Page::Appearance);
        assert_eq!(cli.size, DEFAULT_SIZE);
        assert!(cli.config_dir.is_none() && cli.search.is_none());
    }

    #[test]
    fn flags_are_parsed() {
        let cli = run(&[
            "--page",
            "keyboard",
            "--search",
            "alt tab",
            "--config-dir",
            "/tmp/c",
            "--size",
            "900x600",
            "--set",
            "appearance.accent=#FF00AA",
            "--do",
            "set:appearance.theme=desilium-light",
        ]);
        assert_eq!(cli.page, Page::Keyboard);
        assert_eq!(cli.search.as_deref(), Some("alt tab"));
        assert_eq!(cli.size, (900, 600));
        assert_eq!(cli.sets, ["appearance.accent=#FF00AA"]);
        assert_eq!(
            split_action(&cli.actions[0]),
            ("set", Some("appearance.theme=desilium-light"))
        );
    }

    #[test]
    fn help_and_version() {
        assert_eq!(parse_args(&["--help"]), Ok(Parsed::Help));
        assert_eq!(parse_args(&["-V"]), Ok(Parsed::Version));
    }

    #[test]
    fn bad_input_is_a_message_not_a_panic() {
        assert!(parse_args(&["--page", "nope"]).is_err());
        assert!(parse_args(&["--page"]).is_err());
        assert!(parse_args(&["--size", "10"]).is_err());
        assert!(parse_args(&["--size", "0x10"]).is_err());
        assert!(parse_args(&["--bogus"]).is_err());
    }

    #[test]
    fn set_requires_a_config_dir() {
        let error = parse_args(&["--set", "a=b"]).expect_err("needs config dir");
        assert!(error.0.contains("--config-dir"));
        assert!(parse_args(&["--set", "a=b", "--config-dir", "/x"]).is_ok());
    }

    #[test]
    fn no_window_flag() {
        assert!(run(&["--no-window"]).no_window);
        assert!(!run(&[]).no_window);
    }

    #[test]
    fn actions_without_argument() {
        assert_eq!(split_action("undo"), ("undo", None));
    }
}
