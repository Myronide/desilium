//! Generates the `:root { --token: value; }` block for the active theme.
//!
//! Token NAMES are identical to `docs/design/tokens.css` (a test compares the
//! two sets). Values come from the [`Theme`] where it has an equivalent and from
//! the design constants otherwise, so changing the accent or loading a user
//! theme needs no CSS edit. Derived surface colours are tuned to reproduce the
//! built-in dark and light themes within a few levels per channel.

use std::fmt::Write;

use desilium_theme::{Color, Mode, ShadowStyle, Theme};

use super::fixups;

/// Saturation kept when the accent is darkened into text on light surfaces.
const LIGHT_TEXT_SATURATION: f32 = 0.77;
/// Alpha below which a colour is written as `rgba()` instead of hex.
const OPAQUE: u8 = 255;

/// The `:root { ... }` block for `theme`.
pub fn tokens_css(theme: &Theme) -> String {
    format!(":root {{\n{}}}\n", tokens_declarations(theme))
}

/// Only the declarations (`--name: value;` lines), usable as an inline `style`.
pub fn tokens_declarations(theme: &Theme) -> String {
    let mut out = Decls::default();
    brand(&mut out, theme);
    metrics(&mut out, theme);
    typography(&mut out, theme);
    motion(&mut out, theme);
    status(&mut out, theme);
    app_gradients(&mut out);
    surfaces(&mut out, theme);
    glass(&mut out, theme);
    text_and_lines(&mut out, theme);
    interaction(&mut out, theme);
    elevation(&mut out, theme);
    window(&mut out, theme);
    out.0
}

#[derive(Default)]
struct Decls(String);

impl Decls {
    fn set(&mut self, name: &str, value: impl AsRef<str>) {
        // Writing to a String cannot fail.
        let _ = writeln!(self.0, "  --{name}: {};", value.as_ref());
    }

    fn color(&mut self, name: &str, color: Color) {
        self.set(name, css(color));
    }
}

fn byte(channel: f32) -> u8 {
    (channel.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// CSS text for a colour: `#RRGGBB` when opaque, `rgba()` otherwise.
fn css(color: Color) -> String {
    let [r, g, b, a] = [color.r, color.g, color.b, color.a].map(byte);
    if a == OPAQUE {
        return format!("#{r:02X}{g:02X}{b:02X}");
    }
    format!("rgba({r}, {g}, {b}, {:.2})", color.a)
}

/// Moves a colour's HSL lightness by `delta`, keeping hue and saturation.
fn relight(color: Color, delta: f32, saturation_scale: f32) -> Color {
    let (hue, saturation, lightness) = color.to_hsl();
    let saturation = (saturation * saturation_scale).clamp(0.0, 1.0);
    Color::from_hsl(
        hue,
        saturation,
        (lightness + delta).clamp(0.0, 1.0),
        color.a,
    )
}

fn px(value: f32) -> String {
    format!("{}px", value.round() as i32)
}

fn dark(theme: &Theme) -> bool {
    theme.mode == Mode::Dark
}

fn pick<T>(theme: &Theme, dark_value: T, light_value: T) -> T {
    if dark(theme) { dark_value } else { light_value }
}

fn brand(out: &mut Decls, theme: &Theme) {
    let palette = &theme.palette;
    out.color("accent", palette.accent);
    out.color("accent-hover", palette.accent_hover);
    out.color("accent-press", palette.accent_pressed);
    out.color("on-accent", palette.text_on_accent);
    out.color("on-accent-glyph", Color::WHITE);
}

fn metrics(out: &mut Decls, theme: &Theme) {
    let space = &theme.spacing;
    out.set("space-0", "0");
    out.set("space-1", px(space.xxs));
    out.set("space-2", px(space.xs));
    out.set("space-3", px(space.sm));
    out.set("space-4", px(space.md));
    out.set("space-5", px(space.unit * 5.0));
    out.set("space-6", px(space.lg));
    out.set("space-8", px(space.xl));
    out.set("space-10", px(space.unit * 10.0));
    out.set("space-12", px(space.xxl));
    out.set("space-16", px(space.xxxl));

    // The scale follows the two configurable radii; at the brand values (control 8, window 12)
    // it is 4, 6, 8, 10, 12, 16 and 20 px.
    let control = theme.window.control_radius;
    let window = theme.window.corner_radius;
    out.set("radius-xs", px(control * 0.5));
    out.set("radius-sm", px(control * 0.75));
    out.set("radius-control", px(control));
    out.set("radius-md", px(control * 1.25));
    out.set("radius-lg", px(window));
    out.set("radius-window", px(window));
    out.set("radius-xl", px(window * 4.0 / 3.0));
    out.set("radius-2xl", px(window * 5.0 / 3.0));
    out.set("radius-pill", "999px");

    out.set("titlebar-h", px(theme.window.title_bar_height));
    for (name, value) in [
        ("topbar-h", "32px"),
        ("control-h", "32px"),
        ("control-h-lg", "40px"),
        ("dock-h", "64px"),
        ("icon-sm", "16px"),
        ("icon-md", "20px"),
        ("icon-lg", "24px"),
        ("focus-ring-w", "2px"),
        ("focus-ring-gap", "2px"),
    ] {
        out.set(name, value);
    }
}

fn typography(out: &mut Decls, theme: &Theme) {
    let type_ = &theme.typography;
    out.set("font-sans", type_.ui_css_stack());
    out.set("font-mono", type_.mono_css_stack());
    let sizes = &type_.sizes;
    for (name, size, line) in [
        ("caption", px(sizes.caption), "16px"),
        ("small", px(sizes.small), "16px"),
        ("body", px(sizes.body), "20px"),
        ("body-lg", "14px".to_owned(), "20px"),
        ("title", "16px".to_owned(), "24px"),
        ("headline", "20px".to_owned(), "28px"),
        ("display", "28px".to_owned(), "36px"),
        ("hero", "44px".to_owned(), "52px"),
        ("clock", "176px".to_owned(), "176px"),
    ] {
        out.set(&format!("fs-{name}"), size);
        out.set(&format!("lh-{name}"), line);
    }
    let weights = &type_.weights;
    out.set("fw-regular", fixups::weight(weights.regular).to_string());
    out.set("fw-medium", fixups::weight(weights.medium).to_string());
    out.set("fw-semibold", fixups::weight(weights.semibold).to_string());
    out.set("fw-bold", fixups::weight(weights.bold).to_string());
}

fn motion(out: &mut Decls, theme: &Theme) {
    let motion = &theme.motion;
    out.set("dur-instant", "80ms");
    out.set("dur-fast", format!("{}ms", motion.fast_ms));
    out.set("dur-base", format!("{}ms", motion.normal_ms));
    out.set("dur-slow", format!("{}ms", motion.slow_ms));
    out.set("dur-emphasis", "400ms");
    out.set("ease-standard", "cubic-bezier(0.2, 0, 0, 1)");
    out.set("ease-enter", "cubic-bezier(0.05, 0.7, 0.1, 1)");
    out.set("ease-exit", "cubic-bezier(0.3, 0, 0.8, 0.15)");
    out.set("ease-spring", "cubic-bezier(0.34, 1.4, 0.64, 1)");
}

fn status(out: &mut Decls, theme: &Theme) {
    let palette = &theme.palette;
    out.color("success", palette.success);
    out.color("warning", palette.warning);
    out.color("danger", palette.danger);
    out.color(
        "info",
        pick(theme, Color::rgb(0x4DB2FF), Color::rgb(0x1C8CE0)),
    );
}

fn app_gradients(out: &mut Decls) {
    for (name, from, to) in [
        ("files", "#4DA3FF", "#2B6BFF"),
        ("terminal", "#3A4157", "#1B1F2E"),
        ("web", "#35D0C0", "#1E8FE0"),
        ("settings", "#8A93AD", "#565E78"),
        ("music", "#FF6B9A", "#E8348A"),
        ("photos", "#FFB347", "#FF6A4D"),
        ("code", "#9B84FF", "#5B3FE6"),
        ("calc", "#5AD689", "#1FA463"),
        ("mail", "#54B6FF", "#3A79F2"),
        ("notes", "#FFD866", "#F2A930"),
        ("calendar", "#FF7A7A", "#E0455B"),
        ("store", "#7B61FF", "#C45CFF"),
    ] {
        out.set(&format!("app-{name}"), format!("{from}, {to}"));
    }
}

fn surfaces(out: &mut Decls, theme: &Theme) {
    let palette = &theme.palette;
    let (bg, surface, raised) = (palette.background, palette.surface, palette.surface_raised);
    out.color("bg", bg);
    out.color("surface-0", pick(theme, bg.mix(surface, 0.5), bg));
    out.color("surface-1", surface);
    out.color("surface-2", pick(theme, surface.mix(raised, 0.6), raised));
    let ink = palette.text_primary;
    out.color("surface-3", pick(theme, raised, bg.mix(ink, 0.012)));
    out.color(
        "surface-4",
        pick(theme, palette.overlay, bg.mix(ink, 0.065)),
    );
    out.color(
        "surface-inset",
        pick(theme, bg.mix(surface, 0.2), bg.mix(ink, 0.012)),
    );
    let scrim = pick(theme, bg.darken(0.6), ink).with_alpha(pick(theme, 0.45, 0.16));
    out.color("scrim", scrim);
}

fn glass(out: &mut Decls, theme: &Theme) {
    let palette = &theme.palette;
    let effects = &theme.effects;
    let base = pick(
        theme,
        palette.background.mix(palette.surface, 0.8),
        palette.surface_raised.mix(palette.background, 0.2),
    );
    out.color(
        "glass-bg",
        base.with_alpha(1.0 - effects.panel_translucency * 0.7),
    );
    out.color(
        "glass-bg-strong",
        base.with_alpha(1.0 - effects.popup_translucency * 0.45),
    );
    let sheen = pick(theme, 0.07, 0.70);
    out.set(
        "glass-sheen",
        format!(
            "linear-gradient(180deg, rgba(255, 255, 255, {sheen:.2}), rgba(255, 255, 255, 0) 60%)"
        ),
    );
    let bar = pick(
        theme,
        palette.background.darken(0.35),
        palette.background.lighten(0.5),
    );
    out.color("bar-bg", bar.with_alpha(pick(theme, 0.62, 0.60)));
    let active = pick(
        theme,
        palette.surface_raised.mix(palette.text_primary, 0.12),
        palette.surface_raised,
    );
    out.color("control-active", active);
    out.color(
        "glass-border",
        Color::WHITE.with_alpha(pick(theme, 0.09, 0.70)),
    );
    out.color(
        "glass-highlight",
        Color::WHITE.with_alpha(pick(theme, 0.10, 0.90)),
    );
}

fn text_and_lines(out: &mut Decls, theme: &Theme) {
    let palette = &theme.palette;
    out.color("text", palette.text_primary);
    out.color("text-2", palette.text_secondary);
    out.color("text-3", palette.text_disabled);
    out.color(
        "text-disabled",
        palette
            .text_disabled
            .mix(palette.surface, pick(theme, 0.34, 0.4)),
    );
    let on_surface = pick(
        theme,
        relight(palette.accent, 0.135, 1.0),
        relight(palette.accent, -0.115, LIGHT_TEXT_SATURATION),
    );
    out.color("text-on-surface-accent", on_surface);
    out.color(
        "border",
        palette.text_primary.with_alpha(pick(theme, 0.08, 0.10)),
    );
    out.color("border-strong", palette.border);
    out.color("divider", palette.divider);
}

fn interaction(out: &mut Decls, theme: &Theme) {
    let palette = &theme.palette;
    let ink = palette.text_primary;
    out.color("hover", ink.with_alpha(pick(theme, 0.06, 0.05)));
    out.color("pressed", ink.with_alpha(pick(theme, 0.10, 0.09)));
    out.color(
        "selected",
        palette.accent.with_alpha(pick(theme, 0.20, 0.14)),
    );
    out.color("accent-soft", palette.accent_subtle);
    out.color(
        "accent-line",
        palette.accent.with_alpha(pick(theme, 0.55, 0.50)),
    );
    let ring = pick(
        theme,
        relight(palette.accent, 0.08, 1.0),
        relight(palette.accent, -0.115, LIGHT_TEXT_SATURATION),
    );
    out.color("focus-ring", ring);
    out.color(
        "success-soft",
        palette.success.with_alpha(pick(theme, 0.16, 0.13)),
    );
    out.color(
        "warning-soft",
        palette.warning.with_alpha(pick(theme, 0.16, 0.14)),
    );
    out.color(
        "danger-soft",
        palette.danger.with_alpha(pick(theme, 0.16, 0.12)),
    );
}

fn shadow_layer(style: &ShadowStyle) -> String {
    let color = style.color.with_alpha(fixups::shadow_alpha(style.color.a));
    format!(
        "{} {} {} {} {}",
        px(style.offset_x),
        px(style.offset_y),
        px(style.blur),
        px(style.spread),
        css(color)
    )
}

fn elevation(out: &mut Decls, theme: &Theme) {
    let shadow = &theme.shadow;
    let ring = pick(
        theme,
        "0 0 0 1px rgba(255, 255, 255, 0.03)",
        "0 0 0 1px rgba(30, 36, 80, 0.03)",
    );
    let contact = "0 2px 6px rgba(10, 12, 31, 0.25)";
    out.set("shadow-0", "none");
    out.set(
        "shadow-1",
        format!("{}, {ring}", shadow_layer(&shadow.unfocused)),
    );
    let cards = pick(
        theme,
        "0 2px 4px rgba(0, 0, 0, 0.30), 0 8px 20px rgba(0, 0, 0, 0.32)",
        "0 2px 4px rgba(30, 36, 80, 0.06), 0 8px 20px rgba(30, 36, 80, 0.10)",
    );
    out.set("shadow-2", cards);
    out.set(
        "shadow-3",
        format!("{}, {contact}", shadow_layer(&shadow.focused)),
    );
    let overlay = format!(
        "{}, {}, {contact}",
        shadow_layer(&shadow.popup),
        shadow_layer(&shadow.modal)
    );
    out.set("shadow-4", overlay);
    out.set(
        "shadow-inner",
        pick(
            theme,
            "inset 0 1px 0 rgba(255, 255, 255, 0.06)",
            "inset 0 1px 0 rgba(255, 255, 255, 0.90)",
        ),
    );
}

fn window(out: &mut Decls, theme: &Theme) {
    let window = &theme.window;
    out.set("window-bg", "var(--surface-1)");
    out.color("window-border", window.border_active);
    out.color("window-border-inactive", window.border_inactive);
    out.color("titlebar-bg", window.title_bar_active);
    out.color("titlebar-bg-inactive", window.title_bar_inactive);
    out.set("wallpaper", wallpaper(theme));
}

/// Wallpaper stand-in for shell mockups: the design's soft colour blobs (the first
/// follows the accent) over the theme's own gradient. The compositor draws the real wallpaper.
fn wallpaper(theme: &Theme) -> String {
    let gradient = &theme.background.gradient;
    let stops: Vec<String> = gradient
        .stops
        .iter()
        .map(|stop| format!("{} {:.0}%", css(stop.color), stop.position * 100.0))
        .collect();
    let linear = format!(
        "linear-gradient({:.0}deg, {})",
        gradient.angle_deg,
        stops.join(", ")
    );
    let (accent_alpha, blobs) = pick(
        theme,
        (
            0.55,
            [
                "rgba(40, 120, 220, 0.38)",
                "rgba(214, 70, 160, 0.32)",
                "rgba(30, 160, 190, 0.24)",
            ],
        ),
        (
            0.70,
            [
                "rgba(110, 190, 255, 0.60)",
                "rgba(255, 150, 200, 0.55)",
                "rgba(120, 220, 220, 0.45)",
            ],
        ),
    );
    let accent = css(theme.palette.accent.with_alpha(accent_alpha));
    format!(
        "radial-gradient(1100px 700px at 12% 8%, {accent}, transparent 62%), \
         radial-gradient(900px 700px at 92% 18%, {}, transparent 60%), \
         radial-gradient(1000px 800px at 78% 108%, {}, transparent 62%), \
         radial-gradient(900px 600px at 0% 100%, {}, transparent 60%), {linear}",
        blobs[0], blobs[1], blobs[2]
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const DESIGN_TOKENS: &str = include_str!("../../../../docs/design/tokens.css");

    fn names_in(css_text: &str) -> BTreeSet<String> {
        css_text
            .lines()
            .flat_map(|line| line.split(';'))
            .filter_map(|decl| decl.trim().strip_prefix("--"))
            .filter_map(|rest| rest.split_once(':'))
            .map(|(name, _)| name.trim().to_owned())
            .collect()
    }

    fn value_of(css_text: &str, name: &str) -> Option<String> {
        let prefix = format!("--{name}:");
        let line = css_text.lines().find(|l| l.trim().starts_with(&prefix))?;
        let value = line.trim().strip_prefix(&prefix)?;
        Some(
            value
                .split("/*")
                .next()?
                .trim()
                .trim_end_matches(';')
                .trim()
                .to_owned(),
        )
    }

    #[test]
    fn token_names_match_the_design_file_in_both_modes() {
        let expected = names_in(DESIGN_TOKENS);
        assert!(expected.len() > 100);
        for theme in [Theme::dark(), Theme::light()] {
            let generated = names_in(&tokens_css(&theme));
            let missing: Vec<_> = expected.difference(&generated).collect();
            let extra: Vec<_> = generated.difference(&expected).collect();
            assert!(missing.is_empty(), "missing tokens: {missing:?}");
            assert!(extra.is_empty(), "unknown tokens: {extra:?}");
        }
    }

    fn channels(text: &str) -> Option<[i32; 3]> {
        let hex = text.strip_prefix('#')?;
        let parse = |i: usize| i32::from_str_radix(hex.get(i..i + 2)?, 16).ok();
        Some([parse(0)?, parse(2)?, parse(4)?])
    }

    #[test]
    fn built_in_themes_reproduce_the_opaque_design_colours() {
        const TOLERANCE: i32 = 7;
        let sections = DESIGN_TOKENS
            .split(":root[data-theme=\"light\"]")
            .collect::<Vec<_>>();
        let (dark_part, light_part) = (sections[0], sections[1]);
        for (theme, part) in [(Theme::dark(), dark_part), (Theme::light(), light_part)] {
            let generated = tokens_css(&theme);
            for name in [
                "bg",
                "surface-0",
                "surface-1",
                "surface-2",
                "surface-3",
                "surface-4",
                "surface-inset",
                "text",
                "text-2",
                "text-3",
                "text-disabled",
                "text-on-surface-accent",
                "border-strong",
                "divider",
                "focus-ring",
                "accent",
            ] {
                let want = value_of(part, name).or_else(|| value_of(DESIGN_TOKENS, name));
                let want = channels(&want.expect("design token"));
                let got = channels(&value_of(&generated, name).expect("generated token"));
                let (Some(want), Some(got)) = (want, got) else {
                    panic!("{name} not hex")
                };
                let worst = (0..3).map(|i| (want[i] - got[i]).abs()).max().unwrap_or(0);
                assert!(
                    worst <= TOLERANCE,
                    "{:?} {name}: want {want:?} got {got:?}",
                    theme.mode
                );
            }
        }
    }

    #[test]
    fn accent_changes_flow_into_the_tokens() {
        let mut theme = Theme::dark();
        theme.palette.accent = Color::rgb(0xFF8800);
        let generated = tokens_css(&theme);
        assert_eq!(value_of(&generated, "accent").as_deref(), Some("#FF8800"));
    }

    #[test]
    fn colours_with_alpha_use_rgba() {
        assert_eq!(css(Color::rgba(0x7B61FF29)), "rgba(123, 97, 255, 0.16)");
        assert_eq!(css(Color::rgb(0x0E1016)), "#0E1016");
    }
}
