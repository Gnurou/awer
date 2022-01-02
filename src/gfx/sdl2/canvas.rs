use std::convert::TryFrom;

use sdl2::{
    pixels::PixelFormat,
    rect::Rect,
    render::{Canvas, Texture},
    video::Window,
    Sdl,
};

use anyhow::{anyhow, Result};

use crate::{
    gfx::{self, raster::RasterRenderer, sdl2::Sdl2Display, Color, Gfx, Palette},
    sys::Snapshotable,
};

use super::WINDOW_RESOLUTION;

/// A gfx module that renders the game using the CPU and SDL's Texture/Canvas methods.
///
/// This way of doing does not rely on any particular SDL driver, i.e. it does not require OpenGL or
/// any kind of hardware acceleration.
pub struct Sdl2CanvasGfx {
    canvas: Canvas<Window>,
    texture: Texture,
    pixel_format: PixelFormat,
    bytes_per_pixel: usize,

    raster: RasterRenderer,
}

impl Sdl2CanvasGfx {
    /// Create a new raster display, using the given SDL context. This takes
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
        let pixel_format = PixelFormat::try_from(pixel_format_enum).map_err(|s| anyhow!(s))?;
        let bytes_per_pixel = pixel_format_enum.byte_size_per_pixel();
        let texture = texture_creator.create_texture_streaming(
            None,
            gfx::SCREEN_RESOLUTION[0] as u32,
            gfx::SCREEN_RESOLUTION[1] as u32,
        )?;

        Ok(Sdl2CanvasGfx {
            canvas,
            texture,
            pixel_format,
            bytes_per_pixel,
            raster: RasterRenderer::new(),
        })
    }
}

impl gfx::IndexedRenderer for Sdl2CanvasGfx {
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

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster.blit_buffer(dst_page_id, buffer)
    }
}

impl gfx::Display for Sdl2CanvasGfx {
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette) {
        // Maps each palette index to the native color of the current display.
        let palette_to_color = {
            let mut palette_to_color = [0u32; gfx::PALETTE_SIZE];
            for (i, color) in palette_to_color.iter_mut().enumerate() {
                let &Color { r, g, b } = palette.lookup(i as u8);
                *color = sdl2::pixels::Color::RGB(r, g, b).to_u32(&self.pixel_format);
            }
            palette_to_color
        };

        // Avoid borrowing self in the closure
        let framebuffer = self.raster.get_buffer(page_id);
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
}

impl Snapshotable for Sdl2CanvasGfx {
    type State = <RasterRenderer as Snapshotable>::State;

    fn take_snapshot(&self) -> Self::State {
        self.raster.take_snapshot()
    }

    fn restore_snapshot(&mut self, snapshot: Self::State) -> bool {
        self.raster.restore_snapshot(snapshot)
    }
}

impl Gfx for Sdl2CanvasGfx {}

impl Sdl2Display for Sdl2CanvasGfx {
    fn blit_game(&mut self, dst: &Rect) {
        // Clear screen
        self.canvas
            .set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        self.canvas.clear();
        // Blit the game screen into the window viewport
        self.canvas.copy(&self.texture, None, Some(*dst)).unwrap();

        self.canvas.present();
    }

    fn window(&self) -> &Window {
        self.canvas.window()
    }

    fn as_gfx(&self) -> &dyn Gfx {
        self
    }

    fn as_gfx_mut(&mut self) -> &mut dyn Gfx {
        self
    }
}
