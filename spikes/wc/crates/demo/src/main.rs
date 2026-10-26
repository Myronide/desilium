//! A CPU-rendered Wayland client: no EGL/dmabuf support required.
use font8x8::UnicodeFonts;
use std::{num::NonZeroU32, rc::Rc};
use winit::{
    application::ApplicationHandler,
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{ResizeDirection, Window, WindowId},
};
#[derive(Default)]
struct Demo {
    window: Option<Rc<Window>>,
    context: Option<softbuffer::Context<Rc<Window>>>,
    surface: Option<softbuffer::Surface<Rc<Window>, Rc<Window>>>,
    pointer: (f64, f64),
}
impl ApplicationHandler for Demo {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Rc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("WC Demo")
                        .with_decorations(false)
                        .with_inner_size(winit::dpi::LogicalSize::new(560.0, 360.0))
                        .with_min_inner_size(winit::dpi::LogicalSize::new(300.0, 220.0)),
                )
                .unwrap(),
        );
        let context = softbuffer::Context::new(window.clone()).unwrap();
        self.surface = Some(softbuffer::Surface::new(&context, window.clone()).unwrap());
        self.context = Some(context);
        self.window = Some(window);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(window) = &self.window else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::CursorMoved { position, .. } => self.pointer = (position.x, position.y),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let size = window.inner_size();
                let (x, y) = self.pointer;
                if y < 38.0 {
                    if x > size.width as f64 - 40.0 {
                        event_loop.exit();
                    } else if x > size.width as f64 - 80.0 {
                        window.set_maximized(!window.is_maximized());
                    } else {
                        let _ = window.drag_window();
                    }
                } else if x > size.width as f64 - 22.0 && y > size.height as f64 - 22.0 {
                    let _ = window.drag_resize_window(ResizeDirection::SouthEast);
                }
            }
            WindowEvent::Resized(_) | WindowEvent::Focused(_) => window.request_redraw(),
            WindowEvent::RedrawRequested => {
                let size = window.inner_size();
                let (Some(w), Some(h)) =
                    (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
                else {
                    return;
                };
                let surface = self.surface.as_mut().unwrap();
                surface.resize(w, h).unwrap();
                let mut pixels = surface.buffer_mut().unwrap();
                let width = size.width as usize;
                let height = size.height as usize;
                for (i, pixel) in pixels.iter_mut().enumerate() {
                    let (x, y) = (i % width, i / width);
                    *pixel = if x < 2 || y < 2 || x >= width - 2 || y >= height - 2 {
                        0x6684ad
                    } else if y < 38 {
                        if window.has_focus() {
                            0x344d70
                        } else {
                            0x283445
                        }
                    } else {
                        0x171f2d
                    };
                }
                text(
                    &mut pixels,
                    width,
                    height,
                    16,
                    13,
                    "WC / FLOATING FIRST",
                    0xd5e4fa,
                    1,
                );
                text(
                    &mut pixels,
                    width,
                    height,
                    width.saturating_sub(64),
                    13,
                    "[]  X",
                    0xd5e4fa,
                    1,
                );
                text(
                    &mut pixels,
                    width,
                    height,
                    28,
                    76,
                    "YOUR SPACE.",
                    0xb7d2ff,
                    2,
                );
                text(
                    &mut pixels,
                    width,
                    height,
                    28,
                    102,
                    "YOUR WINDOWS.",
                    0xb7d2ff,
                    2,
                );
                for (row, line) in [
                    "Drag the titlebar to move.",
                    "Drag to an edge to snap.",
                    "Bottom-right corner: resize.",
                    "Super + arrows: arrange.",
                    "Alt + Tab: switch windows.",
                    "Super + comma: settings.",
                ]
                .iter()
                .enumerate()
                {
                    text(
                        &mut pixels,
                        width,
                        height,
                        28,
                        158 + row * 22,
                        line,
                        0x9eafc6,
                        1,
                    );
                }
                text(
                    &mut pixels,
                    width,
                    height,
                    width.saturating_sub(20),
                    height.saturating_sub(16),
                    "/",
                    0xa8c5ff,
                    1,
                );
                pixels.present().unwrap();
            }
            _ => {}
        }
    }
}
#[allow(clippy::too_many_arguments)]
fn text(
    pixels: &mut [u32],
    width: usize,
    height: usize,
    x: usize,
    y: usize,
    text: &str,
    color: u32,
    scale: usize,
) {
    for (index, ch) in text.chars().enumerate() {
        if let Some(glyph) = font8x8::BASIC_FONTS.get(ch) {
            for (row, bits) in glyph.into_iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = x + index * 8 * scale + col * scale + sx;
                                let py = y + row * scale + sy;
                                if px < width && py < height {
                                    pixels[py * width + px] = color;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    EventLoop::new()?.run_app(&mut Demo::default())?;
    Ok(())
}
