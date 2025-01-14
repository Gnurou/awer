//! Structs and code to help render the game using OpenGL.
mod game_renderer;
mod indexed_frame_renderer;
mod raster_renderer;

pub use game_renderer::GlGameRenderer;
pub use game_renderer::PolyRenderingMode;
pub use indexed_frame_renderer::IndexedFrameRenderer;
pub use raster_renderer::GlRasterRenderer;

use std::ffi::CStr;
use std::ffi::CString;
use std::mem;

use anyhow::Result;
use gl::types::*;

use crate::gfx;
use crate::gfx::sw::IndexedImage;

pub(crate) fn get_uniform_location(program: GLuint, name: &CStr) -> GLint {
    unsafe { gl::GetUniformLocation(program, name.as_ptr()) }
}

pub(crate) fn compile_shader(src: &str, typ: GLenum) -> GLuint {
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

pub(crate) fn link_program(vertex_shader: GLuint, fragment_shader: GLuint) -> GLuint {
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

/// Implemented by potential sources for the texture of `IndexedTexture`.
pub trait IndexedTextureSource {
    /// Return the (width, height) dimensions of the source frame.
    fn dimensions(&self) -> (usize, usize);
    /// Return a raw pointer to the frame data.
    fn data(&self) -> *const u8;
}

impl IndexedTextureSource for IndexedImage {
    fn dimensions(&self) -> (usize, usize) {
        (gfx::SCREEN_RESOLUTION[0], gfx::SCREEN_RESOLUTION[1])
    }

    fn data(&self) -> *const u8 {
        self.as_ptr()
    }
}

/// An OpenGL texture which format is similar to that of `IndexedImage`, i.e.
/// 4-bpp indexed colors. It can be rendered into by a shader, or be used as
/// a shader input.
#[derive(Debug)]
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

        self.set_raw_data(source.data(), dimensions.0, dimensions.1, xoffset, yoffset)
    }

    fn set_raw_data(
        &mut self,
        data: *const u8,
        width: usize,
        height: usize,
        xoffset: i32,
        yoffset: i32,
    ) {
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.texture);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                xoffset as GLint,
                yoffset as GLint,
                width as GLint,
                height as GLint,
                gl::RED,
                gl::UNSIGNED_BYTE,
                data as *const _,
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
    }
}

/// Trait for GL-based renderers. Any implementor can provide an `IndexedTexture` containing the
/// current game screen in 16 indexed colors. The resolution of the texture is left to the
/// discretion of implementors.
///
/// Note that "GL-based renderer" only means that the renderer is able to display the game
/// framebuffer using GL, not that the framebuffer itself has been rendered using GL.
///
/// The texture itself can be accessed using the required `AsRef` implementation.
pub trait GlRenderer: AsRef<IndexedTexture> {
    /// Update the texture by rendering buffer `page_id` into it.
    fn update_texture(&mut self, page_id: usize);
}

pub struct Viewport {
    pub x: GLint,
    pub y: GLint,
    pub width: GLsizei,
    pub height: GLsizei,
}
