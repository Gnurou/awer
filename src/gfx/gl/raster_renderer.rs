use anyhow::Result;
use gfx::SCREEN_RESOLUTION;

use crate::gfx::{self, gl::IndexedTexture, raster::RasterRenderer};

use super::GlGameTexture;

// A simple proxy struct for a `RasterRenderer` with the ability to obtain a texture from any of the
// rendered buffers that can be used with OpenGL.
pub struct GlRasterRenderer {
    /// Regular CPU raster renderer where we will render the game.
    raster: RasterRenderer,
    /// Texture into which any buffer from `raster` can be copied in order to serve as a source.
    framebuffer_texture: IndexedTexture,
}

impl GlRasterRenderer {
    pub fn new() -> Result<GlRasterRenderer> {
        Ok(GlRasterRenderer {
            raster: RasterRenderer::new(),

            framebuffer_texture: IndexedTexture::new(SCREEN_RESOLUTION[0], SCREEN_RESOLUTION[1]),
        })
    }
}

impl AsRef<IndexedTexture> for GlRasterRenderer {
    fn as_ref(&self) -> &IndexedTexture {
        &self.framebuffer_texture
    }
}

impl GlGameTexture for GlRasterRenderer {
    fn update_texture(&mut self, page_id: usize) {
        self.framebuffer_texture
            .set_data(&*self.raster.get_buffer(page_id), 0, 0);
    }
}

impl AsRef<RasterRenderer> for GlRasterRenderer {
    fn as_ref(&self) -> &RasterRenderer {
        &self.raster
    }
}

impl AsMut<RasterRenderer> for GlRasterRenderer {
    fn as_mut(&mut self) -> &mut RasterRenderer {
        &mut self.raster
    }
}
