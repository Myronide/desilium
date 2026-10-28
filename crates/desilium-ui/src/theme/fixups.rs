//! Every workaround for known Blitz rendering differences, in one place.
//!
//! Findings from `spikes/blitz/REPORT.md`:
//! - weights 500/600 render visibly bolder than in Firefox, 400 and 700 match;
//! - large soft shadows come out slightly weaker than in Firefox;
//! - the mockups' `html, body { background: #000 }` and the dev theme toggle
//!   must not leak into shell surfaces, which have to start transparent.
//!
//! Token generation calls [`weight`] and [`shadow_alpha`]; documents get
//! [`blitz_fixups_css`] appended last so it wins over author CSS of equal
//! specificity. Remove entries here when Blitz fixes the underlying issue.

/// How much lighter the design's medium weight is drawn (500 -> 450).
const MEDIUM_WEIGHT_CORRECTION: i32 = -50;
/// Semibold gets a smaller correction than medium (600 -> 560).
const SEMIBOLD_WEIGHT_CORRECTION: i32 = -40;
const MEDIUM_WEIGHT: u16 = 500;
const SEMIBOLD_WEIGHT: u16 = 600;
/// Shadow opacity boost in percent, compensating Blitz' weaker blur.
const SHADOW_ALPHA_BOOST_PERCENT: f32 = 12.0;

/// Weight to emit for a design weight (needs a variable Inter to take effect).
pub(super) fn weight(design: u16) -> u16 {
    let correction = match design {
        MEDIUM_WEIGHT => MEDIUM_WEIGHT_CORRECTION,
        SEMIBOLD_WEIGHT => SEMIBOLD_WEIGHT_CORRECTION,
        _ => 0,
    };
    u16::try_from(i32::from(design) + correction).unwrap_or(design)
}

/// Alpha to emit for a shadow colour of design alpha `alpha`.
pub(super) fn shadow_alpha(alpha: f32) -> f32 {
    (alpha * (1.0 + SHADOW_ALPHA_BOOST_PERCENT / 100.0)).clamp(0.0, 1.0)
}

/// Structural CSS fixups, appended after the author CSS of every document.
pub fn blitz_fixups_css() -> &'static str {
    "html, body { background: transparent; }\n\
     body { -webkit-font-smoothing: auto; overflow: hidden; }\n\
     .devtoggle { display: none; }\n\
     input { font: inherit; }\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_medium_and_semibold_change() {
        assert_eq!(weight(400), 400);
        assert_eq!(weight(500), 450);
        assert_eq!(weight(600), 560);
        assert_eq!(weight(700), 700);
    }

    #[test]
    fn shadow_alpha_is_boosted_and_clamped() {
        assert!(shadow_alpha(0.5) > 0.5);
        assert_eq!(shadow_alpha(0.99), 1.0);
        assert_eq!(shadow_alpha(0.0), 0.0);
    }
}
