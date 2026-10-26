//! Pure description of what one window's chrome looks like, front to back.
//!
//! Keeping this separate from the GPU code makes the look (colours, order, states) unit-testable
//! without a GL context.

use crate::animation::ButtonFade;
use crate::decoration::{Button, ButtonState, Chrome, Frame, GlyphKind, Metrics};
use desilium_theme::{Color, ShadowStyle, Theme};
use desilium_wm::Rect;

/// Share of white mixed into the top edge of the title bar for a faint sheen.
const SHEEN: f32 = 0.025;
/// Highest title bar translucency that is honoured; more would hide the title text.
const MAX_TRANSLUCENCY: f32 = 0.85;

/// Everything besides the frame that decides the look.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    /// Cross-fade between the inactive (`0`) and active (`1`) colour sets.
    pub focus: f32,
    pub maximized: bool,
    pub fades: [ButtonFade; 3],
    /// Opacity of the title bar fill, `1` = solid (see `title_opacity`).
    pub bar_opacity: f32,
}

/// Title bar fill opacity for the theme's translucency token (`0` solid, `1` see-through).
///
/// Never fully transparent: the title text and buttons keep a surface behind them even if a
/// theme asks for the extreme.
pub fn title_opacity(translucency: f32) -> f32 {
    let clamped = if translucency.is_finite() {
        translucency.clamp(0.0, MAX_TRANSLUCENCY)
    } else {
        0.0
    };
    1.0 - clamped
}

/// One thing to draw; the plan lists them topmost first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Paint {
    /// Outline around the whole frame.
    Border {
        rect: Rect,
        radius: f32,
        width: i32,
        color: Color,
    },
    /// Button symbol; `rect` is the glyph box.
    Glyph {
        kind: GlyphKind,
        rect: Rect,
        color: Color,
    },
    /// Hover or pressed background of a button.
    ButtonFill {
        button: Button,
        rect: Rect,
        radius: f32,
        color: Color,
    },
    /// Title text within `area`.
    Title { area: Rect, color: Color },
    /// Title bar background with rounded top corners.
    TitleBar {
        rect: Rect,
        top_radius: f32,
        top: Color,
        bottom: Color,
    },
}

/// Lists the paints for a framed window; empty without chrome.
pub fn plan(theme: &Theme, metrics: &Metrics, frame: &Frame, look: &Look) -> Vec<Paint> {
    if frame.chrome == Chrome::None {
        return Vec::new();
    }
    let window = &theme.window;
    let rounded = frame.chrome == Chrome::Windowed;
    let mut paints = Vec::new();
    if rounded && frame.border > 0 {
        paints.push(Paint::Border {
            rect: frame.outer,
            radius: window.corner_radius,
            width: frame.border,
            color: pick(look.focus, window.border_active, window.border_inactive),
        });
    }
    for button in Button::ALL {
        let fade = look.fades[button.index()];
        paints.push(Paint::Glyph {
            kind: glyph_kind(button, look.maximized),
            rect: frame.glyph_rect(metrics, button),
            color: glyph_color(theme, button, fade, look.focus),
        });
    }
    for button in Button::ALL {
        let fade = look.fades[button.index()];
        let Some(color) = fill_color(theme, button, fade) else {
            continue;
        };
        paints.push(Paint::ButtonFill {
            button,
            rect: frame.button_rect(button),
            radius: window.buttons.corner_radius,
            color,
        });
    }
    let base = pick(
        look.focus,
        window.title_bar_active,
        window.title_bar_inactive,
    );
    paints.push(Paint::Title {
        area: frame.title_text,
        color: pick(
            look.focus,
            window.title_text_active,
            window.title_text_inactive,
        ),
    });
    paints.push(Paint::TitleBar {
        rect: frame.title_bar,
        top_radius: if rounded { window.corner_radius } else { 0.0 },
        top: faded(
            base.lighten(SHEEN * look.focus.clamp(0.0, 1.0)),
            look.bar_opacity,
        ),
        bottom: faded(base, look.bar_opacity),
    });
    paints
}

/// The drop shadow while cross-fading between the unfocused and focused elevation.
pub fn shadow_style(theme: &Theme, focus: f32) -> ShadowStyle {
    let (on, off) = (&theme.shadow.focused, &theme.shadow.unfocused);
    let amount = focus.clamp(0.0, 1.0);
    if amount >= 1.0 {
        return on.clone();
    }
    let lerp = |from: f32, to: f32| from + (to - from) * amount;
    ShadowStyle {
        blur: lerp(off.blur, on.blur),
        spread: lerp(off.spread, on.spread),
        offset_x: lerp(off.offset_x, on.offset_x),
        offset_y: lerp(off.offset_y, on.offset_y),
        color: blend(off.color, on.color, amount),
    }
}

/// Blends the inactive and active colour: `focus` 0 is inactive, 1 is active.
fn pick(focus: f32, active: Color, inactive: Color) -> Color {
    blend(inactive, active, focus)
}

/// Mixes `from` toward `to`, returning the exact endpoint colours at 0 and 1.
fn blend(from: Color, to: Color, amount: f32) -> Color {
    if amount <= 0.0 {
        return from;
    }
    if amount >= 1.0 {
        return to;
    }
    from.mix(to, amount)
}

fn faded(color: Color, opacity: f32) -> Color {
    color.with_alpha(color.a * opacity)
}

fn glyph_kind(button: Button, maximized: bool) -> GlyphKind {
    match (button, maximized) {
        (Button::Minimize, _) => GlyphKind::Minimize,
        (Button::Maximize, false) => GlyphKind::Maximize,
        (Button::Maximize, true) => GlyphKind::Restore,
        (Button::Close, _) => GlyphKind::Close,
    }
}

fn glyph_color(theme: &Theme, button: Button, fade: ButtonFade, focus: f32) -> Color {
    let buttons = &theme.window.buttons;
    let base = pick(focus, buttons.glyph_active, buttons.glyph_inactive);
    if button == Button::Close && fade.state != ButtonState::Rest {
        return blend(base, buttons.glyph_on_danger, fade.level);
    }
    base
}

/// The hover or pressed background, faded by the button's current level.
fn fill_color(theme: &Theme, button: Button, fade: ButtonFade) -> Option<Color> {
    let buttons = &theme.window.buttons;
    let close = button == Button::Close;
    let full = match fade.state {
        ButtonState::Rest => return None,
        ButtonState::Hover if close => buttons.close_hover_fill,
        ButtonState::Hover => buttons.hover_fill,
        ButtonState::Pressed if close => buttons.close_pressed_fill,
        ButtonState::Pressed => buttons.pressed_fill,
    };
    let level = fade.level.clamp(0.0, 1.0);
    (level > 0.0).then(|| full.with_alpha(full.a * level))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REST: [ButtonFade; 3] = [ButtonFade {
        state: ButtonState::Rest,
        level: 0.0,
    }; 3];

    fn fade(state: ButtonState, level: f32) -> ButtonFade {
        ButtonFade { state, level }
    }

    fn setup(chrome: Chrome) -> (Theme, Metrics, Frame) {
        let theme = Theme::dark();
        let metrics = Metrics::from_theme(&theme);
        let frame = Frame::new(&metrics, chrome, Rect::new(20, 30, 400, 300));
        (theme, metrics, frame)
    }

    fn look(focused: bool, fades: [ButtonFade; 3]) -> Look {
        Look {
            focus: if focused { 1.0 } else { 0.0 },
            maximized: false,
            fades,
            bar_opacity: 1.0,
        }
    }

    #[test]
    fn plan_is_front_to_back() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let paints = plan(&theme, &metrics, &frame, &look(true, REST));
        assert!(matches!(paints.first(), Some(Paint::Border { .. })));
        assert!(matches!(paints.last(), Some(Paint::TitleBar { .. })));
        let glyphs = paints
            .iter()
            .filter(|p| matches!(p, Paint::Glyph { .. }))
            .count();
        assert_eq!(glyphs, 3);
        assert!(!paints.iter().any(|p| matches!(p, Paint::ButtonFill { .. })));
    }

    #[test]
    fn no_chrome_no_paint() {
        let (theme, metrics, frame) = setup(Chrome::None);
        assert!(plan(&theme, &metrics, &frame, &look(true, REST)).is_empty());
    }

    #[test]
    fn maximized_is_square_and_borderless() {
        let (theme, metrics, frame) = setup(Chrome::Maximized);
        let mut look = look(true, REST);
        look.maximized = true;
        let paints = plan(&theme, &metrics, &frame, &look);
        assert!(!paints.iter().any(|p| matches!(p, Paint::Border { .. })));
        assert!(
            paints
                .iter()
                .any(|p| matches!(p, Paint::TitleBar { top_radius, .. } if *top_radius == 0.0))
        );
        assert!(paints.iter().any(|p| matches!(
            p,
            Paint::Glyph {
                kind: GlyphKind::Restore,
                ..
            }
        )));
    }

    #[test]
    fn close_hover_is_danger_with_light_glyph() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let states = [REST[0], REST[1], fade(ButtonState::Hover, 1.0)];
        let paints = plan(&theme, &metrics, &frame, &look(false, states));
        let buttons = &theme.window.buttons;
        assert!(paints.contains(&Paint::ButtonFill {
            button: Button::Close,
            rect: frame.button_rect(Button::Close),
            radius: buttons.corner_radius,
            color: buttons.close_hover_fill,
        }));
        assert!(paints.iter().any(|p| matches!(p, Paint::Glyph { kind: GlyphKind::Close, color, .. } if *color == buttons.glyph_on_danger)));
    }

    #[test]
    fn pressed_uses_the_pressed_fill() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let states = [fade(ButtonState::Pressed, 1.0), REST[1], REST[2]];
        let paints = plan(&theme, &metrics, &frame, &look(true, states));
        assert!(paints.iter().any(|p| matches!(p, Paint::ButtonFill { button: Button::Minimize, color, .. } if *color == theme.window.buttons.pressed_fill)));
    }

    #[test]
    fn focus_changes_colours() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let active = plan(&theme, &metrics, &frame, &look(true, REST));
        let inactive = plan(&theme, &metrics, &frame, &look(false, REST));
        assert_ne!(active, inactive);
        let title = |paints: &[Paint]| {
            paints.iter().find_map(|p| match p {
                Paint::Title { color, .. } => Some(*color),
                _ => None,
            })
        };
        assert_eq!(title(&active), Some(theme.window.title_text_active));
        assert_eq!(title(&inactive), Some(theme.window.title_text_inactive));
    }

    #[test]
    fn focus_blend_lands_between_the_two_colour_sets() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let mut half = look(true, REST);
        half.focus = 0.5;
        let paints = plan(&theme, &metrics, &frame, &half);
        let bar = paints.iter().find_map(|p| match p {
            Paint::TitleBar { bottom, .. } => Some(*bottom),
            _ => None,
        });
        let expected = theme
            .window
            .title_bar_inactive
            .mix(theme.window.title_bar_active, 0.5);
        assert_eq!(bar, Some(expected));
    }

    #[test]
    fn hover_fill_alpha_follows_the_fade_level() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let states = [fade(ButtonState::Hover, 0.5), REST[1], REST[2]];
        let paints = plan(&theme, &metrics, &frame, &look(true, states));
        let alpha = paints.iter().find_map(|p| match p {
            Paint::ButtonFill { color, .. } => Some(color.a),
            _ => None,
        });
        assert_eq!(alpha, Some(theme.window.buttons.hover_fill.a * 0.5));
        let gone = [fade(ButtonState::Hover, 0.0), REST[1], REST[2]];
        let paints = plan(&theme, &metrics, &frame, &look(true, gone));
        assert!(!paints.iter().any(|p| matches!(p, Paint::ButtonFill { .. })));
    }

    #[test]
    fn translucent_bar_keeps_its_colour_and_lowers_only_alpha() {
        let (theme, metrics, frame) = setup(Chrome::Windowed);
        let mut glass = look(true, REST);
        glass.bar_opacity = 0.7;
        let bar = |paints: &[Paint]| {
            paints.iter().find_map(|p| match p {
                Paint::TitleBar { bottom, .. } => Some(*bottom),
                _ => None,
            })
        };
        let solid = bar(&plan(&theme, &metrics, &frame, &look(true, REST)));
        let soft = bar(&plan(&theme, &metrics, &frame, &glass));
        let (solid, soft) = (solid.unwrap_or_default(), soft.unwrap_or_default());
        assert_eq!((soft.r, soft.g, soft.b), (solid.r, solid.g, solid.b));
        assert!((soft.a - 0.7).abs() < 1e-6);
    }

    #[test]
    fn title_opacity_is_bounded_and_survives_garbage() {
        assert_eq!(title_opacity(0.0), 1.0);
        assert!((title_opacity(0.12) - 0.88).abs() < 1e-6);
        assert!((title_opacity(5.0) - (1.0 - MAX_TRANSLUCENCY)).abs() < 1e-6);
        assert_eq!(title_opacity(-1.0), 1.0);
        assert_eq!(title_opacity(f32::NAN), 1.0);
    }

    #[test]
    fn shadow_style_hits_both_ends_exactly_and_blends_between() {
        let theme = Theme::dark();
        assert_eq!(shadow_style(&theme, 1.0), theme.shadow.focused);
        assert_eq!(shadow_style(&theme, 0.0), theme.shadow.unfocused);
        let mid = shadow_style(&theme, 0.5);
        let (on, off) = (&theme.shadow.focused, &theme.shadow.unfocused);
        assert_eq!(mid.blur, (on.blur + off.blur) / 2.0);
    }
}
