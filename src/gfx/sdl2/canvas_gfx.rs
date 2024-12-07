//! A gfx module that renders the game using the CPU and SDL's Texture/Canvas methods.
//!
//! This way of doing does not rely on any particular SDL driver, i.e. it does not require OpenGL or
//! any kind of hardware acceleration.

use std::any::Any;
use std::convert::TryFrom;

use sdl2::pixels::PixelFormat;
use sdl2::rect::Rect;
use sdl2::render::Canvas;
use sdl2::render::Texture;
use sdl2::video::Window;
use sdl2::Sdl;

use anyhow::anyhow;
use anyhow::Result;
use tracing::trace_span;

use crate::gfx;
use crate::gfx::raster::RasterGameRenderer;
use crate::gfx::sdl2::Sdl2Gfx;
use crate::gfx::Color;
use crate::gfx::Display;
use crate::gfx::Gfx;
use crate::gfx::Palette;
use crate::scenes::InitForScene;
use crate::sys::Snapshotable;

use super::WINDOW_RESOLUTION;

/// Pure software renderer and display for SDL2. [`gfx::IndexedRenderer`] is just implemented by
/// proxying `raster`, and the other members are used to display the current game buffer on the
/// screen.
pub struct Sdl2CanvasGfx {
    /// Software rasterizer from which we will get the game buffers to display.
    raster: RasterGameRenderer,

    current_framebuffer: usize,
    current_palette: Palette,

    /// Canvas used to show the current game buffer on the actual display.
    canvas: Canvas<Window>,
    /// Texture onto which the game buffer to be displayed is rendered.
    texture: Texture,
    /// Native pixel format of the display.
    pixel_format: PixelFormat,
    /// Number of bytes per pixel, used when rendering the current buffer to the native pixel
    /// format.
    bytes_per_pixel: usize,
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
            current_framebuffer: 0,
            current_palette: Default::default(),
            texture,
            pixel_format,
            bytes_per_pixel,
            raster: RasterGameRenderer::new(),
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

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.raster.draw_char(dst_page_id, pos, color_idx, c)
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster.blit_buffer(dst_page_id, buffer)
    }

    fn draw_polygons(
        &mut self,
        segment: gfx::PolySegment,
        start_offset: u16,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    ) {
        self.raster
            .draw_polygons(segment, start_offset, dst_page_id, pos, offset, zoom)
    }
}

impl gfx::Display for Sdl2CanvasGfx {
    #[tracing::instrument(level = "trace", skip(self, palette))]
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette) {
        // Keep information useful for snapshotting...
        self.current_framebuffer = page_id;
        self.current_palette = palette.clone();

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
        let bytes_per_pixel = self.bytes_per_pixel;

        let render_into_texture = |texture: &mut [u8], pitch: usize| {
            for (src_line, dst_line) in self
                .raster
                .get_buffer(page_id)
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

        trace_span!("render_into_texture")
            .in_scope(|| self.texture.with_lock(None, render_into_texture).unwrap());
    }
}

#[derive(Clone)]
struct Sdl2CanvasGfxSnapshot {
    raster: RasterGameRenderer,
    current_framebuffer: usize,
    current_palette: Palette,
}

impl Snapshotable for Sdl2CanvasGfx {
    type State = Box<dyn Any>;

    fn take_snapshot(&self) -> Self::State {
        Box::new(Sdl2CanvasGfxSnapshot {
            raster: self.raster.clone(),
            current_framebuffer: self.current_framebuffer,
            current_palette: self.current_palette.clone(),
        })
    }

    fn restore_snapshot(&mut self, snapshot: &Self::State) -> bool {
        if let Some(snapshot) = snapshot.downcast_ref::<Sdl2CanvasGfxSnapshot>() {
            self.raster = snapshot.raster.clone();
            self.blitframebuffer(snapshot.current_framebuffer, &snapshot.current_palette);
            true
        } else {
            false
        }
    }
}

impl InitForScene for Sdl2CanvasGfx {
    #[tracing::instrument(skip(self, resman))]
    fn init_from_scene(
        &mut self,
        resman: &crate::res::ResourceManager,
        scene: &crate::scenes::Scene,
    ) -> std::io::Result<()> {
        self.raster.init_from_scene(resman, scene)
    }
}

impl Gfx for Sdl2CanvasGfx {}

impl Sdl2Gfx for Sdl2CanvasGfx {
    #[tracing::instrument(skip(self))]
    fn show_game_framebuffer(&mut self, dst: &Rect) {
        // Clear screen
        self.canvas
            .set_draw_color(sdl2::pixels::Color::RGB(0, 0, 0));
        self.canvas.clear();
        // Blit the game screen into the window viewport
        self.canvas.copy(&self.texture, None, Some(*dst)).unwrap();
    }

    #[tracing::instrument(skip(self))]
    fn present(&mut self) {
        self.canvas.present();
    }

    fn window(&self) -> &Window {
        self.canvas.window()
    }
}
