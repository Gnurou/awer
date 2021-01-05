use gfx::raster::IndexedImage;
use sdl2::{
    rect::Rect,
    video::{GLContext, GLProfile, Window},
    Sdl,
};

use anyhow::{anyhow, Result};

use gl::types::*;
use std::{ffi::CString, mem};

use crate::gfx::{self, raster::RasterBackend, Palette};

use super::{SDL2Renderer, WINDOW_RESOLUTION};

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
    window: Window,
    _opengl_context: GLContext,

    vao: GLuint,
    vbo: GLuint,

    raster_program: GLuint,
    raster: RasterBackend,
    current_framebuffer: IndexedImage,
    current_palette: Palette,
}

impl SDL2GLRenderer {
    pub fn new(sdl_context: &Sdl) -> Result<Self> {
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

        // Check that the GPU supports enough uniform space
        unsafe {
            let mut max_uniform_size = 0;
            const REQUIRED_UNIFORM_SIZE: usize =
                mem::size_of::<IndexedImage>() + mem::size_of::<Palette>();
            gl::GetIntegerv(gl::MAX_UNIFORM_BLOCK_SIZE, &mut max_uniform_size);
            if max_uniform_size < REQUIRED_UNIFORM_SIZE as i32 {
                return Err(anyhow!("Cannot create SDL2 GL renderer: GPU provides {} bytes of uniform space, but we need {}.", max_uniform_size, REQUIRED_UNIFORM_SIZE));
            }
        }

        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let raster_program = link_program(vertex_shader, fragment_shader);

        let mut vao = 0;
        let mut vbo = 0;
        unsafe {
            gl::Disable(gl::DEPTH_TEST);

            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);
        }

        Ok(SDL2GLRenderer {
            window,
            _opengl_context: opengl_context,

            vao,
            vbo,
            raster_program,
            raster: RasterBackend::new(),
            current_framebuffer: Default::default(),
            current_palette: Default::default(),
        })
    }
}

impl SDL2GLRenderer {
    fn blit_game_raster(&mut self) {
        let program = self.raster_program;

        unsafe {
            gl::UseProgram(program);

            // Vertices
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (VERTICES.len() * mem::size_of::<GLfloat>()) as GLsizeiptr,
                VERTICES.as_ptr() as *const _,
                gl::STATIC_DRAW,
            );

            // position attribute
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0,
                2,
                gl::FLOAT,
                gl::FALSE as GLboolean,
                VERTICES_STRIDE,
                std::ptr::null(),
            );

            // scene_position attribute
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(
                1,
                2,
                gl::FLOAT,
                gl::FALSE as GLboolean,
                VERTICES_STRIDE,
                (2 * mem::size_of::<GLfloat>()) as *const _,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
            gl::BindVertexArray(0);

            let scene_uniform = get_uniform_location(program, "scene");
            let palette_uniform = get_uniform_location(program, "palette");

            gl::Uniform1uiv(
                scene_uniform,
                (gfx::SCREEN_RESOLUTION[0] * gfx::SCREEN_RESOLUTION[1] / 4) as GLint,
                self.current_framebuffer.as_ptr() as *const u32,
            );
            gl::Uniform1uiv(
                palette_uniform,
                gfx::PALETTE_SIZE as GLint,
                self.current_palette.as_ptr() as *const u32,
            );

            gl::BindVertexArray(self.vao);
            gl::DrawElements(
                gl::TRIANGLES,
                INDICES.len() as GLint,
                gl::UNSIGNED_BYTE,
                INDICES.as_ptr() as *const _,
            );
            gl::BindVertexArray(0);
        }
    }
}

impl SDL2Renderer for SDL2GLRenderer {
    fn blit_game(&mut self, dst: Rect) {
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

        self.blit_game_raster();
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
        self.raster.set_palette(palette)
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster.fillvideopage(page_id, color_idx)
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster.copyvideopage(src_page_id, dst_page_id, vscroll)
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &gfx::polygon::Polygon,
    ) {
        self.raster
            .fillpolygon(dst_page_id, x, y, color_idx, polygon)
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.raster.blitframebuffer(page_id);

        // Copy the palette and rendered image that we will pass as uniforms
        // to our shader.
        self.current_framebuffer = self.raster.get_framebuffer().clone();
        self.current_palette = self.raster.get_palette().clone();
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster.blit_buffer(dst_page_id, buffer)
    }
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

        program
    }
}

const VERTICES_STRIDE: GLsizei = 4 * mem::size_of::<GLfloat>() as GLsizei;
// Vertices and their coordinate in the scene
static VERTICES: [GLfloat; 16] = [
    -1.0, -1.0, 0.0, 200.0, // Bottom left
    -1.0, 1.0, 0.0, 0.0, // Top left
    1.0, 1.0, 320.0, 0.0, // Top right
    1.0, -1.0, 320.0, 200.0, // Bottom right
];
static INDICES: [GLubyte; 6] = [0, 1, 2, 0, 2, 3];

static VERTEX_SHADER: &str = r#"
#version 330 core

layout (location = 0) in vec2 position;
layout (location = 1) in vec2 scene_position;

out vec2 scene_pos;

void main() {
    scene_pos = scene_position;
    gl_Position = vec4(position, 0.0, 1.0);
}
"#;

static FRAGMENT_SHADER: &str = r#"
#version 330 core

in vec2 scene_pos;

uniform uint scene[320 * 200 / 4];
uniform uint palette[16];

layout (location = 0) out vec4 color;

void main() {
    int x = int(floor(scene_pos.x));
    int y = int(floor(scene_pos.y));
    uint pixel_idx = uint(y * 320 + x);
    uint pixels = scene[pixel_idx / 4u];
    uint pixel = (pixels >> ((pixel_idx % 4u) * 8u)) % 16u;
    uint palette_color = palette[pixel];
    uint r = (palette_color >> 0u) % 256u;
    uint g = (palette_color >> 8u) % 256u;
    uint b = (palette_color >> 16u) % 256u;
    color = vec4(r / 255.0, g / 255.0, b / 255.0, 1.0);
}
"#;
