//! The desktop background: one Background-layer surface per output.
//!
//! KWin draws no wallpaper of its own, so the shell does. The image is an `<img>` fitted by CSS
//! (`object-fit`); without an image, or while it loads, the theme's gradient shows.

use std::path::Path;

use desilium_config_types::AppearanceSettings;
use desilium_theme::{Gradient, Theme};
use desilium_ui::{
    SurfaceOptions, UiError, UiSurface,
    wayland::{Anchor, KeyboardMode, Layer, LayerConfig, LayerHost, SurfaceId},
};

use crate::view::{BASE_CSS, esc, icon::file_url};

/// The styles of the desktop icons, drawn on the primary wallpaper surface.
const DESKTOP_CSS: &str = include_str!("../assets/desktop.css");

/// Namespace of the wallpaper layer surfaces.
pub const NAMESPACE: &str = "desilium-wallpaper";
/// Size used until KWin configures the real one.
const PLACEHOLDER: (u32, u32) = (1920, 1080);
const CSS: &str = "html,body{margin:0;padding:0;width:100%;height:100%;overflow:hidden}\
.wall{position:fixed;left:0;top:0;width:100%;height:100%;overflow:hidden}\
.wall img{position:absolute;left:0;top:0;width:100%;height:100%}\
.wall img.fill{object-fit:cover}\
.wall img.fit{object-fit:contain}\
.wall img.stretch{object-fit:fill}\
.wall img.center{object-fit:none}";

/// The style sheets of a wallpaper surface.
pub(crate) const STYLES: [&str; 3] = [BASE_CSS, CSS, DESKTOP_CSS];

/// What the background looks like; two equal looks need no rebuild.
#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    /// CSS `background` value drawn behind (and without) the image.
    pub backdrop: String,
    /// `file://` URL of the image, if one is configured and exists.
    pub image: Option<String>,
    /// CSS class that fits the image: `fill`, `fit`, `stretch` or `center`.
    pub fit: &'static str,
}

impl Look {
    /// The look for `appearance` under `theme`.
    pub fn of(appearance: &AppearanceSettings, theme: &Theme) -> Self {
        let image = appearance
            .wallpaper
            .as_deref()
            .map(Path::new)
            .filter(|path| path.is_file())
            .map(file_url);
        Self {
            backdrop: gradient_css(&theme.background.gradient),
            image,
            fit: fit_class(&appearance.wallpaper_mode),
        }
    }

    /// The document of a wallpaper surface: the picture and the container of the desktop icons.
    pub(crate) fn html(&self) -> String {
        let image = self.image.as_deref().map_or_else(String::new, |url| {
            format!("<img class=\"{}\" src=\"{}\">", self.fit, esc(url))
        });
        // `#desk` carries the desktop icons; a click on it that no icon takes is a click on the
        // desktop.
        format!(
            "<div class=\"wall\" style=\"background:{}\">{image}</div>\
             <div id=desk data-action=desk-bg data-secondary-action=desk-menu data-drag=desk-band></div>",
            esc(&self.backdrop)
        )
    }
}

/// `tile` has no CSS equivalent for an `<img>`; it is shown as `fill`.
fn fit_class(mode: &str) -> &'static str {
    match mode {
        "fit" => "fit",
        "stretch" => "stretch",
        "center" => "center",
        _ => "fill",
    }
}

fn gradient_css(gradient: &Gradient) -> String {
    let stops: Vec<String> = gradient
        .stops
        .iter()
        .map(|stop| {
            let percent = (stop.position.clamp(0.0, 1.0) * 100.0).round();
            format!("{} {percent}%", stop.color.to_hex())
        })
        .collect();
    match stops.as_slice() {
        [] => "#000000".to_owned(),
        [only] => only.split(' ').next().unwrap_or("#000000").to_owned(),
        _ => format!(
            "linear-gradient({}deg, {})",
            gradient.angle_deg.round(),
            stops.join(", ")
        ),
    }
}

/// The wallpaper surfaces and the look they show.
pub struct Wallpaper {
    surfaces: Vec<SurfaceId>,
    look: Look,
}

impl Wallpaper {
    /// Creates one surface per output the compositor reports (one anywhere when it reports none).
    pub fn new(host: &mut LayerHost, look: Look) -> Result<Self, UiError> {
        let surfaces = Self::create(host, &look)?;
        Ok(Self { surfaces, look })
    }

    /// Rebuilds the surfaces when the look changed. Returns whether it did, because everything
    /// that was drawn onto them (the desktop icons) is gone then.
    pub fn update(&mut self, host: &mut LayerHost, look: Look) -> Result<bool, UiError> {
        if look == self.look {
            return Ok(false);
        }
        for id in self.surfaces.drain(..) {
            host.remove_surface(id);
        }
        self.surfaces = Self::create(host, &look)?;
        self.look = look;
        Ok(true)
    }

    /// The surface that carries the desktop icons: the first output's.
    pub fn primary(&self) -> Option<SurfaceId> {
        self.surfaces.first().copied()
    }

    /// Whether `id` is one of the wallpaper surfaces.
    pub fn owns(&self, id: SurfaceId) -> bool {
        self.surfaces.contains(&id)
    }

    /// Whether `id` is the surface that carries the desktop icons.
    pub fn is_primary(&self, id: SurfaceId) -> bool {
        self.primary() == Some(id)
    }

    fn create(host: &mut LayerHost, look: &Look) -> Result<Vec<SurfaceId>, UiError> {
        let outputs = host.output_names();
        let targets: Vec<Option<String>> = if outputs.is_empty() {
            vec![None]
        } else {
            outputs.into_iter().map(Some).collect()
        };
        let mut ids = Vec::new();
        for output in targets {
            let options = SurfaceOptions::new(PLACEHOLDER.0, PLACEHOLDER.1);
            let ui = UiSurface::new(&look.html(), &STYLES, options)?;
            let mut config = LayerConfig::new(NAMESPACE, (0, 0));
            config.layer = Layer::Background;
            if ids.is_empty() {
                // The first output carries the desktop icons; renaming one needs the keyboard.
                config.keyboard = KeyboardMode::OnDemand;
            }
            config.anchor = Anchor {
                top: true,
                bottom: true,
                left: true,
                right: true,
            };
            // Ignore other surfaces' exclusive zones: the background lies under the bar.
            config.exclusive_zone = -1;
            config.output = output;
            ids.push(host.add_surface(config, ui)?);
        }
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_look_is_the_theme_gradient_without_an_image() {
        let look = Look::of(&AppearanceSettings::default(), &Theme::dark());
        assert!(look.backdrop.starts_with("linear-gradient(160deg, "));
        assert_eq!(look.image, None);
        assert_eq!(look.fit, "fill");
    }

    #[test]
    fn a_missing_image_file_is_ignored() {
        let appearance = AppearanceSettings {
            wallpaper: Some("/nonexistent/wall.jpg".to_owned()),
            ..AppearanceSettings::default()
        };
        assert_eq!(Look::of(&appearance, &Theme::dark()).image, None);
    }

    #[test]
    fn an_existing_image_becomes_a_file_url_with_the_chosen_fit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("wall paper.png");
        std::fs::write(&path, b"x").expect("write");
        let appearance = AppearanceSettings {
            wallpaper: Some(path.to_string_lossy().into_owned()),
            wallpaper_mode: "fit".to_owned(),
            ..AppearanceSettings::default()
        };
        let look = Look::of(&appearance, &Theme::dark());
        let url = look.image.as_deref().expect("image");
        assert!(
            url.starts_with("file:///") && url.contains("wall%20paper.png"),
            "{url}"
        );
        assert_eq!(look.fit, "fit");
        assert!(look.html().contains("<img class=\"fit\""));
    }

    #[test]
    fn unknown_and_tile_modes_fall_back_to_fill() {
        assert_eq!(fit_class("tile"), "fill");
        assert_eq!(fit_class("nonsense"), "fill");
        assert_eq!(fit_class("stretch"), "stretch");
        assert_eq!(fit_class("center"), "center");
    }

    #[test]
    fn a_light_theme_has_a_different_backdrop() {
        let dark = Look::of(&AppearanceSettings::default(), &Theme::dark());
        let light = Look::of(&AppearanceSettings::default(), &Theme::light());
        assert_ne!(dark.backdrop, light.backdrop);
    }
}
