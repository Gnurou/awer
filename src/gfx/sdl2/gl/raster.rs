use std::any::Any;

use anyhow::Result;
use gfx::SCREEN_RESOLUTION;
use sdl2::rect::Rect;

use crate::gfx::{
    self,
    gl::{indexed_frame_renderer::*, IndexedTexture, Viewport},
    raster::{IndexedImage, RasterBackend},
    Palette,
};

pub struct SDL2GLRasterRenderer {
    raster: RasterBackend,
    current_framebuffer: IndexedImage,
    current_palette: Palette,

    framebuffer_texture: IndexedTexture,
    framebuffer_renderer: IndexedFrameRenderer,
}

struct State {
    raster: Box<dyn Any>,
    current_framebuffer: IndexedImage,
    current_palette: Palette,
}

impl SDL2GLRasterRenderer {
    pub fn new() -> Result<SDL2GLRasterRenderer> {
        Ok(SDL2GLRasterRenderer {
            raster: RasterBackend::new(),
            current_framebuffer: Default::default(),
            current_palette: Default::default(),

            framebuffer_texture: IndexedTexture::new(SCREEN_RESOLUTION[0], SCREEN_RESOLUTION[1]),
            framebuffer_renderer: IndexedFrameRenderer::new()?,
        })
    }

    pub fn blit(&mut self, dst: &Rect) {
        self.framebuffer_texture
            .set_data(&self.current_framebuffer, 0, 0);
        self.framebuffer_renderer.render_into(
            &self.framebuffer_texture,
            &self.current_palette,
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

    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(State {
            raster: self.raster.get_snapshot(),
            current_framebuffer: self.current_framebuffer.clone(),
            current_palette: self.current_palette.clone(),
        })
    }

    fn set_snapshot(&mut self, snapshot: Box<dyn Any>) {
        if let Ok(state) = snapshot.downcast::<State>() {
            self.raster.set_snapshot(state.raster);
            self.current_framebuffer = state.current_framebuffer;
            self.current_palette = state.current_palette;
        } else {
            eprintln!("Attempting to restore invalid gfx snapshot, ignoring");
        }
    }
}
