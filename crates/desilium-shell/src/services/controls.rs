//! Fixed argv operations against established desktop services, never a shell.
use std::{
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkKind {
    Ethernet,
    Wifi,
}
/// An active network connection reported by NetworkManager.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Link {
    pub kind: LinkKind,
    pub name: String,
}
/// A Wi-Fi network in range.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WifiNetwork {
    pub ssid: String,
    pub signal: u8,
    pub secured: bool,
    pub active: bool,
}
/// Most Wi-Fi networks offered in the quick settings.
pub(crate) const MAX_WIFI_NETWORKS: usize = 5;
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct State {
    /// `None` when NetworkManager is not available.
    pub links: Option<Vec<Link>>,
    pub networks: Vec<WifiNetwork>,
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
pub(crate) enum Action {
    Refresh,
    Volume(u8),
    Mute(bool),
    Microphone(bool),
    Wifi(bool),
    WifiConnect(String),
    Bluetooth(bool),
    Brightness(u8),
    Profile(String),
}
pub(crate) struct Worker(mpsc::SyncSender<Action>);
impl Worker {
    pub(crate) fn spawn(sender: calloop::channel::Sender<State>) -> Result<Self, String> {
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
    pub(crate) fn send(&self, action: Action) -> Result<(), String> {
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
        Action::WifiConnect(ssid) => {
            // An SSID is user-visible radio data: never let it become an option.
            if ssid.is_empty() || ssid.len() > 32 || ssid.starts_with('-') {
                return Err("That network name cannot be used.".into());
            }
            output_within(
                "nmcli",
                &["device", "wifi", "connect", &ssid],
                CONNECT_TIMEOUT,
            )
        }
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
    let links = output("nmcli", &["-t", "-f", "TYPE,STATE,CONNECTION", "device"])
        .ok()
        .map(|text| parse_links(&text));
    let networks = if wifi == Some(true) {
        output(
            "nmcli",
            &[
                "-t",
                "-f",
                "IN-USE,SIGNAL,SECURITY,SSID",
                "device",
                "wifi",
                "list",
            ],
        )
        .map(|text| parse_networks(&text))
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    State {
        links,
        networks,
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
/// Splits one `nmcli -t` line at unescaped colons (`\:` is a literal colon, `\\` a backslash).
fn split_terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                if let (Some(next), Some(field)) = (chars.next(), fields.last_mut()) {
                    field.push(next);
                }
            }
            ':' => fields.push(String::new()),
            _ => {
                if let Some(field) = fields.last_mut() {
                    field.push(ch);
                }
            }
        }
    }
    fields
}
/// Connected Ethernet and Wi-Fi devices from `nmcli -t -f TYPE,STATE,CONNECTION device`.
pub(crate) fn parse_links(text: &str) -> Vec<Link> {
    text.lines()
        .filter_map(|line| {
            let fields = split_terse(line);
            let [kind, state, name] = fields.as_slice() else {
                return None;
            };
            let kind = match kind.as_str() {
                "ethernet" => LinkKind::Ethernet,
                "wifi" => LinkKind::Wifi,
                _ => return None,
            };
            (state == "connected" && !name.is_empty() && name != "--").then(|| Link {
                kind,
                name: name.clone(),
            })
        })
        .collect()
}
/// Strongest-first networks from `nmcli -t -f IN-USE,SIGNAL,SECURITY,SSID device wifi list`.
pub(crate) fn parse_networks(text: &str) -> Vec<WifiNetwork> {
    let mut networks: Vec<WifiNetwork> = Vec::new();
    for line in text.lines() {
        let fields = split_terse(line);
        let [in_use, signal, security, ssid] = fields.as_slice() else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let network = WifiNetwork {
            ssid: ssid.clone(),
            signal: signal.parse::<u8>().unwrap_or(0).min(100),
            secured: !security.is_empty() && security != "--",
            active: in_use == "*",
        };
        match networks.iter_mut().find(|known| known.ssid == network.ssid) {
            Some(known) => {
                known.active |= network.active;
                known.signal = known.signal.max(network.signal);
            }
            None => networks.push(network),
        }
    }
    networks.sort_by_key(|network| (!network.active, std::cmp::Reverse(network.signal)));
    networks.truncate(MAX_WIFI_NETWORKS);
    networks
}
pub(crate) fn parse_volume(text: &str) -> Option<u8> {
    let value = text.split_whitespace().nth(1)?.parse::<f64>().ok()?;
    value
        .is_finite()
        .then(|| (value * 100.0).round().clamp(0.0, 100.0) as u8)
}
const QUICK_TIMEOUT: Duration = Duration::from_secs(3);
/// Joining a network includes DHCP and can legitimately take several seconds.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(25);
fn output(program: &str, args: &[&str]) -> Result<String, String> {
    output_within(program, args, QUICK_TIMEOUT)
}
fn output_within(program: &str, args: &[&str], limit: Duration) -> Result<String, String> {
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
        if start.elapsed() > limit {
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
    fn links_keep_connected_ethernet_and_wifi_only() {
        let text = "ethernet:connected:Wired connection 1\nwifi:disconnected:--\nloopback:connected (externally):lo\nwifi:connected:Caf\\: Free";
        let links = parse_links(text);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].kind, LinkKind::Ethernet);
        assert_eq!(links[1].name, "Caf: Free");
    }
    #[test]
    fn networks_merge_duplicates_and_put_the_active_one_first() {
        let text = " :40:WPA2:Home\n*:55:WPA2:Office\n :80:WPA2:Home\n :70::Open\n :90:WPA2:";
        let networks = parse_networks(text);
        assert_eq!(networks[0].ssid, "Office");
        assert!(networks[0].active);
        assert_eq!(networks[1].ssid, "Home");
        assert_eq!(networks[1].signal, 80);
        assert!(!networks[2].secured);
        assert_eq!(networks.len(), 3, "hidden networks are skipped");
    }
    #[test]
    fn option_like_network_names_never_run_a_program() {
        assert!(execute(Action::WifiConnect("--help".into())).is_err());
        assert!(execute(Action::WifiConnect(String::new())).is_err());
    }
    #[test]
    fn unknown_power_profile_never_runs_a_program() {
        assert!(execute(Action::Profile("bad; command".into())).is_err());
    }
}
