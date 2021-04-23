use std::{cell::RefCell, iter::once};

use crate::gfx::{polygon::Polygon, Point, SCREEN_RESOLUTION};

use super::*;

/// How to render the polygons - either filled polygons, or outlines only.
#[derive(Clone, Copy)]
pub enum RenderingMode {
    Poly,
    Line,
}
/// Draw command for a polygon, requesting it to be drawn at coordinates (`x`,
/// `y`) and with color `color`.
#[derive(Clone)]
pub struct PolyDrawCommand {
    poly: Polygon,
    pos: (i16, i16),
    zoom: u16,
    color: u8,
}

impl PolyDrawCommand {
    pub fn new(poly: Polygon, pos: (i16, i16), zoom: u16, color: u8) -> Self {
        Self {
            poly,
            pos,
            zoom,
            color,
        }
    }
}

#[derive(Clone)]
pub struct BlitBufferCommand {
    image: Box<IndexedImage>,
}

impl From<IndexedImage> for BlitBufferCommand {
    fn from(image: IndexedImage) -> Self {
        Self {
            image: Box::new(image),
        }
    }
}

#[derive(Clone)]
pub enum DrawCommand {
    Poly(PolyDrawCommand),
    BlitBuffer(BlitBufferCommand),
}

/// Allows to render a list of game polys into an 8-bpp OpenGL framebuffer at
/// any resolution, using the GPU. The rendering is still using indexed colors
/// and must be converted to true colors using an `IndexedFrameRenderer`.
pub struct PolyRenderer {
    vao: GLuint,
    vbo: GLuint,
    target_fbo: GLuint,
    source_fbo: GLuint,
    source_texture: RefCell<IndexedTexture>,
    program: GLuint,

    pos_uniform: GLint,
    zoom_uniform: GLint,
    bb_uniform: GLint,
    color_uniform: GLint,
    self_uniform: GLint,
    buffer0_uniform: GLint,
}

impl Drop for PolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.source_fbo);
            gl::DeleteFramebuffers(1, &self.target_fbo);
            gl::DeleteBuffers(1, &self.vbo);
            gl::DeleteVertexArrays(1, &self.vao);
            gl::DeleteProgram(self.program);
        }
    }
}

impl PolyRenderer {
    pub fn new() -> Result<PolyRenderer> {
        let vertex_shader = compile_shader(VERTEX_SHADER, gl::VERTEX_SHADER);
        let fragment_shader = compile_shader(FRAGMENT_SHADER, gl::FRAGMENT_SHADER);
        let program = link_program(vertex_shader, fragment_shader);

        let mut vao = 0;
        let mut vbo = 0;
        let mut target_fbo = 0;
        let mut source_fbo = 0;
        let pos_uniform;
        let zoom_uniform;
        let bb_uniform;
        let color_uniform;
        let self_uniform;
        let buffer0_uniform;
        unsafe {
            gl::GenFramebuffers(1, &mut target_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, target_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);

            gl::GenFramebuffers(1, &mut source_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, source_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);

            gl::GenVertexArrays(1, &mut vao);
            gl::BindVertexArray(vao);
            gl::GenBuffers(1, &mut vbo);
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

            pos_uniform = get_uniform_location(program, "pos");
            zoom_uniform = get_uniform_location(program, "zoom");
            bb_uniform = get_uniform_location(program, "bb");
            color_uniform = get_uniform_location(program, "color_idx");
            self_uniform = get_uniform_location(program, "self");
            buffer0_uniform = get_uniform_location(program, "buffer0");
        }

        Ok(PolyRenderer {
            vao,
            vbo,
            target_fbo,
            source_fbo,
            source_texture: RefCell::new(IndexedTexture::new(
                SCREEN_RESOLUTION[0],
                SCREEN_RESOLUTION[1],
            )),
            program,
            pos_uniform,
            zoom_uniform,
            bb_uniform,
            color_uniform,
            self_uniform,
            buffer0_uniform,
        })
    }

    fn draw_poly(&self, command: &PolyDrawCommand, rendering_mode: RenderingMode) {
        let poly = &command.poly;

        let draw_type = if poly.bbw == 0 || poly.bbh == 0 {
            gl::LINE_LOOP
        } else {
            match rendering_mode {
                RenderingMode::Poly => gl::TRIANGLE_STRIP,
                RenderingMode::Line => {
                    if poly.bbw == SCREEN_RESOLUTION[0] as u16
                        && poly.bbh == SCREEN_RESOLUTION[1] as u16
                    {
                        gl::TRIANGLE_STRIP
                    } else {
                        gl::LINE_LOOP
                    }
                }
            }
        };

        let len = poly.points.len() as u16;
        let indices: Vec<u16> = match draw_type {
            gl::TRIANGLE_STRIP => (0..poly.points.len() as u16 / 2)
                .into_iter()
                .flat_map(|i| once(len - (i + 1)).chain(once(i)))
                .collect(),
            gl::LINE_LOOP => (0..poly.points.len() as u16).into_iter().collect(),
            _ => panic!(),
        };

        unsafe {
            // Vertices
            gl::BindBuffer(gl::ARRAY_BUFFER, self.vbo);
            gl::BufferSubData(
                gl::ARRAY_BUFFER,
                0,
                (poly.points.len() * mem::size_of::<Point<u16>>()) as GLsizeiptr,
                poly.points.as_ptr() as *const _,
            );
            gl::BindBuffer(gl::ARRAY_BUFFER, 0);

            gl::Uniform2i(
                self.pos_uniform,
                command.pos.0 as GLint,
                command.pos.1 as GLint,
            );
            gl::Uniform1ui(self.zoom_uniform, command.zoom as GLuint);
            gl::Uniform2ui(self.bb_uniform, poly.bbw as GLuint, poly.bbh as GLuint);
            gl::Uniform1ui(self.color_uniform, command.color as GLuint);

            // If the next polygon is transparent, make sure that all previous
            // commands are completed to ensure our self-referencing texture
            // will have up-to-date data.
            if command.color == 0x10 {
                gl::Finish();
            }

            gl::DrawElements(
                draw_type,
                indices.len() as GLint,
                gl::UNSIGNED_SHORT,
                indices.as_ptr() as *const _,
            );
        }
    }

    fn draw_buffer(&self, command: &BlitBufferCommand, dimensions: (usize, usize)) {
        // TODO super inefficient as we do this for every frame!
        // The texture should rather be in the command, and be refcounted?
        self.source_texture
            .borrow_mut()
            .set_data(&*command.image, 0, 0);

        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.source_fbo);
            gl::FramebufferTexture(
                gl::READ_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                self.source_texture.borrow().as_tex_id(),
                0,
            );
            if gl::CheckFramebufferStatus(gl::READ_FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                panic!("Error while setting framebuffer up");
            }
            gl::BlitFramebuffer(
                0,
                0,
                SCREEN_RESOLUTION[0] as GLint,
                SCREEN_RESOLUTION[1] as GLint,
                0,
                0,
                dimensions.0 as GLint,
                dimensions.1 as GLint,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0);
        }
    }

    fn draw(
        &self,
        command: &DrawCommand,
        dimensions: (usize, usize),
        rendering_mode: RenderingMode,
    ) {
        match command {
            DrawCommand::Poly(poly) => self.draw_poly(&poly, rendering_mode),
            DrawCommand::BlitBuffer(buffer) => self.draw_buffer(&buffer, dimensions),
        }
    }

    pub fn render_into<'a, C: IntoIterator<Item = &'a DrawCommand>>(
        &self,
        draw_commands: C,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
        rendering_mode: RenderingMode,
    ) {
        let dimensions = target_texture.dimensions();
        unsafe {
            gl::Viewport(0, 0, dimensions.0 as GLint, dimensions.1 as GLint);

            // Setup destination framebuffer
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.target_fbo);
            gl::FramebufferTexture(
                gl::DRAW_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                target_texture.as_tex_id(),
                0,
            );
            if gl::CheckFramebufferStatus(gl::DRAW_FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                panic!("Error while setting framebuffer up");
            }

            gl::UseProgram(self.program);

            // Setup texture to target buffer
            gl::Uniform1i(self.self_uniform, 0);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, target_texture.as_tex_id());

            // Setup texture to buffer0
            gl::Uniform1i(self.buffer0_uniform, 1);
            gl::ActiveTexture(gl::TEXTURE0 + 1);
            gl::BindTexture(gl::TEXTURE_2D, buffer0.as_tex_id());
            // TODO when can we unbind the textures?

            let viewport_uniform = get_uniform_location(self.program, "viewport_size");
            gl::Uniform2f(viewport_uniform, dimensions.0 as f32, dimensions.1 as f32);

            gl::BindVertexArray(self.vao);
        }

        for command in draw_commands {
            self.draw(&command, dimensions, rendering_mode);
        }

        unsafe {
            gl::BindVertexArray(0);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }
    }
}

static VERTEX_SHADER: &str = std::include_str!("poly_render.vert");
static FRAGMENT_SHADER: &str = std::include_str!("poly_render.frag");
