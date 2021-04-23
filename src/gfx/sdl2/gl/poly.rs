use std::any::Any;

use gfx::raster::IndexedImage;
use sdl2::rect::Rect;

use crate::gfx::{
    self,
    gl::{
        indexed_frame_renderer::IndexedFrameRenderer,
        poly_renderer::{DrawCommand, PolyDrawCommand, PolyRenderer},
        IndexedTexture, Viewport,
    },
    polygon::Polygon,
    Palette, Point,
};
use anyhow::Result;

pub use crate::gfx::gl::poly_renderer::RenderingMode;

pub struct Sdl2GlPolyRenderer {
    rendering_mode: RenderingMode,

    draw_commands: [Vec<DrawCommand>; 4],
    framebuffer_index: usize,

    candidate_palette: Palette,
    current_palette: Palette,

    render_texture_buffer0: IndexedTexture,
    render_texture_framebuffer: IndexedTexture,
    poly_renderer: PolyRenderer,
    frame_renderer: IndexedFrameRenderer,
}

struct State {
    draw_commands: [Vec<DrawCommand>; 4],
    framebuffer_index: usize,
    candidate_palette: Palette,
    current_palette: Palette,
}

const TEXTURE_SIZE: (usize, usize) = (1280, 960);

impl Sdl2GlPolyRenderer {
    pub fn new(rendering_mode: RenderingMode) -> Result<Sdl2GlPolyRenderer> {
        Ok(Sdl2GlPolyRenderer {
            rendering_mode,

            draw_commands: Default::default(),
            framebuffer_index: 0,
            candidate_palette: Default::default(),
            current_palette: Default::default(),

            render_texture_buffer0: IndexedTexture::new(TEXTURE_SIZE.0, TEXTURE_SIZE.1),
            render_texture_framebuffer: IndexedTexture::new(TEXTURE_SIZE.0, TEXTURE_SIZE.1),
            poly_renderer: PolyRenderer::new()?,
            frame_renderer: IndexedFrameRenderer::new()?,
        })
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

    fn redraw(&mut self) {
        // First render buffer 0, since it may be needed to render the final
        // buffer.
        self.poly_renderer.render_into(
            &self.draw_commands[0],
            &self.render_texture_buffer0,
            &self.render_texture_buffer0,
            self.rendering_mode,
        );

        self.poly_renderer.render_into(
            &self.draw_commands[self.framebuffer_index],
            &self.render_texture_framebuffer,
            &self.render_texture_buffer0,
            self.rendering_mode,
        );
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

        let w = gfx::SCREEN_RESOLUTION[0] as u16;
        let h = gfx::SCREEN_RESOLUTION[1] as u16;
        commands.push(DrawCommand::Poly(PolyDrawCommand::new(
            Polygon::new(
                (w, h),
                vec![
                    Point { x: 0, y: 0 },
                    Point { x: w, y: 0 },
                    Point { x: w, y: h },
                    Point { x: 0, y: h },
                ],
            ),
            (w as i16 / 2, h as i16 / 2),
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
        color_idx: u8,
        zoom: u16,
        polygon: &Polygon,
    ) {
        let command = &mut self.draw_commands[dst_page_id];
        command.push(DrawCommand::Poly(PolyDrawCommand::new(
            polygon.clone(),
            pos,
            zoom,
            color_idx,
        )));
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
            .unwrap_or_else(|e| eprintln!("blit_buffer failed: {}", e));

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
            eprintln!("Attempting to restore invalid gfx snapshot, ignoring");
        }

        self.redraw();
    }
}
