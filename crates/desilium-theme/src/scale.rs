//! Spacing grid and typography scale.

use serde::{Deserialize, Serialize};

/// Brand: base grid unit in logical px.
pub const GRID_UNIT: f32 = 4.0;

/// Named spacing steps on the 4 px grid (values in logical px).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spacing {
    /// Grid unit all steps snap to.
    pub unit: f32,
    /// Extra extra small (4).
    pub xxs: f32,
    /// Extra small (8).
    pub xs: f32,
    /// Small (12).
    pub sm: f32,
    /// Medium (16).
    pub md: f32,
    /// Large (24).
    pub lg: f32,
    /// Extra large (32).
    pub xl: f32,
    /// Extra extra large (48).
    pub xxl: f32,
    /// Section gap (64).
    pub xxxl: f32,
}

impl Spacing {
    /// All steps in ascending order.
    pub fn steps(&self) -> [f32; 8] {
        [
            self.xxs, self.xs, self.sm, self.md, self.lg, self.xl, self.xxl, self.xxxl,
        ]
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing {
            unit: GRID_UNIT,
            xxs: GRID_UNIT,
            xs: 2.0 * GRID_UNIT,
            sm: 3.0 * GRID_UNIT,
            md: 4.0 * GRID_UNIT,
            lg: 6.0 * GRID_UNIT,
            xl: 8.0 * GRID_UNIT,
            xxl: 12.0 * GRID_UNIT,
            xxxl: 16.0 * GRID_UNIT,
        }
    }
}

/// Font size scale in px.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontSizes {
    /// Captions and badges.
    pub caption: f32,
    /// Secondary labels.
    pub small: f32,
    /// Default UI text.
    pub body: f32,
    /// Section titles.
    pub subtitle: f32,
    /// Dialog titles.
    pub title: f32,
    /// Page headings.
    pub headline: f32,
    /// Clock and hero text.
    pub display: f32,
}

impl FontSizes {
    /// All sizes in ascending order.
    pub fn steps(&self) -> [f32; 7] {
        [
            self.caption,
            self.small,
            self.body,
            self.subtitle,
            self.title,
            self.headline,
            self.display,
        ]
    }
}

impl Default for FontSizes {
    fn default() -> Self {
        FontSizes {
            caption: 11.0,
            small: 12.0,
            body: 13.0,
            subtitle: 15.0,
            title: 18.0,
            headline: 24.0,
            display: 32.0,
        }
    }
}

/// Font weights (CSS numeric scale).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FontWeights {
    /// Body text.
    pub regular: u16,
    /// Emphasis and buttons.
    pub medium: u16,
    /// Titles.
    pub semibold: u16,
    /// Strong emphasis.
    pub bold: u16,
}

impl Default for FontWeights {
    fn default() -> Self {
        FontWeights {
            regular: 400,
            medium: 500,
            semibold: 600,
            bold: 700,
        }
    }
}

/// Line height multipliers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineHeights {
    /// Titles and single-line controls.
    pub tight: f32,
    /// Body text.
    pub normal: f32,
    /// Long-form reading.
    pub relaxed: f32,
}

impl Default for LineHeights {
    fn default() -> Self {
        LineHeights {
            tight: 1.2,
            normal: 1.45,
            relaxed: 1.65,
        }
    }
}

/// Fonts and type scale.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Typography {
    /// UI font family stack, first match wins.
    pub ui_families: Vec<String>,
    /// Monospace family stack.
    pub mono_families: Vec<String>,
    /// Size scale.
    pub sizes: FontSizes,
    /// Weight scale.
    pub weights: FontWeights,
    /// Line height multipliers.
    pub line_heights: LineHeights,
}

impl Typography {
    /// CSS `font-family` value for the UI stack (mockups, web views).
    pub fn ui_css_stack(&self) -> String {
        css_stack(&self.ui_families)
    }

    /// CSS `font-family` value for the monospace stack.
    pub fn mono_css_stack(&self) -> String {
        css_stack(&self.mono_families)
    }
}

const GENERIC_FAMILIES: [&str; 4] = ["sans-serif", "serif", "monospace", "cursive"];

fn css_stack(families: &[String]) -> String {
    let quoted: Vec<String> = families
        .iter()
        .map(|name| match GENERIC_FAMILIES.contains(&name.as_str()) {
            true => name.clone(),
            false => format!("\"{name}\""),
        })
        .collect();
    quoted.join(", ")
}

impl Default for Typography {
    fn default() -> Self {
        let strings = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
        Typography {
            ui_families: strings(&[
                "Inter",
                "Noto Sans",
                "Cantarell",
                "DejaVu Sans",
                "sans-serif",
            ]),
            mono_families: strings(&[
                "JetBrains Mono",
                "Noto Sans Mono",
                "DejaVu Sans Mono",
                "monospace",
            ]),
            sizes: FontSizes::default(),
            weights: FontWeights::default(),
            line_heights: LineHeights::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_spacing_sits_on_the_grid_and_ascends() {
        let spacing = Spacing::default();
        let steps = spacing.steps();
        assert!(steps.iter().all(|step| step % GRID_UNIT == 0.0));
        assert!(steps.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn css_stack_quotes_names_but_not_generics() {
        let stack = Typography::default().ui_css_stack();
        assert_eq!(
            stack,
            "\"Inter\", \"Noto Sans\", \"Cantarell\", \"DejaVu Sans\", sans-serif"
        );
    }

    #[test]
    fn font_sizes_ascend() {
        let sizes = FontSizes::default().steps();
        assert!(sizes.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
