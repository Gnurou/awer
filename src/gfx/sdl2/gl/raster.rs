use anyhow::Result;
use gfx::SCREEN_RESOLUTION;

use crate::gfx::{
    self,
    gl::{indexed_frame_renderer::*, IndexedTexture},
    raster::{IndexedImage, RasterBackend},
    Palette,
};

pub struct SDL2GLRasterRenderer {
    raster: RasterBackend,
    current_framebuffer: IndexedImage,
    framebuffer_texture: IndexedTexture,
    framebuffer_renderer: IndexedFrameRenderer,
    current_palette: Palette,
}

impl SDL2GLRasterRenderer {
    pub fn new() -> Result<SDL2GLRasterRenderer> {
        Ok(SDL2GLRasterRenderer {
            raster: RasterBackend::new(),
            current_framebuffer: Default::default(),
            framebuffer_texture: IndexedTexture::new(SCREEN_RESOLUTION[0], SCREEN_RESOLUTION[1]),
            framebuffer_renderer: IndexedFrameRenderer::new()?,
            current_palette: Default::default(),
        })
    }

    pub fn blit(&mut self) {
        self.framebuffer_texture
            .set_data(&self.current_framebuffer, 0, 0);
        self.framebuffer_renderer
            .render_into(0, &self.framebuffer_texture, &self.current_palette);
    }
}

impl gfx::Backend for SDL2GLRasterRenderer {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.raster.set_palette(palette);
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster.fillvideopage(page_id, color_idx);
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster.copyvideopage(src_page_id, dst_page_id, vscroll);
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        x: i16,
        y: i16,
        color_idx: u8,
        polygon: &gfx::polygon::Polygon,
    ) {
        self.raster
            .fillpolygon(dst_page_id, x, y, color_idx, polygon);
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.raster.blitframebuffer(page_id);

        // Copy the palette and rendered image that we will pass as uniforms
        // to our shader.
        self.current_framebuffer = self.raster.get_framebuffer().clone();
        self.current_palette = self.raster.get_palette().clone();
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster.blit_buffer(dst_page_id, buffer)
    }
}
