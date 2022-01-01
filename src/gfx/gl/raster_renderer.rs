use anyhow::Result;
use gfx::SCREEN_RESOLUTION;

use crate::{
    gfx::{self, gl::IndexedTexture, raster::RasterRenderer, Palette},
    sys::Snapshotable,
};

/// A renderer with which the game is rendered using the CPU at original resolution with a 16 colors
/// indexed palette.
pub struct GlRasterRenderer {
    /// Regular CPU raster renderer where we will render the game.
    raster: RasterRenderer,

    /// Texture where the framebuffer from `raster` will be copied to serve as a source.
    framebuffer_texture: IndexedTexture,
}

impl GlRasterRenderer {
    pub fn new() -> Result<GlRasterRenderer> {
        Ok(GlRasterRenderer {
            raster: RasterRenderer::new(),

            framebuffer_texture: IndexedTexture::new(SCREEN_RESOLUTION[0], SCREEN_RESOLUTION[1]),
        })
    }

    pub fn get_framebuffer_texture_and_palette(&mut self) -> (&IndexedTexture, &Palette) {
        self.framebuffer_texture
            .set_data(&*self.raster.get_framebuffer(), 0, 0);
        (&self.framebuffer_texture, self.raster.get_palette())
    }
}

impl gfx::Renderer for GlRasterRenderer {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.raster.set_palette(palette)
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster.fillvideopage(page_id, color_idx)
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster.copyvideopage(src_page_id, dst_page_id, vscroll)
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        color_idx: u8,
        zoom: u16,
        bb: (u8, u8),
        points: &[gfx::Point<u8>],
    ) {
        self.raster
            .fillpolygon(dst_page_id, pos, offset, color_idx, zoom, bb, points)
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.raster.draw_char(dst_page_id, pos, color_idx, c)
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.raster.blitframebuffer(page_id)
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster.blit_buffer(dst_page_id, buffer)
    }
}

impl Snapshotable for GlRasterRenderer {
    type State = <RasterRenderer as Snapshotable>::State;

    fn take_snapshot(&self) -> Self::State {
        self.raster.take_snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: Self::State) -> bool {
        self.raster.restore_snapshot(snapshot)
    }
}
