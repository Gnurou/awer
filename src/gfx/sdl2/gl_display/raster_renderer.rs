use anyhow::Result;
use gfx::SCREEN_RESOLUTION;

use crate::gfx::{self, gl::IndexedTexture, raster::RasterRenderer, Palette};

/// A renderer with which the game is rendered using the CPU at original resolution with a 16 colors
/// indexed palette.
pub struct Sdl2GlRasterRenderer {
    /// Regular CPU raster renderer where we will render the game.
    raster: RasterRenderer,

    /// Texture where the framebuffer from `raster` will be copied to serve as a source.
    framebuffer_texture: IndexedTexture,
}

impl Sdl2GlRasterRenderer {
    pub fn new() -> Result<Sdl2GlRasterRenderer> {
        Ok(Sdl2GlRasterRenderer {
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
