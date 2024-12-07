use std::ops::Deref;
use std::ops::DerefMut;

use anyhow::Result;
use gfx::SCREEN_RESOLUTION;

use crate::gfx::gl3::IndexedTexture;
use crate::gfx::sw::RasterGameRenderer;
use crate::gfx::{self};

use super::GlRenderer;

/// A simple proxy struct for a `RasterRenderer` with the ability to obtain a texture from any of
/// the rendered buffers that can be used with OpenGL.
pub struct GlRasterRenderer {
    /// Regular CPU raster renderer where we will render the game.
    raster: RasterGameRenderer,
    /// Texture into which any buffer from `raster` can be copied in order to serve as a source.
    framebuffer_texture: IndexedTexture,
}

impl GlRasterRenderer {
    pub fn new() -> Result<GlRasterRenderer> {
        Ok(GlRasterRenderer {
            raster: RasterGameRenderer::new(),
            framebuffer_texture: IndexedTexture::new(SCREEN_RESOLUTION[0], SCREEN_RESOLUTION[1]),
        })
    }
}

impl AsRef<IndexedTexture> for GlRasterRenderer {
    fn as_ref(&self) -> &IndexedTexture {
        &self.framebuffer_texture
    }
}

impl GlRenderer for GlRasterRenderer {
    fn update_texture(&mut self, page_id: usize) {
        self.framebuffer_texture
            .set_data(&*self.raster.get_buffer(page_id), 0, 0);
    }
}

impl Deref for GlRasterRenderer {
    type Target = RasterGameRenderer;

    fn deref(&self) -> &Self::Target {
        &self.raster
    }
}

impl DerefMut for GlRasterRenderer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raster
    }
}
