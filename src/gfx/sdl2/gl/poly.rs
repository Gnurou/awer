use std::{iter::once, mem};

use gfx::SCREEN_RESOLUTION;
use gl::types::*;

use crate::gfx::{self, polygon::Polygon, Palette, Point};
use anyhow::Result;

use super::{compile_shader, get_uniform_location, link_program};

pub enum RenderingMode {
    Poly,
    Line,
}

pub struct SDL2GLPolyRenderer {
    vao: GLuint,
    vbo: GLuint,

    program: GLuint,
    polys: [Vec<(Polygon, i16, i16, u8)>; 4],
    framebuffer_index: usize,
}

impl Drop for SDL2GLPolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

impl SDL2GLPolyRenderer {
    pub fn new() -> Result<SDL2GLPolyRenderer> {
        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vertex_shader, fragment_shader);

        let mut vao = 0;
        let mut vbo = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::GenBuffers(1, &mut vbo);

            gl::BindVertexArray(vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            // We shall have no poly with more than 256 points.
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (256 * mem::size_of::<Point<u16>>()) as GLsizeiptr,
                std::ptr::null() as *const _,
                gl::STREAM_DRAW,
            );

            // vertex attribute
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribIPointer(
                0,
                2,
                gl::SHORT,
                (mem::size_of::<u16>() * 2) as GLsizei,
                std::ptr::null(),
            );
            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);
        }

        Ok(SDL2GLPolyRenderer {
            vao,
            vbo,
            program,

            polys: Default::default(),
            framebuffer_index: 0,
        })
    }

    pub fn blit(&mut self, palette: &Palette, rendering_mode: RenderingMode) {
        let polys = &self.polys[self.framebuffer_index];

        for poly in polys {
            let draw_type = if poly.0.bbw == 0 || poly.0.bbh == 0 {
                gl::LINE_LOOP
            } else {
                match rendering_mode {
                    RenderingMode::Poly => gl::TRIANGLE_STRIP,
                    RenderingMode::Line => {
                        if poly.0.bbw == SCREEN_RESOLUTION[0] as u16
                            && poly.0.bbh == SCREEN_RESOLUTION[1] as u16
                        {
                            gl::TRIANGLE_STRIP
                        } else {
                            gl::LINE_LOOP
                        }
                    }
                }
            };

            let len = poly.0.points.len() as u16;
            let indices: Vec<u16> = match draw_type {
                gl::TRIANGLE_STRIP => (0..poly.0.points.len() as u16 / 2)
                    .into_iter()
                    .flat_map(|i| once(len - (i + 1)).chain(once(i)))
                    .collect(),
                gl::LINE_LOOP => (0..poly.0.points.len() as u16).into_iter().collect(),
                _ => panic!(),
            };

            unsafe {
                // Vertices
                gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
                gl::BufferSubData(
                    gl::ARRAY_BUFFER,
                    0,
                    (poly.0.points.len() * mem::size_of::<Point<u16>>()) as GLsizeiptr,
                    poly.0.points.as_ptr() as *const _,
                );
                gl::BindBuffer(gl::ARRAY_BUFFER, 0);

                gl::UseProgram(self.program);

                let uniform = get_uniform_location(self.program, "pos");
                gl::Uniform2i(uniform, poly.1 as GLint, poly.2 as GLint);

                let uniform = get_uniform_location(self.program, "bb");
                gl::Uniform2ui(uniform, poly.0.bbw as GLuint, poly.0.bbh as GLuint);

                let uniform = get_uniform_location(self.program, "color_idx");
                gl::Uniform1ui(uniform, poly.3 as GLuint);

                let uniform = get_uniform_location(self.program, "palette");
                gl::Uniform1uiv(
                    uniform,
                    gfx::PALETTE_SIZE as GLint,
                    palette.as_ptr() as *const u32,
                );

                gl::BindVertexArray(self.vao);
                gl::DrawElements(
                    draw_type,
                    indices.len() as GLint,
                    gl::UNSIGNED_SHORT,
                    indices.as_ptr() as *const _,
                );
                gl::BindVertexArray(0);
            }
        }
    }
}

impl gfx::Backend for SDL2GLPolyRenderer {
    fn set_palette(&mut self, _palette: &[u8; 32]) {}

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        let polys = &mut self.polys[page_id];
        polys.clear();

        let w = gfx::SCREEN_RESOLUTION[0] as u16;
        let h = gfx::SCREEN_RESOLUTION[1] as u16;
        polys.push((
            Polygon::new(
                (w, h),
                vec![
                    Point { x: 0, y: 0 },
                    Point { x: w, y: 0 },
                    Point { x: w, y: h },
                    Point { x: 0, y: h },
                ],
            ),
            w as i16 / 2,
            h as i16 / 2,
            color_idx,
        ));
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, _vscroll: i16) {
        let src_polys = self.polys[src_page_id].clone();
        self.polys[dst_page_id] = src_polys;
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &Polygon,
    ) {
        let polys = &mut self.polys[dst_page_id];
        polys.push((polygon.clone(), x, y, color_idx));
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
    }

    fn blit_buffer(&mut self, _dst_page_id: usize, _buffer: &[u8]) {}
}

static VERTEX_SHADER: &str = r#"
#version 330 core

layout (location = 0) in ivec2 vertex;

uniform ivec2 pos;
uniform uvec2 bb;
uniform uint color_idx;
uniform uint palette[16];

out vec3 vertex_color;

void main() {
    if (color_idx >= 0x10u) {
        vertex_color = vec3(0.5);
    } else {
        uint palette_color = palette[color_idx];
        uint r = (palette_color >> 0u) % 256u;
        uint g = (palette_color >> 8u) % 256u;
        uint b = (palette_color >> 16u) % 256u;
        vertex_color = vec3(r / 255.0, g / 255.0, b / 255.0);
    }

    vec2 offset = bb / 2.0;
    vec2 fpos = pos + vertex - offset;

    vec2 normalized_pos = vec2((fpos.x / 320.0) * 2 - 1.0, (fpos.y / 200.0) * 2 - 1.0);
    vec2 flipped_pos = vec2(normalized_pos.x, -normalized_pos.y);

    gl_Position = vec4(flipped_pos, 0.0, 1.0);
}
"#;

static FRAGMENT_SHADER: &str = r#"
#version 330 core

in vec3 vertex_color;

layout (location = 0) out vec4 color;

void main() {
    color = vec4(vertex_color, 1.0);
}
"#;
