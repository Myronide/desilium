//! Shared, validated settings and a bounded local control protocol.
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
    time::Duration,
};

pub const MAX_MESSAGE: u64 = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FocusPolicy {
    #[default]
    Click,
    Sloppy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub focus: FocusPolicy,
    pub raise_on_focus: bool,
    pub snap_enabled: bool,
    pub snap_distance: u16,
    pub gap: u16,
    pub keyboard_repeat_delay: u16,
    pub keyboard_repeat_rate: u16,
    pub background: String,
    pub terminal: String,
    pub shortcuts: Shortcuts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Shortcuts {
    pub terminal: String,
    pub settings: String,
    pub close: String,
    pub cycle: String,
    pub maximize: String,
    pub restore: String,
    pub left: String,
    pub right: String,
    pub quit: String,
}
impl Default for Shortcuts {
    fn default() -> Self {
        Self {
            terminal: "Super+Return".into(),
            settings: "Super+comma".into(),
            close: "Alt+F4".into(),
            cycle: "Alt+Tab".into(),
            maximize: "Super+Up".into(),
            restore: "Super+Down".into(),
            left: "Super+Left".into(),
            right: "Super+Right".into(),
            quit: "Super+Shift+Escape".into(),
        }
    }
}
impl Shortcuts {
    pub fn entries(&self) -> [(&str, &str); 9] {
        [
            ("terminal", &self.terminal),
            ("settings", &self.settings),
            ("close", &self.close),
            ("cycle", &self.cycle),
            ("maximize", &self.maximize),
            ("restore", &self.restore),
            ("left", &self.left),
            ("right", &self.right),
            ("quit", &self.quit),
        ]
    }
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            focus: FocusPolicy::Click,
            raise_on_focus: true,
            snap_enabled: true,
            snap_distance: 24,
            gap: 12,
            keyboard_repeat_delay: 300,
            keyboard_repeat_rate: 30,
            background: "#101827".into(),
            terminal: default_terminal(),
            shortcuts: Shortcuts::default(),
        }
    }
}

fn default_terminal() -> String {
    let paths: Vec<_> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for program in ["foot", "kitty", "alacritty", "weston-terminal"] {
        if paths.iter().any(|p: &PathBuf| p.join(program).is_file()) {
            return program.into();
        }
    }
    "weston-terminal".into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chord {
    pub super_: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}
impl Chord {
    pub fn parse(value: &str) -> Result<Self, String> {
        let mut chord = Self {
            super_: false,
            ctrl: false,
            alt: false,
            shift: false,
            key: String::new(),
        };
        for part in value.split('+') {
            match part.to_ascii_lowercase().as_str() {
                "super" if !chord.super_ => chord.super_ = true,
                "ctrl" if !chord.ctrl => chord.ctrl = true,
                "alt" if !chord.alt => chord.alt = true,
                "shift" if !chord.shift => chord.shift = true,
                key if chord.key.is_empty() && valid_key(key) => chord.key = key.into(),
                _ => return Err(format!("Ungültiges Tastenkürzel: {value}")),
            }
        }
        if chord.key.is_empty() || !(chord.super_ || chord.ctrl || chord.alt) {
            return Err(format!(
                "Kürzel braucht Super, Ctrl oder Alt und eine Taste: {value}"
            ));
        }
        Ok(chord)
    }
}
fn valid_key(key: &str) -> bool {
    (key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric())
        || matches!(
            key,
            "return" | "tab" | "escape" | "space" | "comma" | "left" | "right" | "up" | "down"
        )
        || key
            .strip_prefix('f')
            .and_then(|s| s.parse::<u8>().ok())
            .is_some_and(|n| (1..=12).contains(&n))
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if !(4..=96).contains(&self.snap_distance) {
            return Err("Einrastabstand: 4–96 px".into());
        }
        if self.gap > 64 {
            return Err("Fensterabstand: 0–64 px".into());
        }
        if !(100..=1500).contains(&self.keyboard_repeat_delay) {
            return Err("Tastenverzögerung: 100–1500 ms".into());
        }
        if !(1..=100).contains(&self.keyboard_repeat_rate) {
            return Err("Tastenwiederholung: 1–100 Hz".into());
        }
        if self.background.len() != 7
            || !self.background.starts_with('#')
            || !self.background[1..].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Hintergrund muss #RRGGBB sein".into());
        }
        if self.terminal.trim().is_empty()
            || self.terminal.contains('\0')
            || self.terminal.len() > 256
        {
            return Err(
                "Terminal muss ein ausführbarer Programmname oder Pfad sein (ohne Argumente)"
                    .into(),
            );
        }
        let mut seen = Vec::new();
        for (_, value) in self.shortcuts.entries() {
            let chord = Chord::parse(value)?;
            if seen.contains(&chord) {
                return Err(format!("Tastenkürzel doppelt vergeben: {value}"));
            }
            seen.push(chord);
        }
        Ok(())
    }
    pub fn rgba(&self) -> [f32; 4] {
        let component =
            |i| u8::from_str_radix(&self.background[i..i + 2], 16).unwrap_or(0) as f32 / 255.0;
        [component(1), component(3), component(5), 1.0]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    Fullscreen,
    Maximize,
    Left,
    Right,
    Restore,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
pub fn placement_rect(width: i32, height: i32, gap: u16, placement: Placement) -> Rect {
    let g = if placement == Placement::Fullscreen {
        0
    } else {
        i32::from(gap).min((width.min(height) / 4).max(0))
    };
    let half = (width - 3 * g).max(2) / 2;
    match placement {
        Placement::Left => Rect {
            x: g,
            y: g,
            w: half.max(1),
            h: (height - 2 * g).max(1),
        },
        Placement::Right => Rect {
            x: 2 * g + half,
            y: g,
            w: (width - 3 * g - half).max(1),
            h: (height - 2 * g).max(1),
        },
        _ => Rect {
            x: g,
            y: g,
            w: (width - 2 * g).max(1),
            h: (height - 2 * g).max(1),
        },
    }
}

pub fn config_path() -> io::Result<PathBuf> {
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or_else(|| io::Error::other("HOME und XDG_CONFIG_HOME fehlen"))?;
    Ok(root.join("wc/settings.toml"))
}
pub fn load() -> Result<Settings, String> {
    load_from(&config_path().map_err(|e| e.to_string())?)
}
pub fn load_from(path: &std::path::Path) -> Result<Settings, String> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            let settings: Settings = toml::from_str(&raw).map_err(|e| e.to_string())?;
            settings.validate()?;
            Ok(settings)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e.to_string()),
    }
}
pub fn save(settings: &Settings) -> Result<(), String> {
    save_to(settings, &config_path().map_err(|e| e.to_string())?)
}
pub fn save_to(settings: &Settings, path: &std::path::Path) -> Result<(), String> {
    use std::os::unix::fs::OpenOptionsExt;
    settings.validate()?;
    fs::create_dir_all(
        path.parent()
            .ok_or("Konfigurationspfad ohne Elternverzeichnis")?,
    )
    .map_err(|e| e.to_string())?;
    let tmp = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        file.write_all(
            toml::to_string_pretty(settings)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}
pub fn socket_path() -> io::Result<PathBuf> {
    if let Some(p) = std::env::var_os("WC_SOCKET") {
        return Ok(p.into());
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR fehlt"))?;
    Ok(PathBuf::from(runtime).join("wc-control.sock"))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Get,
    Apply(Box<Settings>),
    Place(Placement),
    Cycle,
    Close,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub focused: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub window_details: Vec<WindowInfo>,
    pub settings: Settings,
    pub windows: usize,
    pub wayland_display: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
}
pub fn request(request: &Request) -> Result<Snapshot, String> {
    use std::io::Read;
    let mut stream = UnixStream::connect(socket_path().map_err(|e| e.to_string())?)
        .map_err(|e| format!("Compositor nicht erreichbar: {e}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .map_err(|e| e.to_string())?;
    serde_json::to_writer(&mut stream, request).map_err(|e| e.to_string())?;
    stream.write_all(b"\n").map_err(|e| e.to_string())?;
    let mut raw = String::new();
    BufReader::new(stream.take(MAX_MESSAGE + 1))
        .read_line(&mut raw)
        .map_err(|e| e.to_string())?;
    if raw.len() as u64 > MAX_MESSAGE {
        return Err("Antwort zu groß".into());
    }
    let response: Response = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    if let Some(error) = response.error {
        return Err(error);
    }
    response.snapshot.ok_or_else(|| "Leere Antwort".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_conflicting_shortcuts_and_invalid_settings() {
        let mut s = Settings::default();
        s.validate().unwrap();
        s.shortcuts.close = "Alt+tab".into();
        assert!(s.validate().is_err());
        s = Settings::default();
        s.background = "#12xx00".into();
        assert!(s.validate().is_err());
        s = Settings::default();
        s.keyboard_repeat_rate = 0;
        assert!(s.validate().is_err());
        assert!(Chord::parse("Super+Super+a").is_err());
        assert_eq!(
            Chord::parse("Alt+Shift+Tab").unwrap(),
            Chord::parse("shift+alt+tab").unwrap()
        );
    }
    #[test]
    fn snapping_respects_gaps_and_odd_output_sizes() {
        let l = placement_rect(1921, 1080, 12, Placement::Left);
        let r = placement_rect(1921, 1080, 12, Placement::Right);
        assert_eq!(l.x, 12);
        assert_eq!(l.x + l.w + 12, r.x);
        assert_eq!(r.x + r.w, 1909);
        assert_eq!(l.h, 1056);
        let tiny = placement_rect(20, 20, 64, Placement::Maximize);
        assert!(tiny.w > 0 && tiny.h > 0);
    }
    #[test]
    fn persistence_roundtrip_and_invalid_file_preserved() {
        let dir = std::env::temp_dir().join(format!("wc-core-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("settings.toml");
        let mut s = Settings {
            gap: 23,
            ..Settings::default()
        };
        save_to(&s, &path).unwrap();
        assert_eq!(load_from(&path).unwrap(), s);
        s.gap = 99;
        assert!(save_to(&s, &path).is_err());
        assert_eq!(load_from(&path).unwrap().gap, 23);
        fs::write(&path, "broken = [").unwrap();
        assert!(load_from(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken = [");
        fs::remove_dir_all(dir).unwrap();
    }
}
