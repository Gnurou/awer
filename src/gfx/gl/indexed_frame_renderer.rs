use super::*;

/// A struct to render an `IndexedImage` or any other source for an indexed
/// 16-color frame into a true-color GL framebuffer.
///
/// It works by mapping the frame data into a GL texture and passing the desired
/// `Palette` as a uniform so the fragment shader can lookup the actual color
/// for each pixel.
pub struct IndexedFrameRenderer {
    vao: GLuint,
    vbo: GLuint,
    program: GLuint,
}

impl Drop for IndexedFrameRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

/// Implemented by potential sources for the texture of `IndexedFrameRenderer`.
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

impl IndexedFrameRenderer {
    pub fn new() -> Result<Self> {
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

        Ok(IndexedFrameRenderer { vao, vbo, program })
    }

    /// Renders `framebuffer` using the color `palette` into `target_framebuffer`.
    /// `target_framebuffer` must either be a valid FBO, or `0` in which case
    /// the default framebuffer will be used.
    pub fn render_into(
        &self,
        source: &IndexedTexture,
        palette: &Palette,
        target_framebuffer: GLuint,
        viewport: &Viewport,
    ) {
        unsafe {
            gl::UseProgram(self.program);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, source.as_tex_id());
            let texture_uniform = get_uniform_location(self.program, "game_scene");
            gl::Uniform1i(texture_uniform, 0);

            let palette_uniform = get_uniform_location(self.program, "palette");
            gl::Uniform1uiv(
                palette_uniform,
                gfx::PALETTE_SIZE as GLint,
                palette.as_ptr() as *const u32,
            );

            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, target_framebuffer);
            gl::Viewport(viewport.x, viewport.y, viewport.width, viewport.height);
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
