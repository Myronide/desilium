//! Bridge from [`desilium_theme::Theme`] to the CSS custom properties the shell HTML uses.

mod fixups;
mod tokens;

pub use fixups::blitz_fixups_css;
pub use tokens::tokens_css;
pub(crate) use tokens::tokens_declarations;
