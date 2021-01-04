use std::convert::TryFrom;

use sdl2::{
    pixels::PixelFormat,
    rect::Rect,
    render::{Canvas, Texture, TextureCreator},
    video::{Window, WindowContext},
    Sdl,
};

use anyhow::{anyhow, Result};

use crate::gfx::{self, raster::RasterBackend, sdl2::SDL2Renderer, Color};

use super::WINDOW_RESOLUTION;

/// A renderer that renders the game into a SDL Texture, using only Texture
/// and Canvas methods. The rendered texture is then stretched to fit the
/// display when rendered.
///
/// This way of doing does not rely on any particular SDL driver, i.e. it does
/// not require OpenGL or even any kind of hardware acceleration.
pub struct SDL2RasterRenderer {
    canvas: Canvas<Window>,
    // Textures are owned by their texture creator, so we need to keep this
    // around.
    _texture_creator: TextureCreator<WindowContext>,
    texture: Texture,
    pixel_format: PixelFormat,
    bytes_per_pixel: usize,

    raster: RasterBackend,
}

impl SDL2RasterRenderer {
    /// Create a new raster renderer, using the given SDL context. This takes
    /// care of creating the window, canvas, and everything we need to draw.
    pub fn new(sdl_context: &Sdl) -> Result<Self> {
        let sdl_video = sdl_context.video().map_err(|s| anyhow!(s))?;

        let window = sdl_video
            .window("Another World", WINDOW_RESOLUTION[0], WINDOW_RESOLUTION[1])
            .resizable()
            .allow_highdpi()
            .build()?;

        let canvas = window.into_canvas().build()?;

        let texture_creator = canvas.texture_creator();
        let pixel_format_enum = texture_creator.default_pixel_format();
        let pixel_format = PixelFormat::try_from(pixel_format_enum).unwrap();
        let bytes_per_pixel = pixel_format_enum.byte_size_per_pixel();
        let texture = texture_creator.create_texture_streaming(
            None,
            gfx::SCREEN_RESOLUTION[0] as u32,
            gfx::SCREEN_RESOLUTION[1] as u32,
        )?;

        Ok(SDL2RasterRenderer {
            canvas,
            _texture_creator: texture_creator,
            texture,
            pixel_format,
            bytes_per_pixel,
            raster: RasterBackend::new(),
        })
    }
}

impl SDL2Renderer for SDL2RasterRenderer {
    fn viewport(&self) -> Rect {
        self.canvas.viewport()
    }

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

    fn blit_game(&mut self, dst: Rect) {
        // Clear screen
        self.canvas
            .set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        self.canvas.clear();
        // Blit the game screen into the window viewport
        self.canvas.copy(&self.texture, None, Some(dst)).unwrap();
    }

    fn present(&mut self) {
        self.canvas.present();
    }

    fn as_gfx(&self) -> &dyn crate::gfx::Backend {
        &self.raster
    }

    fn as_gfx_mut(&mut self) -> &mut dyn crate::gfx::Backend {
        &mut self.raster
    }
}
