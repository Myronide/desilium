//! Building the Blitz document: HTML assembly, fonts, config and root theme attributes.

use std::sync::Arc;

use blitz_dom::{DocumentConfig, FontContext, QualName, local_name, ns};
use blitz_html::HtmlProvider;
use blitz_traits::shell::Viewport;
use parley::fontique::Blob;

use super::{SharedState, SurfaceOptions, UiSurface, color_scheme, shell::FileProvider};
use crate::{UiError, theme};

const DEFAULT_BASE: &str = "file:///";

/// Wraps a fragment into a document and injects the author sheets and fixups.
///
/// Sheets are placed at the end of `<head>` so they win over earlier rules; the
/// fixups come last of all.
pub(super) fn assemble(html: &str, css: &[&str]) -> String {
    let mut styles = String::new();
    for sheet in css {
        styles.push_str("<style>\n");
        styles.push_str(sheet);
        styles.push_str("\n</style>\n");
    }
    styles.push_str("<style>\n");
    styles.push_str(theme::blitz_fixups_css());
    styles.push_str("</style>\n");

    let lower = html.to_ascii_lowercase();
    if let Some(head_end) = lower.find("</head>") {
        let mut out = html.to_owned();
        out.insert_str(head_end, &styles);
        return out;
    }
    if lower.contains("<html") || lower.contains("<body") {
        return format!("{styles}{html}");
    }
    format!("<!doctype html><html><head>{styles}</head><body>{html}</body></html>")
}

fn font_context(fonts: &[Vec<u8>]) -> Result<FontContext, UiError> {
    let mut ctx = FontContext::default();
    for bytes in fonts {
        let decoded = blitz_dom::decode_font_bytes(bytes).into_owned();
        let registered = ctx
            .collection
            .register_fonts(Blob::new(Arc::new(decoded) as _), None);
        if registered.is_empty() {
            return Err(UiError::BadFont);
        }
    }
    Ok(ctx)
}

pub(super) fn config(
    options: &SurfaceOptions,
    physical: (u32, u32),
    shared: &Arc<SharedState>,
) -> Result<DocumentConfig, UiError> {
    let base = options
        .base_dir
        .as_ref()
        .and_then(|dir| url_for_dir(dir))
        .unwrap_or_else(|| DEFAULT_BASE.to_owned());
    let viewport = Viewport::new(
        physical.0,
        physical.1,
        options.scale as f32,
        color_scheme(&options.theme),
    );
    Ok(DocumentConfig {
        viewport: Some(viewport),
        base_url: Some(base),
        net_provider: Some(Arc::new(FileProvider)),
        shell_provider: Some(shared.clone()),
        html_parser_provider: Some(Arc::new(HtmlProvider)),
        font_ctx: Some(font_context(&options.fonts)?),
        ..DocumentConfig::default()
    })
}

fn url_for_dir(dir: &std::path::Path) -> Option<String> {
    let absolute = std::fs::canonicalize(dir).ok()?;
    Some(format!("file://{}/", absolute.display()))
}

impl UiSurface {
    /// Writes the active theme as inline custom properties on the root element.
    pub(super) fn apply_theme_to_root(&mut self) {
        let Some(root) = self.doc.try_root_element().map(|node| node.id) else {
            return;
        };
        let declarations = theme::tokens_declarations(&self.theme);
        let mode = match self.theme.mode {
            desilium_theme::Mode::Dark => "dark",
            desilium_theme::Mode::Light => "light",
        };
        let mut mutator = self.doc.mutate();
        let style = QualName::new(None, ns!(), local_name!("style"));
        mutator.set_attribute(root, style, &declarations);
        let theme_attr = QualName::new(None, ns!(), blitz_dom::LocalName::from("data-theme"));
        mutator.set_attribute(root, theme_attr, mode);
    }
}
