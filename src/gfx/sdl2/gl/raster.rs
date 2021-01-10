use std::mem;

use gl::types::*;

use anyhow::Result;

use crate::gfx::{
    self,
    raster::{IndexedImage, RasterBackend},
    Palette,
};

use super::{compile_shader, get_uniform_location, link_program};

pub struct SDL2GLRasterRenderer {
    vao: GLuint,
    vbo: GLuint,
    program: GLuint,

    raster: RasterBackend,
    current_framebuffer: IndexedImage,
    current_palette: Palette,
}

impl Drop for SDL2GLRasterRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

impl SDL2GLRasterRenderer {
    pub fn new() -> Result<SDL2GLRasterRenderer> {
        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vertex_shader, fragment_shader);

        let mut vao = 0;
        let mut vbo = 0;

        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            // Vertices
            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
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
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(SDL2GLRasterRenderer {
            vao,
            vbo,
            program,
            raster: RasterBackend::new(),
            current_framebuffer: Default::default(),
            current_palette: Default::default(),
        })
    }

    pub fn blit(&mut self) {
        unsafe {
            gl::UseProgram(self.program);

            let scene_uniform = get_uniform_location(self.program, "scene");
            let palette_uniform = get_uniform_location(self.program, "palette");

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

impl gfx::Backend for SDL2GLRasterRenderer {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.raster.set_palette(palette);
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster.fillvideopage(page_id, color_idx);
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster.copyvideopage(src_page_id, dst_page_id, vscroll);
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
            .fillpolygon(dst_page_id, x, y, color_idx, polygon);
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
