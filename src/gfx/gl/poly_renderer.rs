mod programs;

use gl::types::{GLint, GLuint};

// TODO not elegant, but needed for now.
pub use programs::PolyRenderingMode;

use crate::{
    gfx::{self, gl::IndexedTexture, polygon::Polygon, raster::IndexedImage, Point},
    sys::Snapshotable,
};
use anyhow::Result;

use self::programs::*;

use super::GlRenderer;

/// Draw command for a polygon, requesting it to be drawn at coordinates (`x`,
/// `y`) and with color `color`.
#[derive(Clone)]
struct PolyDrawCommand {
    poly: Polygon,
    pos: (i16, i16),
    offset: (i16, i16),
    zoom: u16,
    color: u8,
}

impl PolyDrawCommand {
    pub fn new(poly: Polygon, pos: (i16, i16), offset: (i16, i16), zoom: u16, color: u8) -> Self {
        Self {
            poly,
            pos,
            offset,
            zoom,
            color,
        }
    }
}

#[derive(Clone)]
struct BlitBufferCommand {
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
struct CharDrawCommand {
    pos: (i16, i16),
    color: u8,
    c: u8,
}

impl CharDrawCommand {
    pub fn new(pos: (i16, i16), color: u8, c: u8) -> Self {
        Self { pos, color, c }
    }
}

#[derive(Clone)]
enum DrawCommand {
    Poly(PolyDrawCommand),
    BlitBuffer(BlitBufferCommand),
    Char(CharDrawCommand),
}

/// A renderer that uses the GPU to render the game into a 16 colors indexed bufffer of any size.
pub struct GlPolyRenderer {
    rendering_mode: PolyRenderingMode,

    draw_commands: [Vec<DrawCommand>; 4],
    framebuffer_index: usize,

    target_fbo: GLuint,

    render_texture_buffer0: IndexedTexture,
    render_texture_framebuffer: IndexedTexture,

    renderers: Programs,
}

impl Drop for GlPolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.target_fbo);
        }
    }
}

impl GlPolyRenderer {
    pub fn new(
        rendering_mode: PolyRenderingMode,
        width: usize,
        height: usize,
    ) -> Result<GlPolyRenderer> {
        let mut target_fbo = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut target_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, target_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }

        Ok(GlPolyRenderer {
            rendering_mode,

            draw_commands: Default::default(),
            framebuffer_index: 0,

            target_fbo,

            render_texture_buffer0: IndexedTexture::new(width, height),
            render_texture_framebuffer: IndexedTexture::new(width, height),

            renderers: Programs::new(
                PolyRenderer::new()?,
                BitmapRenderer::new()?,
                FontRenderer::new()?,
            ),
        })
    }

    pub fn set_rendering_mode(&mut self, rendering_mode: PolyRenderingMode) {
        self.rendering_mode = rendering_mode;
    }

    pub fn resize_render_textures(&mut self, width: usize, height: usize) {
        self.render_texture_buffer0 = IndexedTexture::new(width, height);
        self.render_texture_framebuffer = IndexedTexture::new(width, height);
        self.redraw();
    }

    fn run_command_list(&mut self, commands_index: usize, rendering_mode: PolyRenderingMode) {
        let draw_commands = &self.draw_commands[commands_index];
        let mut draw_runner = self.renderers.start_drawing(
            &self.render_texture_framebuffer,
            &self.render_texture_buffer0,
        );
        for command in draw_commands {
            match command {
                DrawCommand::Poly(poly) => {
                    draw_runner.draw_poly(
                        &poly.poly,
                        poly.pos,
                        poly.offset,
                        poly.zoom,
                        poly.color,
                        rendering_mode,
                    );
                }
                DrawCommand::BlitBuffer(buffer) => {
                    draw_runner.draw_bitmap(&buffer.image);
                }
                DrawCommand::Char(c) => {
                    draw_runner.draw_char(c.pos, c.color, c.c);
                }
            }
        }
    }

    fn set_render_target(&self, target_texture: &IndexedTexture) {
        unsafe {
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
        }
    }

    pub fn redraw(&mut self) {
        unsafe {
            let dimensions = self.render_texture_framebuffer.dimensions();
            gl::Viewport(0, 0, dimensions.0 as GLint, dimensions.1 as GLint);
        }

        // First render buffer 0, since it may be needed to render the final
        // buffer.
        self.set_render_target(&self.render_texture_buffer0);
        self.run_command_list(0, self.rendering_mode);

        // Then render the framebuffer, which can now use buffer0 as a source
        // texture.
        self.set_render_target(&self.render_texture_framebuffer);
        self.run_command_list(self.framebuffer_index, self.rendering_mode);

        // TODO move into proper method?
        unsafe {
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }
    }
}

impl gfx::IndexedRenderer for GlPolyRenderer {
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        let commands = &mut self.draw_commands[page_id];
        commands.clear();

        let w = gfx::SCREEN_RESOLUTION[0] as i16;
        let h = gfx::SCREEN_RESOLUTION[1] as i16;
        commands.push(DrawCommand::Poly(PolyDrawCommand::new(
            Polygon::new(
                (w as u16, h as u16),
                vec![
                    Point { x: 0, y: 0 },
                    Point { x: w, y: 0 },
                    Point { x: w, y: h },
                    Point { x: 0, y: h },
                ],
            ),
            (w / 2, h / 2),
            (0, 0),
            64,
            color_idx,
        )));
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, _vscroll: i16) {
        let src_polys = self.draw_commands[src_page_id].clone();
        self.draw_commands[dst_page_id] = src_polys;
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        color_idx: u8,
        zoom: u16,
        bb: (u8, u8),
        points: &[Point<u8>],
    ) {
        let command = &mut self.draw_commands[dst_page_id];
        command.push(DrawCommand::Poly(PolyDrawCommand::new(
            Polygon::new(
                (bb.0 as u16, bb.1 as u16),
                points
                    .iter()
                    .map(|p| Point::new(p.x as i16, p.y as i16))
                    .collect(),
            ),
            pos,
            offset,
            zoom,
            color_idx,
        )));
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color: u8, c: u8) {
        let command_queue = &mut self.draw_commands[dst_page_id];
        command_queue.push(DrawCommand::Char(CharDrawCommand::new(pos, color, c)));
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        let mut image: IndexedImage = Default::default();
        image
            .set_content(buffer)
            .unwrap_or_else(|e| log::error!("blit_buffer failed: {}", e));

        self.draw_commands[dst_page_id].clear();
        self.draw_commands[dst_page_id].push(DrawCommand::BlitBuffer(image.into()));
    }
}

#[derive(Clone)]
pub struct GlPolyRendererSnapshot {
    draw_commands: [Vec<DrawCommand>; 4],
    framebuffer_index: usize,
}

impl Snapshotable for GlPolyRenderer {
    type State = GlPolyRendererSnapshot;

    fn take_snapshot(&self) -> Self::State {
        GlPolyRendererSnapshot {
            draw_commands: self.draw_commands.clone(),
            framebuffer_index: self.framebuffer_index,
        }
    }

    fn restore_snapshot(&mut self, snapshot: &Self::State) -> bool {
        self.draw_commands = snapshot.draw_commands.clone();
        self.framebuffer_index = snapshot.framebuffer_index;
        true
    }
}

impl AsRef<IndexedTexture> for GlPolyRenderer {
    fn as_ref(&self) -> &IndexedTexture {
        &self.render_texture_framebuffer
    }
}

impl GlRenderer for GlPolyRenderer {
    fn update_texture(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
        self.redraw();
    }
}
