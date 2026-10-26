//! Accent presets offered as swatches.

/// A named accent colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// Name used in messages ("Accent set to Violet").
    pub name: &'static str,
    /// `#RRGGBB`, uppercase.
    pub hex: &'static str,
}

/// The swatch row, brand violet first. Matches the design mockup.
pub const ACCENT_PRESETS: [Preset; 9] = [
    Preset {
        name: "Violet",
        hex: "#7B61FF",
    },
    Preset {
        name: "Blue",
        hex: "#4DA3FF",
    },
    Preset {
        name: "Teal",
        hex: "#35C9C0",
    },
    Preset {
        name: "Green",
        hex: "#2FCB84",
    },
    Preset {
        name: "Amber",
        hex: "#F2B441",
    },
    Preset {
        name: "Orange",
        hex: "#FF8A4D",
    },
    Preset {
        name: "Coral",
        hex: "#FF5C73",
    },
    Preset {
        name: "Pink",
        hex: "#E8348A",
    },
    Preset {
        name: "Slate",
        hex: "#8A93AD",
    },
];

/// The preset whose colour equals `hex` (case-insensitive).
pub fn preset_for(hex: &str) -> Option<&'static Preset> {
    ACCENT_PRESETS
        .iter()
        .find(|preset| preset.hex.eq_ignore_ascii_case(hex))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_valid_unique_hex_colours() {
        for (index, preset) in ACCENT_PRESETS.iter().enumerate() {
            assert!(
                desilium_theme::Color::from_hex(preset.hex).is_ok(),
                "{}",
                preset.name
            );
            let duplicate = ACCENT_PRESETS[index + 1..]
                .iter()
                .any(|p| p.hex == preset.hex);
            assert!(!duplicate, "{}", preset.name);
        }
    }

    #[test]
    fn brand_violet_is_the_first_preset() {
        assert_eq!(ACCENT_PRESETS[0].hex, "#7B61FF");
        assert_eq!(preset_for("#7b61ff").map(|p| p.name), Some("Violet"));
        assert_eq!(preset_for("#000000"), None);
    }
}
