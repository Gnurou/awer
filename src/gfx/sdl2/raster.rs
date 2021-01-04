use std::convert::TryFrom;

use sdl2::{
    pixels::PixelFormat,
    render::{Canvas, Texture, TextureCreator},
    video::{Window, WindowContext},
};

use crate::gfx::{self, raster::RasterBackend, sdl2::SDL2Renderer, Color};

pub struct SDL2RasterRenderer {
    // Textures are owned by their texture creator, so we need to keep this
    // around.
    _texture_creator: TextureCreator<WindowContext>,
    texture: Texture,
    pixel_format: PixelFormat,
    bytes_per_pixel: usize,

    raster: RasterBackend,
}

impl SDL2RasterRenderer {
    pub fn new(canvas: &Canvas<Window>) -> Self {
        let texture_creator = canvas.texture_creator();
        let pixel_format_enum = texture_creator.default_pixel_format();
        let pixel_format = PixelFormat::try_from(pixel_format_enum).unwrap();
        let bytes_per_pixel = pixel_format_enum.byte_size_per_pixel();
        let texture = texture_creator
            .create_texture_streaming(
                None,
                gfx::SCREEN_RESOLUTION[0] as u32,
                gfx::SCREEN_RESOLUTION[1] as u32,
            )
            .unwrap();

        SDL2RasterRenderer {
            _texture_creator: texture_creator,
            texture,
            pixel_format,
            bytes_per_pixel,
            raster: RasterBackend::new(),
        }
    }
}

impl SDL2Renderer for SDL2RasterRenderer {
    fn render_game(&mut self) {
        // First generate the true color palette
        let palette = self.raster.get_palette();
        let palette_to_color = {
            let mut palette_to_color = [0u32; gfx::PALETTE_SIZE];
            for (i, color) in palette_to_color.iter_mut().enumerate() {
                let &Color { r, g, b } = palette.lookup(i as u8);
                *color = sdl2::pixels::Color::RGB(r, g, b).to_u32(&self.pixel_format);
            }
            palette_to_color
        };

        // Avoid borrowing self in the closure
        let framebuffer = self.raster.get_framebuffer();
        let bytes_per_pixel = self.bytes_per_pixel;

        let render_into_texture = |texture: &mut [u8], pitch: usize| {
            for (src_line, dst_line) in framebuffer
                .pixels()
                .chunks_exact(gfx::SCREEN_RESOLUTION[0])
                .zip(texture.chunks_exact_mut(pitch))
            {
                for (src_pix, dst_pix) in src_line
                    .iter()
                    .zip(dst_line.chunks_exact_mut(bytes_per_pixel))
                {
                    let color = palette_to_color[*src_pix as usize];
                    dst_pix.copy_from_slice(&color.to_ne_bytes()[0..bytes_per_pixel]);
                }
            }
        };

        self.texture.with_lock(None, render_into_texture).unwrap();
    }

    fn as_gfx(&self) -> &dyn crate::gfx::Backend {
        &self.raster
    }

    fn as_gfx_mut(&mut self) -> &mut dyn crate::gfx::Backend {
        &mut self.raster
    }

    fn get_rendered_texture(&self) -> &Texture {
        &self.texture
    }
}
