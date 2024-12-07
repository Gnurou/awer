use std::mem;

use crate::font::*;
use crate::gfx::gl3::*;

use anyhow::Result;
use gl::types::GLshort;
use gl::types::GLsizei;
use gl::types::GLsizeiptr;
use gl::types::GLuint;

use super::*;

const MAX_PENDING_CHARS: usize = 64;

#[repr(C, packed)]
struct CharVertexInput {
    x: i16,
    y: i16,
    char_x: u16,
    char_y: u16,
    color: u16,
    char_offset: u16,
}

/// A GL renderer for in-game fonts.
pub struct FontRenderer {
    vao: GLuint,
    vbo: GLuint,
    program: GLuint,
}

impl Program for FontRenderer {
    fn activate(&mut self, _target_texture: &IndexedTexture, _buffer0: &IndexedTexture) {
        unsafe {
            gl::UseProgram(self.program);
        }
    }
}

impl FontRenderer {
    pub fn new() -> Result<Self> {
        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vertex_shader, fragment_shader);

        let mut vao = 0;
        let mut vbo = 0;
        unsafe {
            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (MAX_PENDING_CHARS * (mem::size_of::<u16>() * 6)) as GLsizeiptr,
                std::ptr::null() as *const _,
                gl::STREAM_DRAW,
            );

            // pos attribute
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribIPointer(
                0,
                2,
                gl::SHORT,
                mem::size_of::<CharVertexInput>() as GLsizei,
                std::ptr::null(),
            );

            // font_position attribute
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribIPointer(
                1,
                2,
                gl::UNSIGNED_SHORT,
                mem::size_of::<CharVertexInput>() as GLsizei,
                (2 * mem::size_of::<GLshort>()) as *const _,
            );

            // char_color attribute
            gl::EnableVertexAttribArray(2);
            gl::VertexAttribIPointer(
                2,
                1,
                gl::UNSIGNED_SHORT,
                mem::size_of::<CharVertexInput>() as GLsizei,
                (4 * mem::size_of::<GLshort>()) as *const _,
            );

            // char_offset attribute
            gl::EnableVertexAttribArray(3);
            gl::VertexAttribIPointer(
                3,
                1,
                gl::UNSIGNED_SHORT,
                mem::size_of::<CharVertexInput>() as GLsizei,
                (5 * mem::size_of::<GLshort>()) as *const _,
            );

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);

            gl::UseProgram(program);
            let font_uniform = get_uniform_location(program, "font");
            gl::Uniform1uiv(font_uniform, 192, FONT.as_ptr() as *const GLuint);
            gl::UseProgram(0);
        }

        Ok(FontRenderer { vao, vbo, program })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn draw_char(&self, pos: (i16, i16), color: u8, c: u8) {
        let char_offset = (c - FONT_FIRST_CHAR) as u16;
        let color = color as u16;
        // Looks like we are 1 pixel off horizontally?
        let pos = (pos.0 - 1, pos.1);
        let shader_input = [
            CharVertexInput {
                x: pos.0,
                y: pos.1,
                char_x: 0,
                char_y: 0,
                color,
                char_offset,
            },
            CharVertexInput {
                x: pos.0,
                y: pos.1 + CHAR_HEIGHT as i16,
                char_x: 0,
                char_y: 8,
                color,
                char_offset,
            },
            CharVertexInput {
                x: pos.0 + CHAR_WIDTH as i16,
                y: pos.1,
                char_x: 8,
                char_y: 0,
                color,
                char_offset,
            },
            CharVertexInput {
                x: pos.0 + CHAR_WIDTH as i16,
                y: pos.1 + CHAR_HEIGHT as i16,
                char_x: 8,
                char_y: 8,
                color,
                char_offset,
            },
        ];
        unsafe {
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (shader_input.len() * mem::size_of::<CharVertexInput>()) as GLsizeiptr,
                shader_input.as_ptr() as *const _,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);

            gl::DrawArrays(gl::TRIANGLE_STRIP, 0, shader_input.len() as GLsizei);
        }
    }
}

static VERTEX_SHADER: &str = std::include_str!("font_render.vert");
static FRAGMENT_SHADER: &str = std::include_str!("font_render.frag");
