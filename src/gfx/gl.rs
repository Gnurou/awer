//! Structs and code to help render the game using OpenGL.
use std::{ffi::CString, mem};

use anyhow::Result;
use gl::types::*;

use crate::gfx::{self, raster::IndexedImage, Palette};

pub fn get_uniform_location(program: GLuint, name: &str) -> GLint {
    let cstr = CString::new(name).unwrap();
    unsafe { gl::GetUniformLocation(program, cstr.as_ptr()) }
}

pub fn compile_shader(src: &str, typ: GLenum) -> GLuint {
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

pub fn link_program(vertex_shader: GLuint, fragment_shader: GLuint) -> GLuint {
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

/// A struct to render an `IndexedImage` into a true-color GL framebuffer.
/// It works by mapping the `IndexedImage` into a GL texture, and passing the
/// `Palette` as a uniform so the fragment shader can lookup the actual color
/// for each pixel.
///
/// It should also be easily extendable to support FBOs as source, to e.g.
/// render the game at higher resolutions than the legacy 320x200.
pub struct IndexedImageRenderer {
    vao: GLuint,
    vbo: GLuint,
    texture: GLuint,
    program: GLuint,
}

impl Drop for IndexedImageRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.texture);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

impl IndexedImageRenderer {
    pub fn new() -> Result<Self> {
        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vertex_shader, fragment_shader);
        let mut vao = 0;
        let mut vbo = 0;
        let mut texture = 0;

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

            // game scene texture
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RED as i32,
                gfx::SCREEN_RESOLUTION[0] as GLint,
                gfx::SCREEN_RESOLUTION[1] as GLint,
                0,
                gl::RED,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        Ok(IndexedImageRenderer {
            vao,
            vbo,
            texture,
            program,
        })
    }

    /// Renders `framebuffer` using the color `palette` into `target_framebuffer`.
    /// `target_framebuffer` must either be a valid FBO, or `0` in which case
    /// the default framebuffer will be used.
    pub fn render_into(
        &self,
        target_framebuffer: GLuint,
        framebuffer: &IndexedImage,
        palette: &Palette,
    ) {
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                gfx::SCREEN_RESOLUTION[0] as GLint,
                gfx::SCREEN_RESOLUTION[1] as GLint,
                gl::RED,
                gl::UNSIGNED_BYTE,
                framebuffer.as_ptr() as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);

            gl::UseProgram(self.program);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            let texture_uniform = get_uniform_location(self.program, "game_scene");
            gl::Uniform1i(texture_uniform, 0);

            let palette_uniform = get_uniform_location(self.program, "palette");
            gl::Uniform1uiv(
                palette_uniform,
                gfx::PALETTE_SIZE as GLint,
                palette.as_ptr() as *const u32,
            );

            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, target_framebuffer);
            gl::BindVertexArray(self.vao);
            gl::DrawElements(
                gl::TRIANGLES,
                INDICES.len() as GLint,
                gl::UNSIGNED_BYTE,
                INDICES.as_ptr() as *const _,
            );
            gl::BindVertexArray(0);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }
}

const VERTICES_STRIDE: GLsizei = 4 * mem::size_of::<GLfloat>() as GLsizei;
// Vertices and their texture coordinate
static VERTICES: [GLfloat; 16] = [
    -1.0, -1.0, 0.0, 1.0, // Bottom left
    -1.0, 1.0, 0.0, 0.0, // Top left
    1.0, 1.0, 1.0, 0.0, // Top right
    1.0, -1.0, 1.0, 1.0, // Bottom right
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

uniform sampler2D game_scene;
uniform uint palette[16];

layout (location = 0) out vec4 color;

void main() {
    uint pixel = uint((texture(game_scene, scene_pos).r * 256.0));
    uint palette_color = palette[pixel];
    uint r = (palette_color >> 0u) % 256u;
    uint g = (palette_color >> 8u) % 256u;
    uint b = (palette_color >> 16u) % 256u;
    color = vec4(r / 255.0, g / 255.0, b / 255.0, 1.0);
}
"#;
