//! Parameter types of the effect library. Colors are premultiplied RGBA so the
//! shaders can blend with `ONE, ONE_MINUS_SRC_ALPHA` without dark fringes.

use smithay::utils::{Logical, Rectangle};

/// Premultiplied RGBA color, every channel in `0.0..=1.0`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba(pub [f32; 4]);

const BYTE_MAX: f32 = 255.0;

impl Rgba {
    /// Fully transparent black.
    pub const TRANSPARENT: Rgba = Rgba([0.0; 4]);
    /// Opaque black.
    pub const BLACK: Rgba = Rgba([0.0, 0.0, 0.0, 1.0]);
    /// Opaque white.
    pub const WHITE: Rgba = Rgba([1.0; 4]);

    /// Builds a premultiplied color from straight (non-premultiplied) channels.
    pub fn from_straight(red: f32, green: f32, blue: f32, alpha: f32) -> Rgba {
        let alpha = sanitize_unit(alpha);
        Rgba([
            sanitize_unit(red) * alpha,
            sanitize_unit(green) * alpha,
            sanitize_unit(blue) * alpha,
            alpha,
        ])
    }

    /// Builds a color from 8 bit straight channels, e.g. `Rgba::from_rgba8(30, 30, 46, 255)`.
    pub fn from_rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Rgba {
        Rgba::from_straight(
            f32::from(red) / BYTE_MAX,
            f32::from(green) / BYTE_MAX,
            f32::from(blue) / BYTE_MAX,
            f32::from(alpha) / BYTE_MAX,
        )
    }

    /// Scales the whole color (including alpha) by `opacity`; premultiplied math stays valid.
    pub fn with_opacity(self, opacity: f32) -> Rgba {
        let factor = sanitize_unit(opacity);
        Rgba(self.0.map(|channel| channel * factor))
    }

    /// Alpha channel.
    pub fn alpha(self) -> f32 {
        self.0[3]
    }

    /// True when the color covers whatever is below it.
    pub fn is_opaque(self) -> bool {
        self.0[3] >= 1.0
    }

    /// Channels clamped to a valid premultiplied range (NaN becomes 0).
    pub(super) fn sanitized(self) -> [f32; 4] {
        let alpha = sanitize_unit(self.0[3]);
        [
            sanitize_unit(self.0[0]).min(alpha),
            sanitize_unit(self.0[1]).min(alpha),
            sanitize_unit(self.0[2]).min(alpha),
            alpha,
        ]
    }
}

/// Clamps to `0.0..=1.0`, mapping NaN to 0.
pub(super) fn sanitize_unit(value: f32) -> f32 {
    if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    }
}

/// Clamps to a finite non-negative number, mapping NaN and infinities to 0.
pub(super) fn sanitize_length(value: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        0.0
    }
}

/// Corner radii in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Corners {
    /// Top-left radius.
    pub top_left: f32,
    /// Top-right radius.
    pub top_right: f32,
    /// Bottom-right radius.
    pub bottom_right: f32,
    /// Bottom-left radius.
    pub bottom_left: f32,
}

impl Corners {
    /// Square corners.
    pub const NONE: Corners = Corners {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    /// The same radius on all four corners.
    pub fn all(radius: f32) -> Corners {
        Corners {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }

    /// Rounded top corners only, as used by title bars.
    pub fn top(radius: f32) -> Corners {
        Corners {
            top_left: radius,
            top_right: radius,
            ..Corners::NONE
        }
    }

    /// Rounded bottom corners only.
    pub fn bottom(radius: f32) -> Corners {
        Corners {
            bottom_right: radius,
            bottom_left: radius,
            ..Corners::NONE
        }
    }

    /// Largest of the four radii.
    pub fn max(self) -> f32 {
        let radii = self.sanitized();
        radii[0].max(radii[1]).max(radii[2]).max(radii[3])
    }

    /// Radii as `[top-left, top-right, bottom-right, bottom-left]`, finite and non-negative.
    pub(super) fn sanitized(self) -> [f32; 4] {
        [
            sanitize_length(self.top_left),
            sanitize_length(self.top_right),
            sanitize_length(self.bottom_right),
            sanitize_length(self.bottom_left),
        ]
    }
}

/// Which side of the rectangle edge a border occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderPlacement {
    /// Drawn inside the rectangle; the element area is unchanged.
    #[default]
    Inside,
    /// Drawn outside the rectangle; the element area grows by the border width.
    Outside,
}

/// Uniform-width border around a rounded rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    /// Width in logical pixels (snapped to whole physical pixels, at least one).
    pub width: f32,
    /// Border color.
    pub color: Rgba,
    /// Inside or outside the rectangle edge.
    pub placement: BorderPlacement,
}

impl Border {
    /// Inner border of the given width.
    pub fn inside(width: f32, color: Rgba) -> Border {
        Border {
            width,
            color,
            placement: BorderPlacement::Inside,
        }
    }

    /// Outer border of the given width.
    pub fn outside(width: f32, color: Rgba) -> Border {
        Border {
            width,
            color,
            placement: BorderPlacement::Outside,
        }
    }
}

/// Fill of a rounded rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fill {
    /// One flat color.
    Solid(Rgba),
    /// Two-stop linear gradient; `angle_degrees` follows CSS (0 = towards the top, 90 = towards the right,
    /// 180 = towards the bottom).
    Linear {
        /// Color at the start of the gradient line.
        from: Rgba,
        /// Color at the end of the gradient line.
        to: Rgba,
        /// Direction of the gradient line.
        angle_degrees: f32,
    },
}

impl Fill {
    /// Top-to-bottom gradient, the common title bar sheen.
    pub fn vertical(top: Rgba, bottom: Rgba) -> Fill {
        Fill::Linear {
            from: top,
            to: bottom,
            angle_degrees: 180.0,
        }
    }

    /// True when every pixel of the fill is fully opaque.
    pub fn is_opaque(&self) -> bool {
        match self {
            Fill::Solid(color) => color.is_opaque(),
            Fill::Linear { from, to, .. } => from.is_opaque() && to.is_opaque(),
        }
    }
}

/// A rounded, optionally bordered and gradient-filled rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct RoundedRect {
    /// Geometry in logical coordinates.
    pub rect: Rectangle<i32, Logical>,
    /// Output scale the element is rendered at (needed to snap edges to physical pixels).
    pub scale: f64,
    /// Per-corner radii in logical pixels.
    pub radii: Corners,
    /// Interior fill.
    pub fill: Fill,
    /// Optional border.
    pub border: Option<Border>,
    /// Overall opacity multiplier.
    pub alpha: f32,
}

impl RoundedRect {
    /// Solid rectangle with square corners, scale 1 and full opacity.
    pub fn new(rect: Rectangle<i32, Logical>, color: Rgba) -> RoundedRect {
        RoundedRect {
            rect,
            scale: 1.0,
            radii: Corners::NONE,
            fill: Fill::Solid(color),
            border: None,
            alpha: 1.0,
        }
    }

    /// Sets the output scale.
    pub fn scale(mut self, scale: f64) -> RoundedRect {
        self.scale = scale;
        self
    }

    /// Sets the same radius on all corners.
    pub fn radius(mut self, radius: f32) -> RoundedRect {
        self.radii = Corners::all(radius);
        self
    }

    /// Sets individual corner radii.
    pub fn corners(mut self, radii: Corners) -> RoundedRect {
        self.radii = radii;
        self
    }

    /// Replaces the fill.
    pub fn fill(mut self, fill: Fill) -> RoundedRect {
        self.fill = fill;
        self
    }

    /// Adds a border.
    pub fn border(mut self, border: Border) -> RoundedRect {
        self.border = Some(border);
        self
    }

    /// Sets the opacity multiplier.
    pub fn alpha(mut self, alpha: f32) -> RoundedRect {
        self.alpha = alpha;
        self
    }
}

/// Soft drop shadow of a rounded rectangle.
#[derive(Debug, Clone, PartialEq)]
pub struct Shadow {
    /// The caster rectangle (e.g. the window geometry), logical coordinates.
    pub rect: Rectangle<i32, Logical>,
    /// Output scale.
    pub scale: f64,
    /// Corner radii of the caster.
    pub radii: Corners,
    /// Blur extent in logical pixels (CSS `blur-radius`; sigma is half of it).
    pub blur: f32,
    /// Grows (positive) or shrinks (negative) the shadow shape before blurring.
    pub spread: f32,
    /// Horizontal shadow offset, logical pixels.
    pub offset_x: f32,
    /// Vertical shadow offset, logical pixels.
    pub offset_y: f32,
    /// Premultiplied shadow color; its alpha is the peak darkness.
    pub color: Rgba,
    /// Overall opacity multiplier.
    pub alpha: f32,
    /// Cut the shadow out below the caster so translucent windows do not show it through.
    pub knockout: bool,
}

impl Shadow {
    /// A neutral soft shadow: 24 px blur, 8 px downwards, 35% black, knocked out below the caster.
    pub fn new(rect: Rectangle<i32, Logical>) -> Shadow {
        Shadow {
            rect,
            scale: 1.0,
            radii: Corners::NONE,
            blur: 24.0,
            spread: 0.0,
            offset_x: 0.0,
            offset_y: 8.0,
            color: Rgba::BLACK.with_opacity(0.35),
            alpha: 1.0,
            knockout: true,
        }
    }

    /// Sets the output scale.
    pub fn scale(mut self, scale: f64) -> Shadow {
        self.scale = scale;
        self
    }

    /// Sets the caster corner radii.
    pub fn corners(mut self, radii: Corners) -> Shadow {
        self.radii = radii;
        self
    }

    /// Sets blur extent and spread.
    pub fn blur(mut self, blur: f32, spread: f32) -> Shadow {
        self.blur = blur;
        self.spread = spread;
        self
    }

    /// Sets the offset.
    pub fn offset(mut self, x: f32, y: f32) -> Shadow {
        self.offset_x = x;
        self.offset_y = y;
        self
    }

    /// Sets the (premultiplied) color.
    pub fn color(mut self, color: Rgba) -> Shadow {
        self.color = color;
        self
    }

    /// Sets the opacity multiplier.
    pub fn alpha(mut self, alpha: f32) -> Shadow {
        self.alpha = alpha;
        self
    }

    /// Enables or disables the knockout below the caster.
    pub fn knockout(mut self, knockout: bool) -> Shadow {
        self.knockout = knockout;
        self
    }
}

/// One color stop of a [`GradientBackground`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient line, `0.0..=1.0`.
    pub position: f32,
    /// Premultiplied color.
    pub color: Rgba,
}

/// Maximum number of gradient stops the shader supports.
pub const MAX_GRADIENT_STOPS: usize = 4;

/// Full-area linear gradient (desktop background).
#[derive(Debug, Clone, PartialEq)]
pub struct GradientBackground {
    /// Area to cover, logical coordinates (usually the output).
    pub area: Rectangle<i32, Logical>,
    /// Output scale.
    pub scale: f64,
    /// 2 to 4 stops; extra stops are ignored, fewer than two yield no element.
    pub stops: Vec<GradientStop>,
    /// CSS-style angle: 0 = towards the top, 90 = towards the right, 180 = towards the bottom.
    pub angle_degrees: f32,
    /// Dither strength in 8 bit LSBs (1.0 removes banding without visible grain; 0 disables).
    pub dither: f32,
    /// Overall opacity multiplier.
    pub alpha: f32,
}

impl GradientBackground {
    /// Two-stop gradient from `from` to `to` at the given CSS angle.
    pub fn two_stop(
        area: Rectangle<i32, Logical>,
        from: Rgba,
        to: Rgba,
        angle_degrees: f32,
    ) -> Self {
        GradientBackground {
            area,
            scale: 1.0,
            stops: vec![
                GradientStop {
                    position: 0.0,
                    color: from,
                },
                GradientStop {
                    position: 1.0,
                    color: to,
                },
            ],
            angle_degrees,
            dither: 1.0,
            alpha: 1.0,
        }
    }

    /// Sets the output scale.
    pub fn scale(mut self, scale: f64) -> Self {
        self.scale = scale;
        self
    }

    /// Replaces the stops (positions are clamped and sorted by the shader setup).
    pub fn stops(mut self, stops: Vec<GradientStop>) -> Self {
        self.stops = stops;
        self
    }
}
