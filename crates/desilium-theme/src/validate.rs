//! Theme validation: repairs every out-of-range value and reports what changed.

use crate::check::{Bounds, Checker, MIN_DISABLED_CONTRAST, MIN_TEXT_CONTRAST};
use crate::palette::best_on_accent;
use crate::{
    Background, Color, Effects, GRID_UNIT, Gradient, MAX_NOISE, Motion, Palette, Shadow,
    ShadowStyle, Spacing, Theme, ThemeIssue, Typography, Window,
};

const RADIUS: Bounds = Bounds::new(0.0, 64.0, 12.0);
const CONTROL_RADIUS_RANGE: Bounds = Bounds::new(0.0, 32.0, 8.0);
const BORDER_WIDTH: Bounds = Bounds::new(0.0, 8.0, 1.0);
const TITLE_BAR: Bounds = Bounds::new(24.0, 64.0, 36.0);
const TITLE_FONT: Bounds = Bounds::new(8.0, 32.0, 13.0);
const BUTTON_WIDTH: Bounds = Bounds::new(16.0, 48.0, 32.0);
const BUTTON_HEIGHT: Bounds = Bounds::new(16.0, 32.0, 28.0);
const BUTTON_RADIUS: Bounds = Bounds::new(0.0, 16.0, 8.0);
const BUTTON_SPACING: Bounds = Bounds::new(0.0, 16.0, 2.0);
const BUTTON_SIDE_INSET: Bounds = Bounds::new(0.0, 32.0, 4.0);
const GLYPH_SIZE: Bounds = Bounds::new(8.0, 24.0, 16.0);
const GLYPH_STROKE: Bounds = Bounds::new(0.5, 3.0, 1.5);
const SHADOW_BLUR: Bounds = Bounds::new(0.0, 200.0, 24.0);
const SHADOW_SPREAD: Bounds = Bounds::new(-64.0, 64.0, 0.0);
const SHADOW_OFFSET: Bounds = Bounds::new(-128.0, 128.0, 0.0);
const SPACING_STEP: Bounds = Bounds::new(0.0, 256.0, 8.0);
const SPACING_UNIT: Bounds = Bounds::new(1.0, 16.0, GRID_UNIT);
const FONT_SIZE: Bounds = Bounds::new(8.0, 96.0, 13.0);
const LINE_HEIGHT: Bounds = Bounds::new(1.0, 3.0, 1.4);
const SPRING_STIFFNESS: Bounds = Bounds::new(1.0, 2000.0, 320.0);
const SPRING_DAMPING: Bounds = Bounds::new(0.1, 200.0, 28.0);
const SPRING_MASS: Bounds = Bounds::new(0.1, 20.0, 1.0);
const NOISE: Bounds = Bounds::new(0.0, MAX_NOISE, 0.0);
const BLUR_RADIUS: Bounds = Bounds::new(0.0, 128.0, 24.0);
const SATURATION: Bounds = Bounds::new(0.0, 3.0, 1.0);
const UNIT_INTERVAL: Bounds = Bounds::new(0.0, 1.0, 0.0);
const ANGLE_FALLBACK: f32 = 160.0;
const FULL_TURN: f32 = 360.0;
const MAX_DURATION_MS: u32 = 5000;
const WEIGHT_RANGE: (u32, u32) = (100, 900);
const MIN_GRADIENT_STOPS: usize = 2;
const MAX_GRADIENT_STOPS: usize = 8;
/// Space kept above and below a window button inside the title bar.
const BUTTON_MARGIN: f32 = 2.0 * GRID_UNIT;

impl Theme {
    /// Repairs invalid values in place and returns what was changed.
    ///
    /// Afterwards the theme is safe to render: finite, in range, and readable.
    pub fn validate(&mut self) -> Vec<ThemeIssue> {
        let mut checker = Checker::default();
        if self.name.trim().is_empty() {
            checker.report("name", "empty name replaced".to_owned());
            self.name = "Custom theme".to_owned();
        }
        check_palette(&mut checker, &mut self.palette);
        check_window(&mut checker, &mut self.window);
        check_shadow(&mut checker, &mut self.shadow);
        check_spacing(&mut checker, &mut self.spacing);
        check_typography(&mut checker, &mut self.typography);
        check_motion(&mut checker, &mut self.motion);
        check_background(
            &mut checker,
            &mut self.background,
            self.mode,
            self.palette.accent,
        );
        check_effects(&mut checker, &mut self.effects);
        check_chrome_contrast(&mut checker, &mut self.window);
        checker.issues
    }
}

/// Fixes text contrast of a palette; used by accent derivation too.
pub(crate) fn repair_palette_contrast(palette: &mut Palette, issues: &mut Vec<ThemeIssue>) {
    let mut checker = Checker::default();
    check_palette_contrast(&mut checker, palette);
    issues.append(&mut checker.issues);
}

fn check_palette(checker: &mut Checker, palette: &mut Palette) {
    let opaque_colors = [
        ("background", &mut palette.background),
        ("surface", &mut palette.surface),
        ("surface_raised", &mut palette.surface_raised),
        ("overlay", &mut palette.overlay),
        ("text_primary", &mut palette.text_primary),
        ("text_secondary", &mut palette.text_secondary),
        ("text_disabled", &mut palette.text_disabled),
        ("text_on_accent", &mut palette.text_on_accent),
        ("accent", &mut palette.accent),
        ("accent_hover", &mut palette.accent_hover),
        ("accent_pressed", &mut palette.accent_pressed),
        ("success", &mut palette.success),
        ("warning", &mut palette.warning),
        ("danger", &mut palette.danger),
        ("border", &mut palette.border),
        ("divider", &mut palette.divider),
    ];
    for (name, color) in opaque_colors {
        checker.color(&format!("palette.{name}"), color, true);
    }
    checker.color("palette.accent_subtle", &mut palette.accent_subtle, false);
    checker.color("palette.selection", &mut palette.selection, false);
    check_palette_contrast(checker, palette);
}

fn check_palette_contrast(checker: &mut Checker, palette: &mut Palette) {
    let surfaces = [
        palette.background,
        palette.surface,
        palette.surface_raised,
        palette.overlay,
    ];
    checker.contrast(
        "palette.text_primary",
        &mut palette.text_primary,
        &surfaces,
        MIN_TEXT_CONTRAST,
    );
    checker.contrast(
        "palette.text_secondary",
        &mut palette.text_secondary,
        &surfaces,
        MIN_TEXT_CONTRAST,
    );
    checker.contrast(
        "palette.text_disabled",
        &mut palette.text_disabled,
        &surfaces,
        MIN_DISABLED_CONTRAST,
    );
    let accent = palette.accent;
    if palette.text_on_accent.contrast_ratio(accent) < MIN_TEXT_CONTRAST {
        let better = best_on_accent(accent);
        let message = format!(
            "unreadable on {accent}; {} became {better}",
            palette.text_on_accent
        );
        checker.report("palette.text_on_accent", message);
        palette.text_on_accent = better;
    }
}

fn check_window(checker: &mut Checker, window: &mut Window) {
    checker.float("window.corner_radius", &mut window.corner_radius, RADIUS);
    checker.float(
        "window.control_radius",
        &mut window.control_radius,
        CONTROL_RADIUS_RANGE,
    );
    checker.float(
        "window.border_width",
        &mut window.border_width,
        BORDER_WIDTH,
    );
    checker.float(
        "window.title_bar_height",
        &mut window.title_bar_height,
        TITLE_BAR,
    );
    checker.float(
        "window.title_font_size",
        &mut window.title_font_size,
        TITLE_FONT,
    );
    weight(
        checker,
        "window.title_font_weight",
        &mut window.title_font_weight,
    );
    check_window_colors(checker, window);
    check_window_buttons(checker, window);
}

fn check_window_buttons(checker: &mut Checker, window: &mut Window) {
    let largest_height = window.title_bar_height - BUTTON_MARGIN;
    let buttons = &mut window.buttons;
    checker.float("window.buttons.width", &mut buttons.width, BUTTON_WIDTH);
    checker.float("window.buttons.height", &mut buttons.height, BUTTON_HEIGHT);
    checker.float(
        "window.buttons.corner_radius",
        &mut buttons.corner_radius,
        BUTTON_RADIUS,
    );
    checker.float(
        "window.buttons.spacing",
        &mut buttons.spacing,
        BUTTON_SPACING,
    );
    checker.float(
        "window.buttons.side_inset",
        &mut buttons.side_inset,
        BUTTON_SIDE_INSET,
    );
    checker.float(
        "window.buttons.glyph_size",
        &mut buttons.glyph_size,
        GLYPH_SIZE,
    );
    checker.float(
        "window.buttons.glyph_stroke",
        &mut buttons.glyph_stroke,
        GLYPH_STROKE,
    );
    fit_within(
        checker,
        "window.buttons.height",
        &mut buttons.height,
        largest_height,
    );
    let glyph_limit = buttons.width.min(buttons.height);
    fit_within(
        checker,
        "window.buttons.glyph_size",
        &mut buttons.glyph_size,
        glyph_limit,
    );
}

/// Shrinks `value` to `limit` when it overflows its container.
fn fit_within(checker: &mut Checker, field: &str, value: &mut f32, limit: f32) {
    if *value <= limit {
        return;
    }
    checker.report(field, format!("{value} does not fit; using {limit}"));
    *value = limit;
}

fn check_window_colors(checker: &mut Checker, window: &mut Window) {
    let opaque_colors = [
        ("border_active", &mut window.border_active),
        ("border_inactive", &mut window.border_inactive),
        ("title_bar_active", &mut window.title_bar_active),
        ("title_bar_inactive", &mut window.title_bar_inactive),
        ("title_text_active", &mut window.title_text_active),
        ("title_text_inactive", &mut window.title_text_inactive),
    ];
    for (name, color) in opaque_colors {
        checker.color(&format!("window.{name}"), color, true);
    }
    let buttons = &mut window.buttons;
    let fills = [
        ("rest_fill", &mut buttons.rest_fill),
        ("hover_fill", &mut buttons.hover_fill),
        ("pressed_fill", &mut buttons.pressed_fill),
        ("close_hover_fill", &mut buttons.close_hover_fill),
        ("close_pressed_fill", &mut buttons.close_pressed_fill),
    ];
    for (name, color) in fills {
        checker.color(&format!("window.buttons.{name}"), color, false);
    }
    let glyphs = [
        ("glyph_active", &mut buttons.glyph_active),
        ("glyph_inactive", &mut buttons.glyph_inactive),
        ("glyph_on_danger", &mut buttons.glyph_on_danger),
    ];
    for (name, color) in glyphs {
        checker.color(&format!("window.buttons.{name}"), color, true);
    }
}

fn check_chrome_contrast(checker: &mut Checker, window: &mut Window) {
    let (active, inactive) = (window.title_bar_active, window.title_bar_inactive);
    checker.contrast(
        "window.title_text_active",
        &mut window.title_text_active,
        &[active],
        MIN_TEXT_CONTRAST,
    );
    checker.contrast(
        "window.title_text_inactive",
        &mut window.title_text_inactive,
        &[inactive],
        MIN_TEXT_CONTRAST,
    );
    let buttons = &mut window.buttons;
    checker.contrast(
        "window.buttons.glyph_active",
        &mut buttons.glyph_active,
        &[active],
        MIN_DISABLED_CONTRAST,
    );
    checker.contrast(
        "window.buttons.glyph_inactive",
        &mut buttons.glyph_inactive,
        &[inactive],
        MIN_DISABLED_CONTRAST,
    );
    let danger = [buttons.close_hover_fill, buttons.close_pressed_fill];
    checker.contrast(
        "window.buttons.glyph_on_danger",
        &mut buttons.glyph_on_danger,
        &danger,
        MIN_TEXT_CONTRAST,
    );
}

fn check_shadow(checker: &mut Checker, shadow: &mut Shadow) {
    let styles = [
        ("unfocused", &mut shadow.unfocused),
        ("focused", &mut shadow.focused),
        ("popup", &mut shadow.popup),
        ("modal", &mut shadow.modal),
    ];
    for (name, style) in styles {
        check_shadow_style(checker, &format!("shadow.{name}"), style);
    }
}

fn check_shadow_style(checker: &mut Checker, path: &str, style: &mut ShadowStyle) {
    checker.float(&format!("{path}.blur"), &mut style.blur, SHADOW_BLUR);
    checker.float(&format!("{path}.spread"), &mut style.spread, SHADOW_SPREAD);
    checker.float(
        &format!("{path}.offset_x"),
        &mut style.offset_x,
        SHADOW_OFFSET,
    );
    checker.float(
        &format!("{path}.offset_y"),
        &mut style.offset_y,
        SHADOW_OFFSET,
    );
    checker.color(&format!("{path}.color"), &mut style.color, false);
}

fn check_spacing(checker: &mut Checker, spacing: &mut Spacing) {
    checker.float("spacing.unit", &mut spacing.unit, SPACING_UNIT);
    let unit = spacing.unit;
    let steps = [
        ("xxs", &mut spacing.xxs),
        ("xs", &mut spacing.xs),
        ("sm", &mut spacing.sm),
        ("md", &mut spacing.md),
        ("lg", &mut spacing.lg),
        ("xl", &mut spacing.xl),
        ("xxl", &mut spacing.xxl),
        ("xxxl", &mut spacing.xxxl),
    ];
    for (name, step) in steps {
        let field = format!("spacing.{name}");
        checker.float(&field, step, SPACING_STEP);
        let snapped = (*step / unit).round() * unit;
        if snapped != *step {
            checker.report(
                &field,
                format!("{step} is off the {unit} px grid; using {snapped}"),
            );
            *step = snapped;
        }
    }
}

fn check_typography(checker: &mut Checker, typography: &mut Typography) {
    let defaults = Typography::default();
    families(
        checker,
        "typography.ui_families",
        &mut typography.ui_families,
        defaults.ui_families,
    );
    families(
        checker,
        "typography.mono_families",
        &mut typography.mono_families,
        defaults.mono_families,
    );
    let sizes = &mut typography.sizes;
    let fields = [
        ("caption", &mut sizes.caption),
        ("small", &mut sizes.small),
        ("body", &mut sizes.body),
        ("subtitle", &mut sizes.subtitle),
        ("title", &mut sizes.title),
        ("headline", &mut sizes.headline),
        ("display", &mut sizes.display),
    ];
    for (name, size) in fields {
        checker.float(&format!("typography.sizes.{name}"), size, FONT_SIZE);
    }
    let weights = &mut typography.weights;
    for (name, value) in [
        ("regular", &mut weights.regular),
        ("medium", &mut weights.medium),
        ("semibold", &mut weights.semibold),
        ("bold", &mut weights.bold),
    ] {
        weight(checker, &format!("typography.weights.{name}"), value);
    }
    let lines = &mut typography.line_heights;
    checker.float(
        "typography.line_heights.tight",
        &mut lines.tight,
        LINE_HEIGHT,
    );
    checker.float(
        "typography.line_heights.normal",
        &mut lines.normal,
        LINE_HEIGHT,
    );
    checker.float(
        "typography.line_heights.relaxed",
        &mut lines.relaxed,
        LINE_HEIGHT,
    );
}

fn families(checker: &mut Checker, field: &str, list: &mut Vec<String>, default: Vec<String>) {
    let before = list.len();
    list.retain(|name| !name.trim().is_empty());
    if list.is_empty() {
        checker.report(
            field,
            "no usable font family; using the default stack".to_owned(),
        );
        *list = default;
        return;
    }
    if list.len() != before {
        checker.report(field, "empty family names removed".to_owned());
    }
}

fn weight(checker: &mut Checker, field: &str, value: &mut u16) {
    let mut wide = u32::from(*value);
    checker.int(field, &mut wide, WEIGHT_RANGE);
    *value = u16::try_from(wide).unwrap_or(*value);
}

fn check_motion(checker: &mut Checker, motion: &mut Motion) {
    let durations = [
        ("fast_ms", &mut motion.fast_ms),
        ("normal_ms", &mut motion.normal_ms),
        ("slow_ms", &mut motion.slow_ms),
        ("window_open_ms", &mut motion.window_open_ms),
        ("window_close_ms", &mut motion.window_close_ms),
        ("workspace_switch_ms", &mut motion.workspace_switch_ms),
    ];
    for (name, millis) in durations {
        checker.int(&format!("motion.{name}"), millis, (0, MAX_DURATION_MS));
    }
    checker.float(
        "motion.spring.stiffness",
        &mut motion.spring.stiffness,
        SPRING_STIFFNESS,
    );
    checker.float(
        "motion.spring.damping",
        &mut motion.spring.damping,
        SPRING_DAMPING,
    );
    checker.float("motion.spring.mass", &mut motion.spring.mass, SPRING_MASS);
}

fn check_background(
    checker: &mut Checker,
    background: &mut Background,
    mode: crate::Mode,
    accent: Color,
) {
    checker.float("background.noise", &mut background.noise, NOISE);
    let gradient = &mut background.gradient;
    if !gradient.angle_deg.is_finite() {
        checker.report(
            "background.gradient.angle_deg",
            "not a number; using the default angle".to_owned(),
        );
        gradient.angle_deg = ANGLE_FALLBACK;
    }
    gradient.angle_deg = gradient.angle_deg.rem_euclid(FULL_TURN);
    if gradient.stops.len() < MIN_GRADIENT_STOPS {
        checker.report(
            "background.gradient.stops",
            "needs at least two stops; using the default gradient".to_owned(),
        );
        *gradient = Gradient {
            angle_deg: gradient.angle_deg,
            ..Background::for_accent(accent, mode).gradient
        };
    }
    gradient.stops.truncate(MAX_GRADIENT_STOPS);
    for stop in &mut gradient.stops {
        checker.float(
            "background.gradient.stops.position",
            &mut stop.position,
            UNIT_INTERVAL,
        );
        checker.color("background.gradient.stops.color", &mut stop.color, true);
    }
    gradient
        .stops
        .sort_by(|left, right| left.position.total_cmp(&right.position));
}

fn check_effects(checker: &mut Checker, effects: &mut Effects) {
    checker.float("effects.blur_radius", &mut effects.blur_radius, BLUR_RADIUS);
    checker.float(
        "effects.backdrop_saturation",
        &mut effects.backdrop_saturation,
        SATURATION,
    );
    checker.float(
        "effects.panel_translucency",
        &mut effects.panel_translucency,
        UNIT_INTERVAL,
    );
    checker.float(
        "effects.titlebar_translucency",
        &mut effects.titlebar_translucency,
        UNIT_INTERVAL,
    );
    checker.float(
        "effects.popup_translucency",
        &mut effects.popup_translucency,
        UNIT_INTERVAL,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_themes_have_no_issues() {
        for mut theme in [Theme::dark(), Theme::light()] {
            let issues = theme.validate();
            assert!(issues.is_empty(), "{issues:?}");
        }
    }

    #[test]
    fn hostile_values_are_all_repaired() {
        let mut theme = Theme::dark();
        theme.window.corner_radius = -4.0;
        theme.window.title_bar_height = f32::NAN;
        theme.shadow.focused.blur = 1.0e9;
        theme.shadow.focused.color.a = 7.0;
        theme.effects.panel_translucency = 3.0;
        theme.effects.blur_radius = f32::INFINITY;
        theme.motion.fast_ms = u32::MAX;
        theme.typography.sizes.body = 0.0;
        theme.typography.ui_families = vec![String::new(), "  ".into()];
        theme.palette.background.a = 0.5;
        theme.background.noise = -1.0;
        theme.background.gradient.stops.clear();
        theme.background.gradient.angle_deg = 725.0;
        theme.spacing.md = 17.0;
        let issues = theme.validate();
        assert!(issues.len() >= 13, "{issues:?}");
        assert_eq!(theme.window.corner_radius, 0.0);
        assert_eq!(theme.window.title_bar_height, 36.0);
        assert_eq!(theme.shadow.focused.blur, 200.0);
        assert_eq!(theme.shadow.focused.color.a, 1.0);
        assert_eq!(theme.effects.blur_radius, 24.0);
        assert_eq!(theme.motion.fast_ms, MAX_DURATION_MS);
        assert_eq!(
            theme.typography.ui_families,
            Typography::default().ui_families
        );
        assert_eq!(theme.spacing.md, 16.0);
        assert_eq!(theme.background.gradient.stops.len(), 3);
        assert_eq!(theme.background.gradient.angle_deg, 5.0);
        assert!(theme.validate().is_empty(), "validation must be idempotent");
    }

    #[test]
    fn unreadable_text_is_fixed_everywhere() {
        let mut theme = Theme::dark();
        theme.palette.text_primary = theme.palette.background;
        theme.palette.text_on_accent = theme.palette.accent;
        theme.window.title_text_active = theme.window.title_bar_active;
        let issues = theme.validate();
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"palette.text_primary"));
        assert!(fields.contains(&"palette.text_on_accent"));
        assert!(fields.contains(&"window.title_text_active"));
        let palette = &theme.palette;
        assert!(palette.text_primary.contrast_ratio(palette.background) >= MIN_TEXT_CONTRAST);
        assert!(palette.text_on_accent.contrast_ratio(palette.accent) >= MIN_TEXT_CONTRAST);
    }

    #[test]
    fn oversized_buttons_shrink_to_fit() {
        let mut theme = Theme::dark();
        theme.window.title_bar_height = 30.0;
        theme.window.buttons.height = 30.0;
        theme.window.buttons.glyph_size = 24.0;
        let issues = theme.validate();
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"window.buttons.height"));
        assert!(theme.window.buttons.height + BUTTON_MARGIN <= theme.window.title_bar_height);
        assert!(theme.window.buttons.glyph_size <= theme.window.buttons.height);
        assert!(theme.validate().is_empty(), "validation must be idempotent");
    }

    #[test]
    fn button_glyph_and_fills_are_repaired() {
        let mut theme = Theme::dark();
        theme.window.buttons.glyph_active = theme.window.title_bar_active;
        theme.window.buttons.glyph_on_danger = theme.window.buttons.close_hover_fill;
        theme.window.buttons.hover_fill.a = 9.0;
        let issues = theme.validate();
        let fields: Vec<&str> = issues.iter().map(|issue| issue.field.as_str()).collect();
        assert!(fields.contains(&"window.buttons.glyph_active"));
        assert!(fields.contains(&"window.buttons.glyph_on_danger"));
        assert!(fields.contains(&"window.buttons.hover_fill"));
        let buttons = &theme.window.buttons;
        let fill = buttons.close_hover_fill;
        assert!(buttons.glyph_on_danger.contrast_ratio(fill) >= MIN_TEXT_CONTRAST);
        assert!(buttons.hover_fill.a <= 1.0);
        assert!(theme.validate().is_empty(), "validation must be idempotent");
    }

    #[test]
    fn gradient_stops_are_sorted_and_limited() {
        let mut theme = Theme::dark();
        let stop = theme.background.gradient.stops[0];
        theme.background.gradient.stops = (0..12)
            .map(|i| crate::GradientStop {
                position: 1.0 - i as f32 / 12.0,
                ..stop
            })
            .collect();
        theme.validate();
        let stops = &theme.background.gradient.stops;
        assert_eq!(stops.len(), MAX_GRADIENT_STOPS);
        assert!(
            stops
                .windows(2)
                .all(|pair| pair[0].position <= pair[1].position)
        );
    }

    #[test]
    fn weights_are_clamped() {
        let mut theme = Theme::dark();
        theme.window.title_font_weight = 5000;
        theme.typography.weights.regular = 0;
        theme.validate();
        assert_eq!(theme.window.title_font_weight, 900);
        assert_eq!(Theme::dark().window.title_font_weight, 500);
        assert_eq!(theme.typography.weights.regular, 100);
    }
}
