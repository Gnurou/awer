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
static VERTEX_SHADER: &str = std::include_str!("indexed_render.vert");
static FRAGMENT_SHADER: &str = std::include_str!("indexed_render.frag");
