use std::any::Any;

use gfx::raster::IndexedImage;
use gl::types::{GLint, GLuint};
use sdl2::rect::Rect;

use crate::gfx::{
    self,
    gl::{
        bitmap_renderer::BitmapRenderer, font_renderer::FontRenderer,
        indexed_frame_renderer::IndexedFrameRenderer, poly_renderer::PolyRenderer,
        renderer::CurrentRenderer, IndexedTexture, Viewport,
    },
    polygon::Polygon,
    Palette, Point,
};
use anyhow::Result;

pub use crate::gfx::gl::poly_renderer::RenderingMode;

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

pub struct Sdl2GlPolyRenderer {
    rendering_mode: RenderingMode,

    draw_commands: [Vec<DrawCommand>; 4],
    framebuffer_index: usize,

    candidate_palette: Palette,
    current_palette: Palette,

    target_fbo: GLuint,

    render_texture_buffer0: IndexedTexture,
    render_texture_framebuffer: IndexedTexture,

    poly_renderer: PolyRenderer,
    bitmap_renderer: BitmapRenderer,
    font_renderer: FontRenderer,
    frame_renderer: IndexedFrameRenderer,
}

struct State {
    draw_commands: [Vec<DrawCommand>; 4],
    framebuffer_index: usize,
    candidate_palette: Palette,
    current_palette: Palette,
}

impl Drop for Sdl2GlPolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.target_fbo);
        }
    }
}

impl Sdl2GlPolyRenderer {
    pub fn new(
        rendering_mode: RenderingMode,
        width: usize,
        height: usize,
    ) -> Result<Sdl2GlPolyRenderer> {
        let mut target_fbo = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut target_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, target_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }

        Ok(Sdl2GlPolyRenderer {
            rendering_mode,

            draw_commands: Default::default(),
            framebuffer_index: 0,
            candidate_palette: Default::default(),
            current_palette: Default::default(),

            target_fbo,

            render_texture_buffer0: IndexedTexture::new(width, height),
            render_texture_framebuffer: IndexedTexture::new(width, height),

            poly_renderer: PolyRenderer::new()?,
            bitmap_renderer: BitmapRenderer::new()?,
            font_renderer: FontRenderer::new()?,
            frame_renderer: IndexedFrameRenderer::new()?,
        })
    }

    pub fn set_rendering_mode(&mut self, rendering_mode: RenderingMode) {
        self.rendering_mode = rendering_mode;
    }

    pub fn resize_render_textures(&mut self, width: usize, height: usize) {
        self.render_texture_buffer0 = IndexedTexture::new(width, height);
        self.render_texture_framebuffer = IndexedTexture::new(width, height);
        self.redraw();
    }

    pub fn blit(&mut self, dst: &Rect) {
        self.frame_renderer.render_into(
            &self.render_texture_framebuffer,
            &self.current_palette,
            0,
            &Viewport {
                x: dst.x(),
                y: dst.y(),
                width: dst.width() as i32,
                height: dst.height() as i32,
            },
        );
    }

    fn run_command_list<'a, C: IntoIterator<Item = &'a DrawCommand>>(
        &self,
        draw_commands: C,
        rendering_mode: RenderingMode,
    ) {
        let mut current_renderer = CurrentRenderer::new();

        for command in draw_commands {
            match command {
                DrawCommand::Poly(poly) => {
                    current_renderer.use_poly(
                        &self.poly_renderer,
                        &self.render_texture_framebuffer,
                        &self.render_texture_buffer0,
                    );
                    self.poly_renderer.draw_poly(
                        &poly.poly,
                        poly.pos,
                        poly.offset,
                        poly.zoom,
                        poly.color,
                        rendering_mode,
                    );
                }
                DrawCommand::BlitBuffer(buffer) => {
                    current_renderer.use_bitmap(
                        &self.bitmap_renderer,
                        &self.render_texture_framebuffer,
                        &self.render_texture_buffer0,
                    );
                    self.bitmap_renderer.draw_bitmap(&buffer.image);
                }
                DrawCommand::Char(c) => {
                    current_renderer.use_font(
                        &self.font_renderer,
                        &self.render_texture_framebuffer,
                        &self.render_texture_buffer0,
                    );
                    self.font_renderer.draw_char(c.pos, c.color, c.c);
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
        self.run_command_list(&self.draw_commands[0], self.rendering_mode);

        // Then render the framebuffer, which can now use buffer0 as a source
        // texture.
        self.set_render_target(&self.render_texture_framebuffer);
        self.run_command_list(
            &self.draw_commands[self.framebuffer_index],
            self.rendering_mode,
        );

        // TODO move into proper method?
        unsafe {
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }
    }
}

impl gfx::Backend for Sdl2GlPolyRenderer {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.candidate_palette = {
            let mut p: Palette = Default::default();
            p.set(palette);
            p
        }
    }

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

    fn blitframebuffer(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
        self.current_palette = self.candidate_palette.clone();

        self.redraw();
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        let mut image: IndexedImage = Default::default();
        image
            .set_content(buffer)
            .unwrap_or_else(|e| log::error!("blit_buffer failed: {}", e));

        self.draw_commands[dst_page_id].clear();
        self.draw_commands[dst_page_id].push(DrawCommand::BlitBuffer(image.into()));
    }

    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(State {
            draw_commands: self.draw_commands.clone(),
            framebuffer_index: self.framebuffer_index,
            candidate_palette: self.candidate_palette.clone(),
            current_palette: self.current_palette.clone(),
        })
    }

    fn set_snapshot(&mut self, snapshot: Box<dyn Any>) {
        if let Ok(state) = snapshot.downcast::<State>() {
            self.draw_commands = state.draw_commands;
            self.framebuffer_index = state.framebuffer_index;
            self.candidate_palette = state.candidate_palette;
            self.current_palette = state.current_palette;
        } else {
            log::error!("Attempting to restore invalid gfx snapshot, ignoring");
        }

        self.redraw();
    }
}
