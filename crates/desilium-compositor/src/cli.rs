use crate::error::CompositorError;
use desilium_cursor::CursorKind;
use desilium_theme::Mode;
use std::path::PathBuf;
use std::time::Duration;

const DEFAULT_WIDTH: i32 = 1280;
const DEFAULT_HEIGHT: i32 = 800;
const DEFAULT_SCREENSHOT_DELAY_MS: u64 = 2000;
const MAX_DIMENSION: i32 = 16_384;

/// Parsed command line.
#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    /// Client programs to start, in order.
    pub commands: Vec<String>,
    /// Render offscreen instead of opening a window.
    pub headless: bool,
    pub size: (i32, i32),
    pub scale: f64,
    pub screenshot: Option<PathBuf>,
    /// `--theme dark|light`: overrides the configured theme id for screenshots and tests.
    pub theme: Option<Mode>,
    /// Directory of the configuration store (`--config-dir`); `None` means the default location.
    pub config_dir: Option<PathBuf>,
    /// `--no-config`: use built-in defaults and never touch the configuration files.
    pub no_config: bool,
    /// Quiet time after the last client activity before the screenshot is taken.
    pub screenshot_delay: Duration,
    /// Initial pointer position for headless screenshots (`--pointer X,Y[,SHAPE]`).
    pub pointer: Option<PointerStart>,
    /// `--freeze-time MS[:KINDS]`: render animations at a fixed offset (see `docs/headless.md`).
    pub freeze_time: Option<crate::animation::Freeze>,
    /// `--demo ACTION`: scripted window action for animation captures.
    pub demo: Option<crate::animation::Demo>,
    /// `--shell PATH`: the desktop shell to start, overriding the one found next to the compositor.
    pub shell: Option<PathBuf>,
    /// `--no-shell`: start no desktop shell, not even the default one.
    pub no_shell: bool,
}

/// Where the pointer starts, in logical pixels, and optionally which cursor shape it shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerStart {
    pub x: f64,
    pub y: f64,
    /// Forces a themed shape (CSS name such as `text` or `ew-resize`).
    pub shape: Option<CursorKind>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            headless: false,
            size: (DEFAULT_WIDTH, DEFAULT_HEIGHT),
            scale: 1.0,
            screenshot: None,
            theme: None,
            config_dir: None,
            no_config: false,
            screenshot_delay: Duration::from_millis(DEFAULT_SCREENSHOT_DELAY_MS),
            pointer: None,
            freeze_time: None,
            demo: None,
            shell: None,
            no_shell: false,
        }
    }
}

/// Parses the arguments after the program name.
pub fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, CompositorError> {
    let mut parsed = Args::default();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--command" | "-c" => parsed.commands.push(value(&mut args, &arg)?),
            "--headless" => parsed.headless = true,
            "--size" => parsed.size = parse_size(&value(&mut args, &arg)?)?,
            "--scale" => parsed.scale = parse_scale(&value(&mut args, &arg)?)?,
            "--theme" => parsed.theme = Some(parse_theme(&value(&mut args, &arg)?)?),
            "--config-dir" => parsed.config_dir = Some(PathBuf::from(value(&mut args, &arg)?)),
            "--no-config" => parsed.no_config = true,
            "--pointer" => parsed.pointer = Some(parse_pointer(&value(&mut args, &arg)?)?),
            "--freeze-time" => {
                parsed.freeze_time = Some(
                    crate::animation::parse_freeze(&value(&mut args, &arg)?)
                        .ok_or_else(|| usage("--freeze-time needs MS or MS:open,focus,..."))?,
                );
            }
            "--demo" => {
                parsed.demo = Some(
                    crate::animation::Demo::parse(&value(&mut args, &arg)?)
                        .ok_or_else(|| usage("--demo needs one of: maximize, close"))?,
                );
            }
            "--shell" => parsed.shell = Some(PathBuf::from(value(&mut args, &arg)?)),
            "--no-shell" => parsed.no_shell = true,
            "--screenshot" => parsed.screenshot = Some(PathBuf::from(value(&mut args, &arg)?)),
            "--screenshot-delay" => {
                let millis = value(&mut args, &arg)?
                    .parse::<u64>()
                    .map_err(|_| usage("--screenshot-delay needs milliseconds"))?;
                parsed.screenshot_delay = Duration::from_millis(millis);
            }
            other => return Err(usage(format!("unknown argument: {other}"))),
        }
    }
    if parsed.screenshot.is_some() && !parsed.headless {
        return Err(usage("--screenshot needs --headless"));
    }
    if parsed.no_config && parsed.config_dir.is_some() {
        return Err(usage("--no-config and --config-dir cannot be combined"));
    }
    if parsed.no_shell && parsed.shell.is_some() {
        return Err(usage("--no-shell and --shell cannot be combined"));
    }
    Ok(parsed)
}

impl Args {
    /// The configuration directory for this run, or `None` when configuration is disabled.
    ///
    /// `--no-config` disables it. An explicit `--config-dir` always wins over the
    /// default. Headless runs (screenshots, CI, Docker) default to disabled so they
    /// never read or write the real user configuration.
    pub fn resolved_config_dir(&self, default_dir: PathBuf) -> Option<PathBuf> {
        if self.no_config {
            return None;
        }
        if let Some(dir) = &self.config_dir {
            return Some(dir.clone());
        }
        (!self.headless).then_some(default_dir)
    }
}

fn value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, CompositorError> {
    args.next()
        .ok_or_else(|| usage(format!("{flag} needs a value")))
}

fn usage(message: impl Into<String>) -> CompositorError {
    CompositorError::Usage(message.into())
}

fn parse_theme(text: &str) -> Result<Mode, CompositorError> {
    match text {
        "dark" => Ok(Mode::Dark),
        "light" => Ok(Mode::Light),
        _ => Err(usage("--theme needs dark or light")),
    }
}

fn parse_size(text: &str) -> Result<(i32, i32), CompositorError> {
    let parsed = text
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<i32>().ok()?, h.parse::<i32>().ok()?)));
    match parsed {
        Some((w, h)) if (1..=MAX_DIMENSION).contains(&w) && (1..=MAX_DIMENSION).contains(&h) => {
            Ok((w, h))
        }
        _ => Err(usage("--size needs WxH, e.g. 1280x800")),
    }
}

fn parse_pointer(text: &str) -> Result<PointerStart, CompositorError> {
    let message = "--pointer needs X,Y or X,Y,SHAPE, e.g. 400,300,text";
    let mut parts = text.splitn(3, ',');
    let coordinate = |part: Option<&str>| {
        part.and_then(|part| part.trim().parse::<f64>().ok())
            .filter(|value| value.is_finite())
    };
    let x = coordinate(parts.next()).ok_or_else(|| usage(message))?;
    let y = coordinate(parts.next()).ok_or_else(|| usage(message))?;
    let shape = match parts.next() {
        None => None,
        Some(name) => Some(CursorKind::from_css_name(name.trim()).ok_or_else(|| usage(message))?),
    };
    Ok(PointerStart { x, y, shape })
}

fn parse_scale(text: &str) -> Result<f64, CompositorError> {
    match text.parse::<f64>() {
        Ok(scale) if scale.is_finite() && scale > 0.0 && scale <= 8.0 => Ok(scale),
        _ => Err(usage("--scale needs a number in (0, 8]")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(items: &[&str]) -> Result<Args, CompositorError> {
        parse_args(items.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn defaults() {
        let args = parse(&[]).unwrap();
        assert_eq!(args, Args::default());
        assert_eq!(args.size, (1280, 800));
    }

    #[test]
    fn parses_headless_options_and_repeated_commands() {
        let args = parse(&[
            "--headless",
            "--size",
            "640x480",
            "--scale",
            "1.5",
            "--screenshot",
            "a.png",
            "--screenshot-delay",
            "500",
            "--command",
            "foot",
            "-c",
            "kitty",
        ])
        .unwrap();
        assert!(args.headless);
        assert_eq!(args.size, (640, 480));
        assert_eq!(args.scale, 1.5);
        assert_eq!(args.screenshot, Some(PathBuf::from("a.png")));
        assert_eq!(args.screenshot_delay, Duration::from_millis(500));
        assert_eq!(args.commands, ["foot", "kitty"]);
    }

    #[test]
    fn parses_pointer_with_and_without_shape() {
        let plain = parse(&["--pointer", "10.5,20"]).unwrap();
        assert_eq!(
            plain.pointer,
            Some(PointerStart {
                x: 10.5,
                y: 20.0,
                shape: None
            })
        );
        let shaped = parse(&["--pointer", "1,2,text"]).unwrap();
        assert_eq!(shaped.pointer.and_then(|p| p.shape), Some(CursorKind::Text));
        assert!(parse(&["--pointer", "1"]).is_err());
        assert!(parse(&["--pointer", "1,2,sparkle"]).is_err());
        assert!(parse(&["--pointer", "a,b"]).is_err());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse(&["--size", "0x10"]).is_err());
        assert!(parse(&["--size", "abc"]).is_err());
        assert!(parse(&["--scale", "-1"]).is_err());
        assert!(parse(&["--scale", "nan"]).is_err());
        assert!(parse(&["--command"]).is_err());
        assert!(parse(&["--bogus"]).is_err());
        assert!(parse(&["--screenshot", "a.png"]).is_err());
    }

    #[test]
    fn theme_flag_is_an_optional_override() {
        assert_eq!(parse(&[]).unwrap().theme, None);
        assert_eq!(
            parse(&["--theme", "light"]).unwrap().theme,
            Some(Mode::Light)
        );
        assert_eq!(parse(&["--theme", "dark"]).unwrap().theme, Some(Mode::Dark));
        assert!(parse(&["--theme", "sepia"]).is_err());
    }

    #[test]
    fn shell_flags_parse_and_conflict() {
        assert_eq!(parse(&[]).unwrap().shell, None);
        assert!(!parse(&[]).unwrap().no_shell);
        let explicit = parse(&["--shell", "/opt/desilium/desilium-shell"]).unwrap();
        assert_eq!(
            explicit.shell,
            Some(PathBuf::from("/opt/desilium/desilium-shell"))
        );
        assert!(parse(&["--no-shell"]).unwrap().no_shell);
        assert!(parse(&["--no-shell", "--shell", "x"]).is_err());
        assert!(parse(&["--shell"]).is_err());
    }

    #[test]
    fn config_flags_parse_and_conflict() {
        let args = parse(&["--config-dir", "/tmp/cfg"]).unwrap();
        assert_eq!(args.config_dir, Some(PathBuf::from("/tmp/cfg")));
        assert!(parse(&["--no-config"]).unwrap().no_config);
        assert!(parse(&["--no-config", "--config-dir", "/tmp/cfg"]).is_err());
        assert!(parse(&["--config-dir"]).is_err());
    }

    #[test]
    fn windowed_run_uses_the_default_config_dir() {
        let args = parse(&[]).unwrap();
        assert_eq!(
            args.resolved_config_dir(PathBuf::from("/home/u/.config/desilium")),
            Some(PathBuf::from("/home/u/.config/desilium"))
        );
    }

    #[test]
    fn headless_run_disables_config_unless_a_dir_is_given() {
        let default = PathBuf::from("/home/u/.config/desilium");
        let headless = parse(&["--headless"]).unwrap();
        assert_eq!(headless.resolved_config_dir(default.clone()), None);
        let explicit = parse(&["--headless", "--config-dir", "/tmp/cfg"]).unwrap();
        assert_eq!(
            explicit.resolved_config_dir(default),
            Some(PathBuf::from("/tmp/cfg"))
        );
    }

    #[test]
    fn no_config_disables_the_store_even_when_headless_is_off() {
        let args = parse(&["--no-config"]).unwrap();
        assert_eq!(
            args.resolved_config_dir(PathBuf::from("/home/u/.config/desilium")),
            None
        );
    }
}
