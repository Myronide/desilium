//! Thin raw-GL layer of the blur pipeline. Every function expects the renderer's GL context
//! to be current, which `GlesRenderer::with_context` / `GlesFrame::with_context` guarantee.

use smithay::backend::renderer::gles::ffi::{self, Gles2};
use std::ffi::CString;
use thiserror::Error;

type Name = ffi::types::GLuint;

const VERTEX_SRC: &str = include_str!("../../shaders/blur.vert");
const POSITION_ATTRIB: Name = 0;
const INFO_LOG_CAP: usize = 2048;
const FLOATS_PER_VERTEX: i32 = 2;

/// Everything that can go wrong while building the blur pipeline.
#[derive(Debug, Error)]
pub enum BlurError {
    /// A shader stage did not compile.
    #[error("blur shader failed to compile: {0}")]
    Shader(String),
    /// The program did not link.
    #[error("blur program failed to link: {0}")]
    Link(String),
    /// A framebuffer object was incomplete.
    #[error("blur framebuffer incomplete (status {0:#x})")]
    Framebuffer(u32),
    /// The resources are already borrowed (re-entrant draw), which must not happen.
    #[error("blur resources are busy")]
    Busy,
    /// A size or coordinate did not fit GL's integer range.
    #[error("blur size out of range")]
    Range,
}

/// A linked program plus its uniform locations (`-1` for unknown names is ignored by GL).
#[derive(Debug)]
pub(super) struct Program {
    pub id: Name,
    locations: Vec<(&'static str, i32)>,
}

impl Program {
    /// Compiles the shared vertex shader plus `fragment` and resolves `uniforms`.
    pub fn compile(
        gl: &Gles2,
        fragment: &str,
        uniforms: &[&'static str],
    ) -> Result<Program, BlurError> {
        let vert = compile_stage(gl, ffi::VERTEX_SHADER, VERTEX_SRC)?;
        let frag = compile_stage(gl, ffi::FRAGMENT_SHADER, fragment);
        let frag = match frag {
            Ok(frag) => frag,
            Err(error) => {
                // SAFETY: `vert` is a shader name created above in this context.
                unsafe { gl.DeleteShader(vert) };
                return Err(error);
            }
        };
        let linked = link(gl, vert, frag);
        // SAFETY: both names are valid; deleting after linking only flags them for deletion.
        unsafe {
            gl.DeleteShader(vert);
            gl.DeleteShader(frag);
        }
        let id = linked?;
        let locations = uniforms
            .iter()
            .map(|name| (*name, uniform_location(gl, id, name)))
            .collect();
        Ok(Program { id, locations })
    }

    /// Location of a uniform, `-1` if the compiler optimised it away.
    pub fn loc(&self, name: &str) -> i32 {
        self.locations
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map_or(-1, |(_, location)| *location)
    }

    pub fn delete(&self, gl: &Gles2) {
        // SAFETY: the program name was created in this context and is deleted exactly once.
        unsafe { gl.DeleteProgram(self.id) };
    }
}

fn uniform_location(gl: &Gles2, program: Name, name: &str) -> i32 {
    let Ok(name) = CString::new(name) else {
        return -1;
    };
    // SAFETY: `name` is NUL-terminated and outlives the call.
    unsafe { gl.GetUniformLocation(program, name.as_ptr()) }
}

fn compile_stage(gl: &Gles2, kind: u32, source: &str) -> Result<Name, BlurError> {
    let length = i32::try_from(source.len()).map_err(|_| BlurError::Range)?;
    // SAFETY: the source pointer/length pair is valid for the duration of the call and the
    // info log buffer is sized by `INFO_LOG_CAP`.
    unsafe {
        let shader = gl.CreateShader(kind);
        let pointer = source.as_ptr().cast();
        gl.ShaderSource(shader, 1, &pointer, &length);
        gl.CompileShader(shader);
        let mut status = 0;
        gl.GetShaderiv(shader, ffi::COMPILE_STATUS, &mut status);
        if status == i32::from(ffi::TRUE) {
            return Ok(shader);
        }
        let mut log = vec![0_u8; INFO_LOG_CAP];
        let mut written = 0;
        gl.GetShaderInfoLog(
            shader,
            INFO_LOG_CAP as i32,
            &mut written,
            log.as_mut_ptr().cast(),
        );
        gl.DeleteShader(shader);
        Err(BlurError::Shader(log_text(log, written)))
    }
}

fn link(gl: &Gles2, vert: Name, frag: Name) -> Result<Name, BlurError> {
    // SAFETY: names are valid shaders of this context; the attribute name is NUL-terminated.
    unsafe {
        let program = gl.CreateProgram();
        gl.AttachShader(program, vert);
        gl.AttachShader(program, frag);
        gl.BindAttribLocation(program, POSITION_ATTRIB, c"a_pos".as_ptr());
        gl.LinkProgram(program);
        let mut status = 0;
        gl.GetProgramiv(program, ffi::LINK_STATUS, &mut status);
        if status == i32::from(ffi::TRUE) {
            return Ok(program);
        }
        let mut log = vec![0_u8; INFO_LOG_CAP];
        let mut written = 0;
        gl.GetProgramInfoLog(
            program,
            INFO_LOG_CAP as i32,
            &mut written,
            log.as_mut_ptr().cast(),
        );
        gl.DeleteProgram(program);
        Err(BlurError::Link(log_text(log, written)))
    }
}

fn log_text(mut log: Vec<u8>, written: i32) -> String {
    log.truncate(usize::try_from(written).unwrap_or(0));
    String::from_utf8_lossy(&log).into_owned()
}

/// An RGBA8 texture with a framebuffer object attached.
#[derive(Debug, Clone, Copy)]
pub(super) struct Target {
    pub texture: Name,
    pub fbo: Name,
    /// Allocated size in texels.
    pub size: (i32, i32),
}

impl Target {
    /// Allocates an uninitialised target. Leaves the texture/framebuffer bindings changed;
    /// the caller restores them via [`SavedState`].
    pub fn create(gl: &Gles2, size: (i32, i32)) -> Result<Target, BlurError> {
        // SAFETY: plain object creation with valid enums; the null data pointer asks GL for
        // uninitialised storage.
        unsafe {
            let mut texture = 0;
            gl.GenTextures(1, &mut texture);
            gl.BindTexture(ffi::TEXTURE_2D, texture);
            for (parameter, value) in [
                (ffi::TEXTURE_MIN_FILTER, ffi::LINEAR),
                (ffi::TEXTURE_MAG_FILTER, ffi::LINEAR),
                (ffi::TEXTURE_WRAP_S, ffi::CLAMP_TO_EDGE),
                (ffi::TEXTURE_WRAP_T, ffi::CLAMP_TO_EDGE),
            ] {
                gl.TexParameteri(ffi::TEXTURE_2D, parameter, value as i32);
            }
            gl.TexImage2D(
                ffi::TEXTURE_2D,
                0,
                ffi::RGBA as i32,
                size.0,
                size.1,
                0,
                ffi::RGBA,
                ffi::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            let mut fbo = 0;
            gl.GenFramebuffers(1, &mut fbo);
            gl.BindFramebuffer(ffi::FRAMEBUFFER, fbo);
            gl.FramebufferTexture2D(
                ffi::FRAMEBUFFER,
                ffi::COLOR_ATTACHMENT0,
                ffi::TEXTURE_2D,
                texture,
                0,
            );
            let status = gl.CheckFramebufferStatus(ffi::FRAMEBUFFER);
            let target = Target { texture, fbo, size };
            if status == ffi::FRAMEBUFFER_COMPLETE {
                return Ok(target);
            }
            target.delete(gl);
            Err(BlurError::Framebuffer(status))
        }
    }

    pub fn delete(&self, gl: &Gles2) {
        // SAFETY: both names belong to this context and are deleted exactly once.
        unsafe {
            gl.DeleteFramebuffers(1, &self.fbo);
            gl.DeleteTextures(1, &self.texture);
        }
    }
}

/// GL state the blur pipeline changes, restored so smithay's frame is unaffected.
#[derive(Debug, Clone, Copy)]
pub(super) struct SavedState {
    pub framebuffer: i32,
    pub viewport: [i32; 4],
    program: i32,
    texture: i32,
    active_texture: i32,
    blend: bool,
}

impl SavedState {
    pub fn capture(gl: &Gles2) -> SavedState {
        let mut state = SavedState {
            framebuffer: 0,
            viewport: [0; 4],
            program: 0,
            texture: 0,
            active_texture: 0,
            blend: false,
        };
        // SAFETY: every query writes at most the number of integers we provide.
        unsafe {
            gl.GetIntegerv(ffi::FRAMEBUFFER_BINDING, &mut state.framebuffer);
            gl.GetIntegerv(ffi::VIEWPORT, state.viewport.as_mut_ptr());
            gl.GetIntegerv(ffi::CURRENT_PROGRAM, &mut state.program);
            gl.GetIntegerv(ffi::ACTIVE_TEXTURE, &mut state.active_texture);
            gl.ActiveTexture(ffi::TEXTURE0);
            gl.GetIntegerv(ffi::TEXTURE_BINDING_2D, &mut state.texture);
            state.blend = gl.IsEnabled(ffi::BLEND) == ffi::TRUE;
        }
        state
    }

    pub fn restore(&self, gl: &Gles2) {
        // SAFETY: restores values previously read from the same context; negative values
        // cannot occur for object names, `max(0)` keeps the casts total.
        unsafe {
            gl.BindFramebuffer(ffi::FRAMEBUFFER, self.framebuffer.max(0) as Name);
            gl.Viewport(
                self.viewport[0],
                self.viewport[1],
                self.viewport[2],
                self.viewport[3],
            );
            gl.UseProgram(self.program.max(0) as Name);
            gl.ActiveTexture(ffi::TEXTURE0);
            gl.BindTexture(ffi::TEXTURE_2D, self.texture.max(0) as Name);
            gl.ActiveTexture(self.active_texture.max(0) as u32);
            if self.blend {
                gl.Enable(ffi::BLEND);
                gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
            }
        }
    }
}

/// Binds a framebuffer by name (the frame's own target, to read from or draw to).
pub(super) fn bind_framebuffer(gl: &Gles2, name: i32) {
    // SAFETY: the name was queried from this context.
    unsafe { gl.BindFramebuffer(ffi::FRAMEBUFFER, name.max(0) as Name) };
}

/// Re-binds the frame's target with its viewport and smithay's blending (premultiplied).
pub(super) fn bind_final_target(gl: &Gles2, saved: &SavedState) {
    bind_framebuffer(gl, saved.framebuffer);
    // SAFETY: plain state setters with values read from this context.
    unsafe {
        gl.Viewport(
            saved.viewport[0],
            saved.viewport[1],
            saved.viewport[2],
            saved.viewport[3],
        );
        gl.Enable(ffi::BLEND);
        gl.BlendFunc(ffi::ONE, ffi::ONE_MINUS_SRC_ALPHA);
    }
}

/// Binds `target` as render target with a viewport of `content` texels, blending off.
pub(super) fn bind_target(gl: &Gles2, target: &Target, content: (i32, i32)) {
    // SAFETY: valid framebuffer name of this context.
    unsafe {
        gl.BindFramebuffer(ffi::FRAMEBUFFER, target.fbo);
        gl.Viewport(0, 0, content.0, content.1);
        gl.Disable(ffi::BLEND);
    }
}

/// Binds `texture` to unit 0 for the active program.
pub(super) fn bind_source(gl: &Gles2, program: &Program, texture: Name) {
    // SAFETY: valid names of this context; the sampler uniform is set on the active program.
    unsafe {
        gl.UseProgram(program.id);
        gl.ActiveTexture(ffi::TEXTURE0);
        gl.BindTexture(ffi::TEXTURE_2D, texture);
        gl.Uniform1i(program.loc("tex"), 0);
    }
}

pub(super) fn uniform_1f(gl: &Gles2, location: i32, value: f32) {
    // SAFETY: the owning program is in use; location -1 is silently ignored by GL.
    unsafe { gl.Uniform1f(location, value) };
}

pub(super) fn uniform_2f(gl: &Gles2, location: i32, value: (f32, f32)) {
    // SAFETY: see `uniform_1f`.
    unsafe { gl.Uniform2f(location, value.0, value.1) };
}

pub(super) fn uniform_4f(gl: &Gles2, location: i32, value: [f32; 4]) {
    // SAFETY: see `uniform_1f`.
    unsafe { gl.Uniform4f(location, value[0], value[1], value[2], value[3]) };
}

/// Draws triangles from client-side clip-space `(x, y)` vertex pairs.
pub(super) fn draw_triangles(gl: &Gles2, vertices: &[f32]) {
    let count = vertices.len() as i32 / FLOATS_PER_VERTEX;
    if count == 0 {
        return;
    }
    // SAFETY: the pointer references `vertices`, which outlives the draw call (client-side
    // arrays are consumed synchronously by DrawArrays); buffer 0 selects client memory.
    unsafe {
        gl.BindBuffer(ffi::ARRAY_BUFFER, 0);
        gl.EnableVertexAttribArray(POSITION_ATTRIB);
        if gl.VertexAttribDivisor.is_loaded() {
            // Smithay leaves instancing divisors set on its own attribute slots.
            gl.VertexAttribDivisor(POSITION_ATTRIB, 0);
        }
        gl.VertexAttribPointer(
            POSITION_ATTRIB,
            FLOATS_PER_VERTEX,
            ffi::FLOAT,
            ffi::FALSE,
            0,
            vertices.as_ptr().cast(),
        );
        gl.DrawArrays(ffi::TRIANGLES, 0, count);
        gl.DisableVertexAttribArray(POSITION_ATTRIB);
    }
}

/// Copies the framebuffer region at `(x, y)` into the bottom-left of `texture`.
pub(super) fn copy_framebuffer(gl: &Gles2, texture: Name, x: i32, y: i32, width: i32, height: i32) {
    // SAFETY: the texture is at least `width x height` and the read framebuffer is the
    // frame's, which the caller has bound.
    unsafe {
        gl.ActiveTexture(ffi::TEXTURE0);
        gl.BindTexture(ffi::TEXTURE_2D, texture);
        gl.CopyTexSubImage2D(ffi::TEXTURE_2D, 0, 0, 0, x, y, width, height);
    }
}
