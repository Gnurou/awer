use crate::gfx::gl::*;
use crate::gfx::polygon::Polygon;

use super::Program;

#[repr(C, packed)]
struct VertexShaderInput {
    pos: (i16, i16),
    vertex: (i16, i16),
    bb: (u8, u8),
    zoom: f32,
    color: u8,
}

impl VertexShaderInput {
    fn new(pos: (i16, i16), vertex: (i16, i16), bb: (u8, u8), zoom: f32, color: u8) -> Self {
        VertexShaderInput {
            pos,
            vertex,
            bb,
            zoom,
            color,
        }
    }
}

/// How to render the polygons - either filled polygons, or outlines only.
#[derive(Clone, Copy, Debug)]
pub enum PolyRenderingMode {
    Poly,
    Line,
}

/// Allows to render a list of game polys into an 8-bpp OpenGL framebuffer at
/// any resolution, using the GPU. The rendering is still using indexed colors
/// and must be converted to true colors using an `IndexedFrameRenderer`.
pub struct PolyRenderer {
    vao: GLuint,
    vbo: GLuint,
    program: GLuint,

    self_uniform: GLint,
    buffer0_uniform: GLint,

    vertices: Vec<VertexShaderInput>,
    indices: Vec<u16>,
    draw_type: GLuint,
}

impl Drop for PolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

impl Program for PolyRenderer {
    #[tracing::instrument(level = "debug", skip(self))]
    fn activate(&mut self, target_texture: &IndexedTexture, buffer0: &IndexedTexture) {
        let dimensions = target_texture.dimensions();
        unsafe {
            gl::UseProgram(self.program);

            // Setup target texture to self (for transparency effect)
            gl::Uniform1i(self.self_uniform, 0);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, target_texture.as_tex_id());

            // Setup buffer0 (for pixel copy from buffer0)
            gl::Uniform1i(self.buffer0_uniform, 1);
            gl::ActiveTexture(gl::TEXTURE0 + 1);
            gl::BindTexture(gl::TEXTURE_2D, buffer0.as_tex_id());
            // TODO when can we unbind the textures?

            let viewport_uniform = get_uniform_location(self.program, "viewport_size");
            gl::Uniform2f(viewport_uniform, dimensions.0 as f32, dimensions.1 as f32);
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn deactivate(&mut self) {
        self.draw();
    }
}

impl PolyRenderer {
    pub fn new() -> Result<PolyRenderer> {
        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vertex_shader, fragment_shader);

        let mut vao = 0;
        let mut vbo = 0;
        let mut source_fbo = 0;
        let self_uniform;
        let buffer0_uniform;
        unsafe {
            gl::GenFramebuffers(1, &mut source_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, source_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);

            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
            gl::GenBuffers(1, &mut vbo);
            gl::BindBuffer(gl::ARRAY_BUFFER, vbo);

            // pos attribute
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0,
                2,
                gl::SHORT,
                gl::FALSE,
                mem::size_of::<VertexShaderInput>() as GLsizei,
                memoffset::offset_of!(VertexShaderInput, pos) as *const _,
            );

            // vertex attribute
            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(
                1,
                2,
                gl::SHORT,
                gl::FALSE,
                mem::size_of::<VertexShaderInput>() as GLsizei,
                memoffset::offset_of!(VertexShaderInput, vertex) as *const _,
            );

            // bounding box attribute
            gl::EnableVertexAttribArray(2);
            gl::VertexAttribPointer(
                2,
                2,
                gl::UNSIGNED_BYTE,
                gl::FALSE,
                mem::size_of::<VertexShaderInput>() as GLsizei,
                memoffset::offset_of!(VertexShaderInput, bb) as *const _,
            );

            // zoom attribute
            gl::EnableVertexAttribArray(3);
            gl::VertexAttribPointer(
                3,
                1,
                gl::FLOAT,
                gl::FALSE,
                mem::size_of::<VertexShaderInput>() as GLsizei,
                memoffset::offset_of!(VertexShaderInput, zoom) as *const _,
            );

            // color attribute
            gl::EnableVertexAttribArray(4);
            gl::VertexAttribIPointer(
                4,
                1,
                gl::UNSIGNED_BYTE,
                mem::size_of::<VertexShaderInput>() as GLsizei,
                memoffset::offset_of!(VertexShaderInput, color) as *const _,
            );

            gl::BindVertexArray(0);
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);

            self_uniform = get_uniform_location(program, "self");
            buffer0_uniform = get_uniform_location(program, "buffer0");
        }

        Ok(PolyRenderer {
            vao,
            vbo,
            program,
            self_uniform,
            buffer0_uniform,
            vertices: Default::default(),
            indices: Default::default(),
            draw_type: gl::TRIANGLE_STRIP,
        })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn draw_poly(
        &mut self,
        poly: &Polygon,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        color: u8,
        rendering_mode: PolyRenderingMode,
    ) {
        // If the next polygon is transparent, make sure that all previous
        // commands are completed to ensure our self-referencing texture
        // will have up-to-date data.
        if color == 0x10 {
            self.draw();
            unsafe {
                gl::Finish();
            }
        }

        let draw_type = match rendering_mode {
            PolyRenderingMode::Poly => gl::TRIANGLE_STRIP,
            PolyRenderingMode::Line => gl::LINE_LOOP,
        };

        if draw_type != self.draw_type {
            if !self.vertices.is_empty() {
                self.draw();
            }
            self.draw_type = draw_type;
        }

        // If our number of vertices would exceed the number of indexes we support, perform a draw
        // call and start clean. We use >= here because the last element is used to indicate a
        // primitive restart.
        if self.vertices.len() + poly.points.len() >= u16::MAX as usize {
            self.draw();
        }

        let zoom = zoom as f32 / 64.0;
        let index_start = self.vertices.len();
        let poly_len = poly.points.len();
        self.vertices.extend(poly.points.iter().map(|p| {
            VertexShaderInput::new(
                (pos.0, pos.1),
                (p.x + offset.0, p.y + offset.1),
                (poly.bbw, poly.bbh),
                zoom,
                color,
            )
        }));
        match draw_type {
            gl::TRIANGLE_STRIP => self.indices.extend((0..poly_len / 2).flat_map(|i| {
                [
                    (index_start + poly_len - (i + 1)) as u16,
                    (index_start + i) as u16,
                ]
                .into_iter()
            })),
            gl::LINE_LOOP => {
                self.indices
                    .extend((0..poly_len).map(|i| (index_start + i) as u16));
            }
            _ => unreachable!(),
        };
        // Insert a primitive restart to avoid being joined to the next poly.
        self.indices.push(u16::MAX);
    }

    // Send all the pending vertices to the GPU for rendering.
    #[tracing::instrument(level = "debug", skip(self), fields(vertices = self.vertices.len(), indices = self.indices.len()))]
    pub fn draw(&mut self) {
        unsafe {
            // Vertices
            gl::BindVertexArray(self.vao);
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferData(
                gl::ARRAY_BUFFER,
                (self.vertices.len() * mem::size_of::<VertexShaderInput>()) as GLsizeiptr,
                self.vertices.as_ptr() as *const _,
                gl::STREAM_DRAW,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);

            gl::DrawElements(
                self.draw_type,
                self.indices.len() as GLsizei,
                gl::UNSIGNED_SHORT,
                self.indices.as_ptr() as *const GLvoid,
            );

            gl::BindVertexArray(0);
        }

        self.indices.clear();
        self.vertices.clear();
    }
}

static VERTEX_SHADER: &str = std::include_str!("poly_render.vert");
static FRAGMENT_SHADER: &str = std::include_str!("poly_render.frag");
