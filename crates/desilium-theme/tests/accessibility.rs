//! WCAG contrast guarantees for built-in and derived themes.

use desilium_theme::{Color, MIN_DISABLED_CONTRAST, MIN_TEXT_CONTRAST, Mode, Theme};

const HUE_STEPS: u16 = 12;
const SWEEP_SATURATIONS: [f32; 3] = [0.35, 0.65, 1.0];
const SWEEP_LIGHTNESS: [f32; 4] = [0.30, 0.45, 0.55, 0.70];

fn assert_contrast(theme: &Theme, label: &str, fg: Color, bg: Color, min: f32) {
    let ratio = fg.contrast_ratio(bg);
    assert!(
        ratio >= min,
        "{} / {label}: {ratio:.2}:1 < {min}:1 ({fg} on {bg})",
        theme.name
    );
}

fn assert_theme_accessible(theme: &Theme) {
    let p = &theme.palette;
    let surfaces = [
        ("background", p.background),
        ("surface", p.surface),
        ("surface_raised", p.surface_raised),
        ("overlay", p.overlay),
    ];
    for (name, surface) in surfaces {
        assert_contrast(
            theme,
            &format!("primary on {name}"),
            p.text_primary,
            surface,
            MIN_TEXT_CONTRAST,
        );
        assert_contrast(
            theme,
            &format!("secondary on {name}"),
            p.text_secondary,
            surface,
            MIN_TEXT_CONTRAST,
        );
        assert_contrast(
            theme,
            &format!("disabled on {name}"),
            p.text_disabled,
            surface,
            MIN_DISABLED_CONTRAST,
        );
    }
    assert_contrast(
        theme,
        "on-accent",
        p.text_on_accent,
        p.accent,
        MIN_TEXT_CONTRAST,
    );
    assert_contrast(
        theme,
        "on-accent hover",
        p.text_on_accent,
        p.accent_hover,
        MIN_DISABLED_CONTRAST,
    );
    assert_contrast(
        theme,
        "on-accent pressed",
        p.text_on_accent,
        p.accent_pressed,
        MIN_DISABLED_CONTRAST,
    );
    let w = &theme.window;
    assert_contrast(
        theme,
        "title active",
        w.title_text_active,
        w.title_bar_active,
        MIN_TEXT_CONTRAST,
    );
    assert_contrast(
        theme,
        "title inactive",
        w.title_text_inactive,
        w.title_bar_inactive,
        MIN_TEXT_CONTRAST,
    );
    let b = &w.buttons;
    assert_contrast(
        theme,
        "button glyph active",
        b.glyph_active,
        w.title_bar_active,
        MIN_DISABLED_CONTRAST,
    );
    assert_contrast(
        theme,
        "button glyph inactive",
        b.glyph_inactive,
        w.title_bar_inactive,
        MIN_DISABLED_CONTRAST,
    );
    for fill in [b.close_hover_fill, b.close_pressed_fill] {
        assert_contrast(
            theme,
            "close glyph on danger",
            b.glyph_on_danger,
            fill,
            MIN_TEXT_CONTRAST,
        );
    }
}

#[test]
fn dark_theme_meets_aa() {
    assert_theme_accessible(&Theme::dark());
}

#[test]
fn light_theme_meets_aa() {
    assert_theme_accessible(&Theme::light());
}

#[test]
fn semantic_colours_are_visible_on_surfaces() {
    for theme in [Theme::dark(), Theme::light()] {
        let p = &theme.palette;
        for (name, color) in [
            ("success", p.success),
            ("warning", p.warning),
            ("danger", p.danger),
        ] {
            assert_contrast(&theme, name, color, p.surface, MIN_DISABLED_CONTRAST);
            assert_contrast(&theme, name, color, p.background, MIN_DISABLED_CONTRAST);
        }
    }
}

#[test]
fn accent_sweep_of_twelve_hues_keeps_aa_in_both_modes() {
    for mode in [Mode::Dark, Mode::Light] {
        for step in 0..HUE_STEPS {
            let hue = f32::from(step) * 360.0 / f32::from(HUE_STEPS);
            let accent = Color::from_hsl(hue, 0.8, 0.55, 1.0);
            let theme = Theme::derive_from_accent(accent, mode);
            assert_eq!(theme.palette.accent, accent.quantized());
            assert_theme_accessible(&theme);
        }
    }
}

#[test]
fn derivation_survives_an_exhaustive_accent_grid() {
    for mode in [Mode::Dark, Mode::Light] {
        for step in 0..HUE_STEPS * 2 {
            for saturation in SWEEP_SATURATIONS {
                for lightness in SWEEP_LIGHTNESS {
                    let hue = f32::from(step) * 360.0 / f32::from(HUE_STEPS * 2);
                    let accent = Color::from_hsl(hue, saturation, lightness, 1.0);
                    assert_theme_accessible(&Theme::derive_from_accent(accent, mode));
                }
            }
        }
    }
}

#[test]
fn derived_theme_needs_no_repairs_and_round_trips() {
    let accent = Color::rgb(0x2E9CFF);
    for mode in [Mode::Dark, Mode::Light] {
        let mut theme = Theme::derive_from_accent(accent, mode);
        assert!(theme.validate().is_empty());
        let reloaded = Theme::from_toml_str(&theme.to_toml_string().unwrap()).unwrap();
        assert_eq!(reloaded, theme);
    }
}

#[test]
fn derived_surfaces_keep_their_elevation_order() {
    let dark = Theme::derive_from_accent(Color::rgb(0x7B61FF), Mode::Dark).palette;
    let lum = |c: Color| c.relative_luminance();
    assert!(lum(dark.background) < lum(dark.surface));
    assert!(lum(dark.surface) < lum(dark.surface_raised));
    assert!(lum(dark.surface_raised) < lum(dark.overlay));
}
