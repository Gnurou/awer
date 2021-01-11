use std::iter::once;

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
    x: i16,
    y: i16,
    color: u8,
}

impl PolyDrawCommand {
    pub fn new(poly: Polygon, x: i16, y: i16, color: u8) -> Self {
        Self { poly, x, y, color }
    }
}

/// Allows to render a list of game polys into an 8-bpp OpenGL framebuffer at
/// any resolution, using the GPU. The rendering is still using indexed colors
/// and must be converted to true colors using an `IndexedFrameRenderer`.
pub struct PolyRenderer {
    vao: GLuint,
    vbo: GLuint,
    fbo: GLuint,
    program: GLuint,

    pos_uniform: GLint,
    bb_uniform: GLint,
    color_uniform: GLint,
    self_uniform: GLint,
    buffer0_uniform: GLint,
}

impl Drop for PolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
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
        let mut fbo = 0;
        let pos_uniform;
        let bb_uniform;
        let color_uniform;
        let self_uniform;
        let buffer0_uniform;
        unsafe {
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, fbo);
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
            bb_uniform = get_uniform_location(program, "bb");
            color_uniform = get_uniform_location(program, "color_idx");
            self_uniform = get_uniform_location(program, "self");
            buffer0_uniform = get_uniform_location(program, "buffer0");
        }

        Ok(PolyRenderer {
            vao,
            vbo,
            fbo,
            program,
            pos_uniform,
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

            gl::Uniform2i(self.pos_uniform, command.x as GLint, command.y as GLint);
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

    pub fn render_into<'a, C: IntoIterator<Item = &'a PolyDrawCommand>>(
        &self,
        draw_commands: C,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
        rendering_mode: RenderingMode,
    ) {
        unsafe {
            let dimensions = target_texture.dimensions();
            gl::Viewport(0, 0, dimensions.0 as GLint, dimensions.1 as GLint);

            // Setup destination framebuffer
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.fbo);
            gl::FramebufferTexture(
                gl::DRAW_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                target_texture.as_tex_id(),
                0,
            );
            if gl::CheckFramebufferStatus(gl::FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
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
            self.draw_poly(&command, rendering_mode);
        }

        unsafe {
            gl::BindVertexArray(0);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }
    }
}

static VERTEX_SHADER: &str = r#"
#version 330 core

layout (location = 0) in ivec2 vertex;

uniform ivec2 pos;
uniform uvec2 bb;
uniform uint color_idx;

void main() {
    vec2 offset = bb / 2.0;
    vec2 fpos = pos + vertex - offset;

    vec2 normalized_pos = vec2((fpos.x / 320.0) * 2 - 1.0, (fpos.y / 200.0) * 2 - 1.0);

    gl_Position = vec4(normalized_pos, 0.0, 1.0);
}
"#;

static FRAGMENT_SHADER: &str = r#"
#version 330 core

uniform uint color_idx;
uniform sampler2D self;
uniform sampler2D buffer0;
uniform vec2 viewport_size;

layout (location = 0) out float color;

void main() {
    if (color_idx == 0x10u) {
        uint source_color = uint(texture(self, gl_FragCoord.xy / viewport_size).r * 256.0);
        color = (source_color | 0x8u) / 256.0;
    }
    else if (color_idx == 0x11u) {
        color = texture(buffer0, gl_FragCoord.xy / viewport_size).r;
    }
    else {
        color = (color_idx & 0xfu) / 256.0;
    }
}
"#;
