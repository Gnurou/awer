use anyhow::Result;
use gfx::SCREEN_RESOLUTION;
use sdl2::rect::Rect;

use crate::gfx::{
    self,
    gl::{indexed_frame_renderer::*, IndexedTexture, Viewport},
    raster::RasterRenderer,
};

pub struct Sdl2GlRasterRenderer {
    raster: RasterRenderer,

    framebuffer_texture: IndexedTexture,
    framebuffer_renderer: IndexedFrameRenderer,
}

impl Sdl2GlRasterRenderer {
    pub fn new() -> Result<Sdl2GlRasterRenderer> {
        Ok(Sdl2GlRasterRenderer {
            raster: RasterRenderer::new(),

            framebuffer_texture: IndexedTexture::new(SCREEN_RESOLUTION[0], SCREEN_RESOLUTION[1]),
            framebuffer_renderer: IndexedFrameRenderer::new()?,
        })
    }

    pub fn blit(&mut self, dst: &Rect) {
        self.framebuffer_texture
            .set_data(&*self.raster.get_framebuffer(), 0, 0);
        self.framebuffer_renderer.render_into(
            &self.framebuffer_texture,
            self.raster.get_palette(),
            0,
            &Viewport {
                x: dst.x(),
                y: dst.y(),
                width: dst.width() as i32,
                height: dst.height() as i32,
            },
        );
    }
}

impl AsRef<dyn gfx::Renderer> for Sdl2GlRasterRenderer {
    fn as_ref(&self) -> &(dyn gfx::Renderer + 'static) {
        &self.raster
    }
}

impl AsMut<dyn gfx::Renderer> for Sdl2GlRasterRenderer {
    fn as_mut(&mut self) -> &mut (dyn gfx::Renderer + 'static) {
        &mut self.raster
    }
}
