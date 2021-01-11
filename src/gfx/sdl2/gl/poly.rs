use poly_renderer::{PolyDrawCommand, PolyRenderer};
use sdl2::rect::Rect;

use crate::gfx::{self, gl::*, polygon::Polygon, Palette, Point};
use anyhow::Result;

pub use crate::gfx::gl::poly_renderer::RenderingMode;

pub struct SDL2GLPolyRenderer {
    rendering_mode: RenderingMode,

    draw_commands: [Vec<PolyDrawCommand>; 4],
    framebuffer_index: usize,

    candidate_palette: Palette,
    current_palette: Palette,

    render_textures: [IndexedTexture; 4],
    poly_renderer: PolyRenderer,
}

impl SDL2GLPolyRenderer {
    pub fn new(rendering_mode: RenderingMode) -> Result<SDL2GLPolyRenderer> {
        Ok(SDL2GLPolyRenderer {
            rendering_mode,

            draw_commands: Default::default(),
            framebuffer_index: 0,
            candidate_palette: Default::default(),
            current_palette: Default::default(),

            render_textures: [
                IndexedTexture::new(1280, 960),
                IndexedTexture::new(1280, 960),
                IndexedTexture::new(1280, 960),
                IndexedTexture::new(1280, 960),
            ],
            poly_renderer: PolyRenderer::new()?,
        })
    }

    pub fn blit(&mut self, dst: &Rect) {
        self.poly_renderer.render_into(
            &self.draw_commands[self.framebuffer_index],
            &self.render_textures[self.framebuffer_index],
            self.rendering_mode,
        );

        unsafe {
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, self.poly_renderer.fbo());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
            gl::BlitFramebuffer(
                0,
                0,
                1280,
                960,
                dst.x(),
                dst.y(),
                dst.x() + dst.width() as i32,
                dst.y() + dst.height() as i32,
                gl::COLOR_BUFFER_BIT,
                gl::LINEAR,
            );
        }
    }
}

impl gfx::Backend for SDL2GLPolyRenderer {
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
        commands.push(PolyDrawCommand::new(
            Polygon::new(
                (w, h),
                vec![
                    Point { x: 0, y: 0 },
                    Point { x: w, y: 0 },
                    Point { x: w, y: h },
                    Point { x: 0, y: h },
                ],
            ),
            w as i16 / 2,
            h as i16 / 2,
            color_idx,
        ));
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, _vscroll: i16) {
        let src_polys = self.draw_commands[src_page_id].clone();
        self.draw_commands[dst_page_id] = src_polys;
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &Polygon,
    ) {
        let command = &mut self.draw_commands[dst_page_id];
        command.push(PolyDrawCommand::new(polygon.clone(), x, y, color_idx));
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
        self.current_palette = self.candidate_palette.clone();
    }

    fn blit_buffer(&mut self, _dst_page_id: usize, _buffer: &[u8]) {}
}
