//! Fixed argv operations against established desktop services, never a shell.
use std::{
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    pub volume: Option<u8>,
    pub muted: bool,
    pub microphone: Option<bool>,
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    pub brightness: Option<u8>,
    pub profile: Option<String>,
    pub notice: String,
}
#[derive(Debug, Clone)]
pub enum Action {
    Refresh,
    Volume(u8),
    Mute(bool),
    Microphone(bool),
    Wifi(bool),
    Bluetooth(bool),
    Brightness(u8),
    Profile(String),
}
pub struct Worker(mpsc::SyncSender<Action>);
impl Worker {
    pub fn spawn(sender: calloop::channel::Sender<State>) -> Result<Self, String> {
        let (commands, receiver) = mpsc::sync_channel(8);
        std::thread::Builder::new()
            .name("desilium-controls".into())
            .spawn(move || {
                while let Ok(action) = receiver.recv() {
                    let error = execute(action).err();
                    let mut state = read();
                    if let Some(error) = error {
                        state.notice = error;
                    }
                    if sender.send(state).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Self(commands))
    }
    pub fn send(&self, action: Action) -> Result<(), String> {
        self.0
            .try_send(action)
            .map_err(|_| "System controls are busy. Try again in a moment.".into())
    }
}
fn execute(action: Action) -> Result<(), String> {
    let enabled = |value| if value { "on" } else { "off" };
    match action {
        Action::Refresh => return Ok(()),
        Action::Volume(percent) => output(
            "wpctl",
            &[
                "set-volume",
                "@DEFAULT_AUDIO_SINK@",
                &format!("{}%", percent.min(100)),
                "--limit",
                "1.0",
            ],
        ),
        Action::Mute(muted) => output(
            "wpctl",
            &[
                "set-mute",
                "@DEFAULT_AUDIO_SINK@",
                if muted { "1" } else { "0" },
            ],
        ),
        Action::Microphone(muted) => output(
            "wpctl",
            &[
                "set-mute",
                "@DEFAULT_AUDIO_SOURCE@",
                if muted { "1" } else { "0" },
            ],
        ),
        Action::Wifi(value) => output("nmcli", &["radio", "wifi", enabled(value)]),
        Action::Bluetooth(value) => output("bluetoothctl", &["power", enabled(value)]),
        Action::Brightness(percent) => output(
            "brightnessctl",
            &[
                "-c",
                "backlight",
                "set",
                &format!("{}%", percent.clamp(5, 100)),
            ],
        ),
        Action::Profile(profile) => {
            if !matches!(profile.as_str(), "balanced" | "power-saver" | "performance") {
                return Err("That power profile does not exist.".into());
            }
            output("powerprofilesctl", &["set", &profile])
        }
    }
    .map(|_| ())
}
fn read() -> State {
    let volume = output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).ok();
    let microphone = output("wpctl", &["get-volume", "@DEFAULT_AUDIO_SOURCE@"]).ok();
    let wifi = output("nmcli", &["-t", "-f", "WIFI", "general"])
        .ok()
        .and_then(|text| match text.trim() {
            "enabled" => Some(true),
            "disabled" => Some(false),
            _ => None,
        });
    let bluetooth = output("bluetoothctl", &["show"]).ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.trim()
                .strip_prefix("Powered: ")
                .map(|value| value == "yes")
        })
    });
    let brightness = output("brightnessctl", &["-c", "backlight", "-m"])
        .ok()
        .and_then(|text| {
            text.split(',').find_map(|part| {
                part.trim()
                    .strip_suffix('%')
                    .and_then(|part| part.parse::<u8>().ok())
            })
        });
    State {
        volume: volume.as_deref().and_then(parse_volume),
        muted: volume.is_some_and(|text| text.contains("MUTED")),
        microphone: microphone.map(|text| text.contains("MUTED")),
        wifi,
        bluetooth,
        brightness,
        profile: output("powerprofilesctl", &["get"]).ok(),
        notice: String::new(),
    }
}
pub fn parse_volume(text: &str) -> Option<u8> {
    let value = text.split_whitespace().nth(1)?.parse::<f64>().ok()?;
    value
        .is_finite()
        .then(|| (value * 100.0).round().clamp(0.0, 100.0) as u8)
}
fn output(program: &str, args: &[&str]) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{program} is unavailable: {error}"))?;
    let start = Instant::now();
    loop {
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            break;
        }
        if start.elapsed() > Duration::from_secs(3) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "{program} did not respond. The session is still running."
            ));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let result = child
        .wait_with_output()
        .map_err(|error| error.to_string())?;
    if !result.status.success() {
        return Err(format!(
            "{program}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&result.stdout).trim().to_owned())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn volume_parser_handles_muted_invalid_and_amplified_values() {
        assert_eq!(parse_volume("Volume: 0.42 [MUTED]"), Some(42));
        assert_eq!(parse_volume("Volume: 1.80"), Some(100));
        assert_eq!(parse_volume("Volume: NaN"), None);
        assert_eq!(parse_volume("missing"), None);
    }
    #[test]
    fn unknown_power_profile_never_runs_a_program() {
        assert!(execute(Action::Profile("bad; command".into())).is_err());
    }
}
