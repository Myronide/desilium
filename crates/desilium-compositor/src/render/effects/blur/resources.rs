//! GL objects of the blur pipeline (programs and a grow-only texture pool) and the passes.

use super::gl::{self, BlurError, Program, SavedState, Target};
use super::params::BackdropBlurParams;
use super::plan::{
    BlurPlan, MAX_DOWNSAMPLES, PxRect, align_capacity, framebuffer_radii, framebuffer_rect,
    level_size, to_clip,
};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::ffi::Gles2;
use smithay::utils::{Physical, Rectangle};
use std::cell::RefCell;
use std::rc::Rc;

const DOWN_SRC: &str = include_str!("../../shaders/blur_down.frag");
const GAUSS_SRC: &str = include_str!("../../shaders/blur_gauss.frag");
const COMPOSITE_SRC: &str = include_str!("../../shaders/blur_composite.frag");

const SAMPLER_UNIFORMS: [&str; 3] = ["tex", "u_src_tex", "u_src_content"];
const GAUSS_UNIFORMS: [&str; 5] = ["tex", "u_src_tex", "u_src_content", "u_dir", "u_sigma"];
const COMPOSITE_UNIFORMS: [&str; 12] = [
    "tex",
    "u_tex",
    "u_content",
    "u_grab_origin",
    "u_inv_level",
    "u_cubic",
    "u_rect",
    "u_radii",
    "u_saturation",
    "u_tint",
    "u_opacity",
    "u_noise",
];
/// Two triangles covering clip space.
const FULLSCREEN: [f32; 12] = [
    -1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0,
];
const MAX_SATURATION: f32 = 4.0;
const MAX_NOISE: f32 = 8.0;
const VERTICES_PER_RECT: usize = 12;

#[derive(Debug)]
struct Programs {
    down: Program,
    gauss: Program,
    composite: Program,
}

/// A sampled region: texture, allocated size and the valid content size.
#[derive(Debug, Clone, Copy)]
struct Surface {
    target: Target,
    content: (i32, i32),
}

#[derive(Debug, Default)]
struct Pool {
    capacity: (i32, i32),
    /// Level 0 is the framebuffer grab, level `k` the k-th 2x down-sample.
    levels: [Option<Target>; MAX_DOWNSAMPLES as usize + 1],
    /// Gaussian ping/pong pair per lowest level (index = number of down-samples).
    pairs: [Option<[Target; 2]>; MAX_DOWNSAMPLES as usize + 1],
}

/// Shared GL state of every [`super::BackdropBlur`] of one renderer.
///
/// Textures are cached per capacity (rounded up, grow-only, rebuilt when the target size
/// changes) so a steady-state frame allocates nothing on the GPU.
#[derive(Debug)]
pub struct BlurResources {
    programs: Programs,
    pool: Pool,
    vertices: Vec<f32>,
    broken: bool,
}

/// Cloneable handle to the [`BlurResources`] of one GL context (single-threaded by design).
#[derive(Debug, Clone)]
pub struct SharedBlur(Rc<RefCell<BlurResources>>);

impl SharedBlur {
    /// Compiles the blur programs; `Err` means blur is unavailable on this driver.
    pub fn compile(renderer: &mut GlesRenderer) -> Result<SharedBlur, BlurError> {
        let resources = renderer
            .with_context(BlurResources::compile)
            .map_err(|error| BlurError::Shader(error.to_string()))??;
        Ok(SharedBlur(Rc::new(RefCell::new(resources))))
    }

    /// Frees the cached textures (they are re-created on demand). Call when an output was
    /// resized or removed, or blur is turned off, so the pool shrinks back to what is needed.
    pub fn trim(&self, renderer: &mut GlesRenderer) {
        let Ok(mut resources) = self.0.try_borrow_mut() else {
            return;
        };
        let _ = renderer.with_context(|gl| resources.pool.free(gl));
    }

    /// Frees every GL object including the programs; the handle must not draw afterwards.
    pub fn destroy(&self, renderer: &mut GlesRenderer) {
        let Ok(mut resources) = self.0.try_borrow_mut() else {
            return;
        };
        let _ = renderer.with_context(|gl| {
            resources.pool.free(gl);
            let programs = &resources.programs;
            [&programs.down, &programs.gauss, &programs.composite]
                .iter()
                .for_each(|program| program.delete(gl));
        });
        resources.broken = true;
    }

    pub(super) fn borrow_mut(&self) -> Result<std::cell::RefMut<'_, BlurResources>, BlurError> {
        self.0.try_borrow_mut().map_err(|_| BlurError::Busy)
    }
}

/// Everything one composite draw needs besides the GL handles.
pub(super) struct DrawJob<'a> {
    pub params: &'a BackdropBlurParams,
    pub projection: [f32; 9],
    pub dst: Rectangle<i32, Physical>,
    pub damage: &'a [Rectangle<i32, Physical>],
}

impl Pool {
    fn free(&mut self, gl: &Gles2) {
        for target in self.levels.iter_mut().filter_map(Option::take) {
            target.delete(gl);
        }
        for pair in self.pairs.iter_mut().filter_map(Option::take) {
            pair.iter().for_each(|target| target.delete(gl));
        }
        self.capacity = (0, 0);
    }

    /// Makes the pool large enough for `grab` and creates the levels `plan` needs.
    fn prepare(&mut self, gl: &Gles2, grab: (i32, i32), plan: &BlurPlan) -> Result<(), BlurError> {
        let wanted = align_capacity(grab);
        let fits = wanted.0 <= self.capacity.0 && wanted.1 <= self.capacity.1;
        if !fits {
            let keep = self.capacity;
            self.free(gl);
            self.capacity = (wanted.0.max(keep.0), wanted.1.max(keep.1));
        }
        for level in 0..=plan.downsamples {
            let slot = level as usize;
            if self.levels[slot].is_none() {
                self.levels[slot] = Some(Target::create(gl, level_size(self.capacity, level))?);
            }
        }
        let slot = plan.downsamples as usize;
        if self.pairs[slot].is_none() {
            let size = level_size(self.capacity, plan.downsamples);
            self.pairs[slot] = Some([Target::create(gl, size)?, Target::create(gl, size)?]);
        }
        Ok(())
    }

    fn level(&self, level: u32, content: (i32, i32)) -> Option<Surface> {
        let target = self.levels.get(level as usize).copied().flatten()?;
        Some(Surface { target, content })
    }

    fn pair(&self, level: u32, content: (i32, i32)) -> Option<[Surface; 2]> {
        let pair = self.pairs.get(level as usize).copied().flatten()?;
        Some(pair.map(|target| Surface { target, content }))
    }
}

impl BlurResources {
    fn compile(gl: &Gles2) -> Result<BlurResources, BlurError> {
        let down = Program::compile(gl, DOWN_SRC, &SAMPLER_UNIFORMS)?;
        let gauss = Program::compile(gl, GAUSS_SRC, &GAUSS_UNIFORMS)?;
        let composite = Program::compile(gl, COMPOSITE_SRC, &COMPOSITE_UNIFORMS)?;
        Ok(BlurResources {
            programs: Programs {
                down,
                gauss,
                composite,
            },
            pool: Pool::default(),
            vertices: Vec::with_capacity(VERTICES_PER_RECT * 4),
            broken: false,
        })
    }

    /// Runs grab, blur and composite for one element. Any error marks the resources broken.
    pub(super) fn draw(&mut self, gl: &Gles2, job: &DrawJob<'_>) -> Result<(), BlurError> {
        if self.broken {
            return Ok(());
        }
        let saved = SavedState::capture(gl);
        let result = self.draw_inner(gl, job, &saved);
        saved.restore(gl);
        if result.is_err() {
            self.broken = true;
        }
        result
    }

    fn draw_inner(
        &mut self,
        gl: &Gles2,
        job: &DrawJob<'_>,
        saved: &SavedState,
    ) -> Result<(), BlurError> {
        let fb = (saved.viewport[2], saved.viewport[3]);
        let glass = framebuffer_rect(&job.projection, job.dst, fb);
        let plan = BlurPlan::new(job.params.radius, job.params.scale);
        let grab = glass.expand(plan.margin).clamp_to(fb.0, fb.1);
        if glass.is_empty() || grab.is_empty() {
            return Ok(());
        }
        self.pool.prepare(gl, (grab.w, grab.h), &plan)?;
        gl::bind_framebuffer(gl, saved.framebuffer);
        let level0 = self.pool.level(0, (grab.w, grab.h));
        let Some(level0) = level0 else {
            return Ok(());
        };
        gl::copy_framebuffer(gl, level0.target.texture, grab.x, grab.y, grab.w, grab.h);
        let blurred = self.blur(gl, &plan, level0)?;
        self.composite(gl, job, saved, (&plan, blurred, grab, glass))
    }

    /// Down-samples then runs the separable Gaussian; returns the final blurred surface.
    fn blur(&self, gl: &Gles2, plan: &BlurPlan, level0: Surface) -> Result<Surface, BlurError> {
        let mut source = level0;
        for level in 1..=plan.downsamples {
            let content = level_size(level0.content, level);
            let Some(dest) = self.pool.level(level, content) else {
                return Err(BlurError::Range);
            };
            self.pass(gl, &self.programs.down, source, dest, |_, _| {});
            source = dest;
        }
        let Some([ping, pong]) = self.pool.pair(plan.downsamples, source.content) else {
            return Err(BlurError::Range);
        };
        let sigma = plan.level_sigma;
        let direction = |dir: (f32, f32)| {
            move |gl: &Gles2, program: &Program| {
                gl::uniform_2f(gl, program.loc("u_dir"), dir);
                gl::uniform_1f(gl, program.loc("u_sigma"), sigma);
            }
        };
        self.pass(
            gl,
            &self.programs.gauss,
            source,
            ping,
            direction((1.0, 0.0)),
        );
        self.pass(gl, &self.programs.gauss, ping, pong, direction((0.0, 1.0)));
        Ok(pong)
    }

    fn pass(
        &self,
        gl: &Gles2,
        program: &Program,
        source: Surface,
        dest: Surface,
        extra: impl FnOnce(&Gles2, &Program),
    ) {
        gl::bind_target(gl, &dest.target, dest.content);
        gl::bind_source(gl, program, source.target.texture);
        gl::uniform_2f(gl, program.loc("u_src_tex"), as_f32(source.target.size));
        gl::uniform_2f(gl, program.loc("u_src_content"), as_f32(source.content));
        extra(gl, program);
        gl::draw_triangles(gl, &FULLSCREEN);
    }

    fn composite(
        &mut self,
        gl: &Gles2,
        job: &DrawJob<'_>,
        saved: &SavedState,
        (plan, blurred, grab, glass): (&BlurPlan, Surface, PxRect, PxRect),
    ) -> Result<(), BlurError> {
        self.fill_vertices(job);
        let program = &self.programs.composite;
        gl::bind_final_target(gl, saved);
        gl::bind_source(gl, program, blurred.target.texture);
        let params = job.params;
        let scale = params.scale as f32;
        let radii = params.corners.sanitized().map(|radius| radius * scale);
        let fb = (saved.viewport[2], saved.viewport[3]);
        let radii = framebuffer_radii(&job.projection, job.dst, fb, radii);
        gl::uniform_2f(gl, program.loc("u_tex"), as_f32(blurred.target.size));
        gl::uniform_2f(gl, program.loc("u_content"), as_f32(blurred.content));
        gl::uniform_2f(
            gl,
            program.loc("u_grab_origin"),
            (grab.x as f32, grab.y as f32),
        );
        let inv_level = 1.0 / (1_u32 << plan.downsamples) as f32;
        gl::uniform_1f(gl, program.loc("u_inv_level"), inv_level);
        gl::uniform_1f(
            gl,
            program.loc("u_cubic"),
            f32::from(u8::from(plan.downsamples > 0)),
        );
        let rect = [glass.x, glass.y, glass.w, glass.h].map(|value| value as f32);
        gl::uniform_4f(gl, program.loc("u_rect"), rect);
        gl::uniform_4f(gl, program.loc("u_radii"), radii);
        gl::uniform_1f(
            gl,
            program.loc("u_saturation"),
            clamp_finite(params.saturation, 0.0, MAX_SATURATION, 1.0),
        );
        gl::uniform_4f(gl, program.loc("u_tint"), params.tint.sanitized());
        gl::uniform_1f(
            gl,
            program.loc("u_opacity"),
            clamp_finite(params.opacity, 0.0, 1.0, 1.0),
        );
        gl::uniform_1f(
            gl,
            program.loc("u_noise"),
            clamp_finite(params.noise, 0.0, MAX_NOISE, 0.0),
        );
        gl::draw_triangles(gl, &self.vertices);
        Ok(())
    }

    /// One quad per damage rectangle, in clip space, reusing the vertex scratch buffer.
    fn fill_vertices(&mut self, job: &DrawJob<'_>) {
        self.vertices.clear();
        let bounds = Rectangle::from_size(job.dst.size);
        for rect in job
            .damage
            .iter()
            .filter_map(|rect| rect.intersection(bounds))
        {
            let left = (job.dst.loc.x + rect.loc.x) as f32;
            let top = (job.dst.loc.y + rect.loc.y) as f32;
            let right = left + rect.size.w as f32;
            let bottom = top + rect.size.h as f32;
            let corner = |x: f32, y: f32| to_clip(&job.projection, x, y);
            let [a, b, c, d] = [
                corner(left, top),
                corner(right, top),
                corner(right, bottom),
                corner(left, bottom),
            ];
            for point in [a, b, c, a, c, d] {
                self.vertices.extend([point.0, point.1]);
            }
        }
    }
}

fn clamp_finite(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn as_f32(size: (i32, i32)) -> (f32, f32) {
    (size.0 as f32, size.1 as f32)
}
