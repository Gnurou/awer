//! Structs and code to help render the game using OpenGL.
pub mod indexed_frame_renderer;
pub mod poly_renderer;

use std::{ffi::CString, mem};

use anyhow::Result;
use gl::types::*;
use indexed_frame_renderer::IndexedTextureSource;

use crate::gfx::{self, raster::IndexedImage, Palette};

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

/// An OpenGL texture which format is similar to that of `IndexedImage`, i.e.
/// 4-bpp indexed colors. It can be rendered into by a shader, or be used as
/// a source.
pub struct IndexedTexture {
    texture: GLuint,
    width: usize,
    height: usize,
}

impl Drop for IndexedTexture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &self.texture);
        }
    }
}

impl IndexedTexture {
    pub fn new(width: usize, height: usize) -> Self {
        let mut texture = 0;
        unsafe {
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RED as i32,
                width as GLint,
                height as GLint,
                0,
                gl::RED,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        Self {
            texture,
            width,
            height,
        }
    }

    pub fn dimensions(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn as_tex_id(&self) -> GLuint {
        self.texture
    }

    pub fn set_data<S: IndexedTextureSource>(&mut self, source: &S, xoffset: i32, yoffset: i32) {
        let dimensions = source.dimensions();

        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                xoffset as GLint,
                yoffset as GLint,
                dimensions.0 as GLint,
                dimensions.1 as GLint,
                gl::RED,
                gl::UNSIGNED_BYTE,
                source.data() as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }
}
