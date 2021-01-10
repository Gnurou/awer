mod poly;
mod raster;

use sdl2::{
    rect::Rect,
    video::{GLContext, GLProfile, Window},
    Sdl,
};

use anyhow::{anyhow, Result};

use gl::types::*;
use std::ffi::CString;

use crate::gfx::{self, polygon::Polygon};

use super::{SDL2Renderer, WINDOW_RESOLUTION};

pub enum RenderingMode {
    Raster,
    Poly,
    Line,
}

/// A GL-based renderer for SDL. Contrary to what the name implies, it still
/// renders using rasterization into a 320x200 texture that is scaled. Howver,
/// it does it much more efficiently than the SDL raster renderer, using a
/// shader that takes the 320x200, 4bpp scene and corresponding palette and
/// infers the actual color of each pixel on the GPU.
///
/// It also operated without using the SDL Canvas API, meaning it can safely be
/// used along with other GL libraries, like ImGUI.
///
/// In the future it should also be able to render a DrawList into polygons at
/// any resolution - ideally we would be able to switch modes on the fly...
pub struct SDL2GLRenderer {
    rendering_mode: RenderingMode,
    window: Window,
    _opengl_context: GLContext,

    raster_renderer: raster::SDL2GLRasterRenderer,
    poly_renderer: poly::SDL2GLPolyRenderer,
}

impl SDL2GLRenderer {
    pub fn new(sdl_context: &Sdl, rendering_mode: RenderingMode) -> Result<Box<Self>> {
        let sdl_video = sdl_context.video().map_err(|s| anyhow!(s))?;

        let gl_attr = sdl_video.gl_attr();
        // TODO use GLES?
        gl_attr.set_context_profile(GLProfile::Core);
        gl_attr.set_context_version(3, 3);

        let window = sdl_video
            .window("Another World", WINDOW_RESOLUTION[0], WINDOW_RESOLUTION[1])
            .opengl()
            .resizable()
            .allow_highdpi()
            .build()?;

        let opengl_context = window.gl_create_context().map_err(|s| anyhow!(s))?;
        gl::load_with(|s| sdl_video.gl_get_proc_address(s) as _);

        unsafe {
            gl::LineWidth(5.0);

            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::STENCIL_TEST);
        }

        Ok(Box::new(SDL2GLRenderer {
            rendering_mode,
            window,
            _opengl_context: opengl_context,

            raster_renderer: raster::SDL2GLRasterRenderer::new()?,
            poly_renderer: poly::SDL2GLPolyRenderer::new()?,
        }))
    }
}

impl SDL2Renderer for SDL2GLRenderer {
    fn blit_game(&mut self, dst: &Rect) {
        unsafe {
            gl::Viewport(
                dst.x(),
                dst.y(),
                dst.width() as GLint,
                dst.height() as GLint,
            );

            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        match self.rendering_mode {
            RenderingMode::Raster => self.raster_renderer.blit(),
            RenderingMode::Poly => self.poly_renderer.blit(poly::RenderingMode::Poly),
            RenderingMode::Line => self.poly_renderer.blit(poly::RenderingMode::Line),
        };
    }

    fn present(&mut self) {
        self.window.gl_swap_window();
    }

    fn as_gfx(&self) -> &dyn crate::gfx::Backend {
        self
    }

    fn as_gfx_mut(&mut self) -> &mut dyn crate::gfx::Backend {
        self
    }

    fn window(&self) -> &Window {
        &self.window
    }
}

impl gfx::Backend for SDL2GLRenderer {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.raster_renderer.set_palette(palette);
        self.poly_renderer.set_palette(palette);
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster_renderer.fillvideopage(page_id, color_idx);
        self.poly_renderer.fillvideopage(page_id, color_idx);
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster_renderer
            .copyvideopage(src_page_id, dst_page_id, vscroll);
        self.poly_renderer
            .copyvideopage(src_page_id, dst_page_id, vscroll);
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &Polygon,
    ) {
        self.raster_renderer
            .fillpolygon(dst_page_id, x, y, color_idx, polygon);
        self.poly_renderer
            .fillpolygon(dst_page_id, x, y, color_idx, polygon);
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.raster_renderer.blitframebuffer(page_id);
        self.poly_renderer.blitframebuffer(page_id);
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster_renderer.blit_buffer(dst_page_id, buffer)
    }

    // TODO get/set snapshot!
}

fn get_uniform_location(program: GLuint, name: &str) -> GLint {
    let cstr = CString::new(name).unwrap();
    unsafe { gl::GetUniformLocation(program, cstr.as_ptr()) }
}

fn compile_shader(src: &str, typ: GLenum) -> GLuint {
    unsafe {
        let shader = gl::CreateShader(typ);

        let src = CString::new(src).unwrap();
        gl::ShaderSource(shader, 1, &src.as_ptr(), std::ptr::null());
        gl::CompileShader(shader);

        let mut status = gl::FALSE as GLint;
        gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);

        if status != gl::TRUE as GLint {
            let mut len = 0;
            gl::GetShaderiv(shader, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; len as usize];
            gl::GetShaderInfoLog(
                shader,
                len,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut GLchar,
            );
            // Remove trailing '\0'
            buf.pop();
            let error_string = String::from_utf8(buf).unwrap();
            panic!("{}", error_string.trim());
        }

        shader
    }
}

fn link_program(vertex_shader: GLuint, fragment_shader: GLuint) -> GLuint {
    unsafe {
        let program = gl::CreateProgram();
        gl::AttachShader(program, vertex_shader);
        gl::AttachShader(program, fragment_shader);
        gl::LinkProgram(program);

        let mut status = gl::FALSE as GLint;
        gl::GetProgramiv(program, gl::LINK_STATUS, &mut status);

        if status != gl::TRUE as GLint {
            let mut len = 0;
            gl::GetProgramiv(program, gl::INFO_LOG_LENGTH, &mut len);
            let mut buf = vec![0u8; (len - 1) as usize];
            gl::GetProgramInfoLog(
                program,
                buf.len() as i32,
                std::ptr::null_mut(),
                buf.as_mut_ptr() as *mut GLchar,
            );
            // Remove trailing '\0'
            buf.pop();
            let error_string = String::from_utf8(buf).unwrap();
            panic!("{}", error_string.trim());
        }

        gl::DeleteShader(fragment_shader);
        gl::DeleteShader(vertex_shader);

        program
    }
}
