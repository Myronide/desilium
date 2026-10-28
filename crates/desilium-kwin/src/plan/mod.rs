//! The typed Desilium configuration as KWin and KDE settings.
//!
//! Everything here is pure: the same configuration always yields the same [`Plan`], so the
//! mapping is unit-tested without KWin. Applying the plan is the job of [`crate::apply`].
//!
//! Key names follow KWin 6 and KConfig. They could not be checked against a running KWin when
//! this was written; `docs/kwin-integration.md` lists them as the first thing to verify.

mod appearance;
mod colors;
mod entry;
mod pointer;

use desilium_config_types::{
    AppearanceSettings, CompositorSettings, DEFAULT_THEME, DesiliumConfig, FocusPolicy, KDE_THEME,
    MAX_DESKTOPS, MIN_DESKTOPS,
};
use desilium_theme::{Mode, Theme};

pub use appearance::{ResolvedAppearance, resolve_appearance};
pub use colors::scheme_entries;
pub use entry::{ConfigEntry, EntryId, Value};
pub use pointer::{
    PROP_ACCELERATION, PROP_FLAT_PROFILE, PROP_LEFT_HANDED, PROP_NATURAL_SCROLL, PROP_POINTER,
    PROP_SCROLL_FACTOR, PROP_TOUCHPAD, PointerPlan,
};

const KWINRC: &str = "kwinrc";
const KDEGLOBALS: &str = "kdeglobals";
const KCMINPUTRC: &str = "kcminputrc";
const KXKBRC: &str = "kxkbrc";

/// Slowest and fastest animation speed the settings accept; outside it the factor would be absurd.
const MIN_SPEED: f32 = 0.25;
const MAX_SPEED: f32 = 4.0;
/// `AnimationDurationFactor` ceiling when the user asked for reduced motion.
const REDUCED_MOTION_FACTOR: f64 = 0.35;

/// What the bridge wants KWin to look like for one configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// Configuration keys to write, in write order.
    pub entries: Vec<ConfigEntry>,
    /// Number of virtual desktops, set over D-Bus.
    pub desktops: u8,
    /// What every mouse is set to, over D-Bus.
    pub pointer: PointerPlan,
}

/// Projects `config` onto KWin. `theme_override` is `--theme` of the bridge, as for the shell.
pub fn plan(config: &DesiliumConfig, theme_override: Option<Mode>) -> (Plan, Vec<String>) {
    // Following KDE means KDE's colours stay as the user set them: nothing is pushed into
    // `kdeglobals`, the shell reads them from there instead.
    let follows_kde = config.appearance.theme == KDE_THEME && theme_override.is_none();
    let resolvable = AppearanceSettings {
        theme: if follows_kde {
            DEFAULT_THEME.to_owned()
        } else {
            config.appearance.theme.clone()
        },
        ..config.appearance.clone()
    };
    let resolved = resolve_appearance(&resolvable, theme_override);
    let mut entries = window_entries(&config.compositor);
    entries.extend(input_entries(&config.compositor));
    entries.extend(appearance_entries(
        &resolved.appearance,
        &resolved.theme,
        follows_kde,
    ));
    let desktops = config.compositor.desktops.clamp(MIN_DESKTOPS, MAX_DESKTOPS);
    let pointer = PointerPlan::of(&config.compositor.mouse);
    (
        Plan {
            entries,
            desktops,
            pointer,
        },
        resolved.warnings,
    )
}

/// Focus, raising and snapping.
fn window_entries(settings: &CompositorSettings) -> Vec<ConfigEntry> {
    let sloppy = settings.focus == FocusPolicy::Sloppy;
    let policy = if sloppy {
        "FocusFollowsMouse"
    } else {
        "ClickToFocus"
    };
    let zone = if settings.snap_enabled {
        settings.snap_distance
    } else {
        0
    };
    vec![
        ConfigEntry::text(KWINRC, "Windows", "FocusPolicy", policy),
        ConfigEntry::flag(KWINRC, "Windows", "ClickRaise", true),
        ConfigEntry::flag(
            KWINRC,
            "Windows",
            "AutoRaise",
            sloppy && settings.raise_on_focus,
        ),
        ConfigEntry::text(KWINRC, "Windows", "BorderSnapZone", zone.to_string()),
        ConfigEntry::text(KWINRC, "Windows", "WindowSnapZone", zone.to_string()),
        ConfigEntry::text(KWINRC, "Windows", "CenterSnapZone", "0"),
        ConfigEntry::flag(
            KWINRC,
            "Windows",
            "ElectricBorderMaximize",
            settings.snap_enabled,
        ),
        ConfigEntry::flag(
            KWINRC,
            "Windows",
            "ElectricBorderTiling",
            settings.snap_enabled,
        ),
    ]
}

/// Keyboard repeat and layout, and the double-click interval of the mouse.
fn input_entries(settings: &CompositorSettings) -> Vec<ConfigEntry> {
    vec![
        ConfigEntry::text(
            KDEGLOBALS,
            "KDE",
            "DoubleClickInterval",
            settings.mouse.double_click_ms.to_string(),
        ),
        ConfigEntry::text(
            KCMINPUTRC,
            "Keyboard",
            "RepeatDelay",
            settings.keyboard_repeat_delay.to_string(),
        ),
        ConfigEntry::text(
            KCMINPUTRC,
            "Keyboard",
            "RepeatRate",
            settings.keyboard_repeat_rate.to_string(),
        ),
        ConfigEntry::flag(KXKBRC, "Layout", "Use", true),
        ConfigEntry::text(
            KXKBRC,
            "Layout",
            "LayoutList",
            settings.keyboard_layout.xkb_name(),
        ),
        ConfigEntry::text(KXKBRC, "Layout", "VariantList", ""),
    ]
}

/// Animations, cursor, icons, effects and (unless KDE's own are kept) colours.
fn appearance_entries(
    appearance: &AppearanceSettings,
    theme: &Theme,
    follows_kde: bool,
) -> Vec<ConfigEntry> {
    let mut entries = vec![
        ConfigEntry::text(
            KDEGLOBALS,
            "KDE",
            "AnimationDurationFactor",
            format_factor(animation_factor(appearance)),
        ),
        ConfigEntry::text(
            KCMINPUTRC,
            "Mouse",
            "cursorSize",
            appearance.cursor_size.to_string(),
        ),
        ConfigEntry::flag(KWINRC, "Plugins", "blurEnabled", true),
    ];
    if let Some(name) = appearance.cursor_theme.as_deref().filter(|n| !n.is_empty()) {
        entries.push(ConfigEntry::text(KCMINPUTRC, "Mouse", "cursorTheme", name));
    }
    if let Some(name) = appearance.icon_theme.as_deref().filter(|n| !n.is_empty()) {
        // KDE and Qt applications use the same icons as the shell.
        entries.push(ConfigEntry::text(KDEGLOBALS, "Icons", "Theme", name));
    }
    if !follows_kde {
        entries.extend(scheme_entries(theme));
    }
    entries
}

/// KDE's duration factor: `1.0` normal, smaller is faster, `0` switches animations off.
fn animation_factor(appearance: &AppearanceSettings) -> f64 {
    if !appearance.animations {
        return 0.0;
    }
    let speed = appearance.animation_speed.clamp(MIN_SPEED, MAX_SPEED);
    let factor = f64::from(1.0 / speed);
    if appearance.reduce_motion {
        factor.min(REDUCED_MOTION_FACTOR)
    } else {
        factor
    }
}

fn format_factor(factor: f64) -> String {
    let text = format!("{factor:.2}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_owned()
    } else {
        trimmed.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(plan: &Plan, file: &str, group: &str, key: &str) -> Option<String> {
        plan.entries
            .iter()
            .find(|e| e.id.file == file && e.id.group == group && e.id.key == key)
            .map(|e| e.value.render().to_owned())
    }

    fn plan_for(config: &DesiliumConfig) -> Plan {
        plan(config, None).0
    }

    #[test]
    fn defaults_map_to_click_focus_and_normal_speed() {
        let result = plan_for(&DesiliumConfig::default());
        assert_eq!(
            value(&result, "kwinrc", "Windows", "FocusPolicy").as_deref(),
            Some("ClickToFocus")
        );
        assert_eq!(
            value(&result, "kwinrc", "Windows", "AutoRaise").as_deref(),
            Some("false")
        );
        assert_eq!(
            value(&result, "kdeglobals", "KDE", "AnimationDurationFactor").as_deref(),
            Some("1")
        );
        assert_eq!(result.desktops, 4);
    }

    #[test]
    fn mouse_settings_reach_the_pointer_plan_and_the_double_click_key() {
        let mut config = DesiliumConfig::default();
        let defaults = plan_for(&config);
        assert_eq!(defaults.pointer, PointerPlan::default());
        assert_eq!(
            value(&defaults, "kdeglobals", "KDE", "DoubleClickInterval").as_deref(),
            Some("400")
        );
        config.compositor.mouse.pointer_speed = -40;
        config.compositor.mouse.double_click_ms = 650;
        config.compositor.mouse.natural_scroll = true;
        let changed = plan_for(&config);
        assert_eq!(changed.pointer.acceleration(), -0.4);
        assert!(changed.pointer.natural_scroll);
        assert_eq!(
            value(&changed, "kdeglobals", "KDE", "DoubleClickInterval").as_deref(),
            Some("650")
        );
    }

    #[test]
    fn sloppy_focus_with_raise_turns_auto_raise_on() {
        let mut config = DesiliumConfig::default();
        config.compositor.focus = FocusPolicy::Sloppy;
        let result = plan_for(&config);
        assert_eq!(
            value(&result, "kwinrc", "Windows", "FocusPolicy").as_deref(),
            Some("FocusFollowsMouse")
        );
        assert_eq!(
            value(&result, "kwinrc", "Windows", "AutoRaise").as_deref(),
            Some("true")
        );
        config.compositor.raise_on_focus = false;
        let result = plan_for(&config);
        assert_eq!(
            value(&result, "kwinrc", "Windows", "AutoRaise").as_deref(),
            Some("false")
        );
    }

    #[test]
    fn snapping_distance_and_switch() {
        let mut config = DesiliumConfig::default();
        config.compositor.snap_distance = 16;
        let on = plan_for(&config);
        assert_eq!(
            value(&on, "kwinrc", "Windows", "BorderSnapZone").as_deref(),
            Some("16")
        );
        assert_eq!(
            value(&on, "kwinrc", "Windows", "ElectricBorderTiling").as_deref(),
            Some("true")
        );
        config.compositor.snap_enabled = false;
        let off = plan_for(&config);
        assert_eq!(
            value(&off, "kwinrc", "Windows", "WindowSnapZone").as_deref(),
            Some("0")
        );
        assert_eq!(
            value(&off, "kwinrc", "Windows", "ElectricBorderTiling").as_deref(),
            Some("false")
        );
    }

    #[test]
    fn keyboard_settings_go_to_the_input_files() {
        let mut config = DesiliumConfig::default();
        config.compositor.keyboard_repeat_delay = 300;
        config.compositor.keyboard_repeat_rate = 40;
        let result = plan_for(&config);
        assert_eq!(
            value(&result, "kcminputrc", "Keyboard", "RepeatDelay").as_deref(),
            Some("300")
        );
        assert_eq!(
            value(&result, "kcminputrc", "Keyboard", "RepeatRate").as_deref(),
            Some("40")
        );
        assert_eq!(
            value(&result, "kxkbrc", "Layout", "LayoutList").as_deref(),
            Some("de")
        );
    }

    #[test]
    fn desktop_count_is_clamped_to_the_supported_range() {
        let mut config = DesiliumConfig::default();
        config.compositor.desktops = 0;
        assert_eq!(plan_for(&config).desktops, MIN_DESKTOPS);
        config.compositor.desktops = 200;
        assert_eq!(plan_for(&config).desktops, MAX_DESKTOPS);
    }

    #[test]
    fn animation_factor_follows_speed_switch_and_reduced_motion() {
        let mut appearance = AppearanceSettings::default();
        assert_eq!(format_factor(animation_factor(&appearance)), "1");
        appearance.animation_speed = 2.0;
        assert_eq!(format_factor(animation_factor(&appearance)), "0.5");
        appearance.animation_speed = 0.5;
        assert_eq!(format_factor(animation_factor(&appearance)), "2");
        appearance.animation_speed = 100.0;
        assert_eq!(format_factor(animation_factor(&appearance)), "0.25");
        appearance.animation_speed = 1.0;
        appearance.reduce_motion = true;
        assert_eq!(format_factor(animation_factor(&appearance)), "0.35");
        appearance.animations = false;
        assert_eq!(format_factor(animation_factor(&appearance)), "0");
    }

    #[test]
    fn cursor_theme_is_only_written_when_chosen() {
        let mut config = DesiliumConfig::default();
        let none = plan_for(&config);
        assert_eq!(value(&none, "kcminputrc", "Mouse", "cursorTheme"), None);
        assert_eq!(
            value(&none, "kcminputrc", "Mouse", "cursorSize").as_deref(),
            Some("24")
        );
        config.appearance.cursor_theme = Some("Adwaita".to_owned());
        let named = plan_for(&config);
        assert_eq!(
            value(&named, "kcminputrc", "Mouse", "cursorTheme").as_deref(),
            Some("Adwaita")
        );
    }

    #[test]
    fn accent_reaches_the_colour_scheme() {
        let mut config = DesiliumConfig::default();
        config.appearance.accent = Some("#E94E77".to_owned());
        let result = plan_for(&config);
        assert_eq!(
            value(&result, "kdeglobals", "General", "AccentColor").as_deref(),
            Some("233,78,119")
        );
    }

    #[test]
    fn following_kde_writes_no_colours_and_no_warning() {
        let mut config = DesiliumConfig::default();
        config.appearance.theme = KDE_THEME.to_owned();
        let (result, warnings) = plan(&config, None);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert!(
            result
                .entries
                .iter()
                .all(|e| !e.id.group.starts_with("Colors:")
                    && e.id.group != "WM"
                    && e.id.key != "ColorScheme"
                    && e.id.key != "AccentColor"),
            "KDE's own colours are left alone"
        );
        assert!(
            value(&result, "kdeglobals", "KDE", "AnimationDurationFactor").is_some(),
            "everything else still applies"
        );
    }

    #[test]
    fn an_explicit_theme_override_still_pushes_colours_while_kde_is_configured() {
        let mut config = DesiliumConfig::default();
        config.appearance.theme = KDE_THEME.to_owned();
        let (result, _) = plan(&config, Some(Mode::Light));
        assert_eq!(
            value(&result, "kdeglobals", "General", "ColorScheme").as_deref(),
            Some("DesiliumLight")
        );
    }

    #[test]
    fn a_chosen_icon_theme_is_handed_to_kde() {
        let mut config = DesiliumConfig::default();
        assert_eq!(
            value(&plan_for(&config), "kdeglobals", "Icons", "Theme"),
            None
        );
        config.appearance.icon_theme = Some("Papirus".to_owned());
        assert_eq!(
            value(&plan_for(&config), "kdeglobals", "Icons", "Theme").as_deref(),
            Some("Papirus")
        );
    }

    #[test]
    fn theme_warnings_are_returned_not_lost() {
        let mut config = DesiliumConfig::default();
        config.appearance.theme = "neon-pink".to_owned();
        let (result, warnings) = plan(&config, None);
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            value(&result, "kdeglobals", "General", "ColorScheme").as_deref(),
            Some("DesiliumDark")
        );
    }

    #[test]
    fn no_two_entries_share_an_identity() {
        let result = plan_for(&DesiliumConfig::default());
        let mut ids: Vec<_> = result.entries.iter().map(|e| e.id.clone()).collect();
        let total = ids.len();
        ids.sort_by(|a, b| (a.file, &a.group, &a.key).cmp(&(b.file, &b.group, &b.key)));
        ids.dedup();
        assert_eq!(ids.len(), total);
    }
}
