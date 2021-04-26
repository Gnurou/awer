mod poly;
mod raster;

use std::any::Any;

use sdl2::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    rect::Rect,
    video::{GLContext, GLProfile, Window},
    Sdl,
};

use anyhow::{anyhow, Result};

use crate::gfx::{self, polygon::Polygon};

use super::{Sdl2Renderer, WINDOW_RESOLUTION};

#[derive(Clone, Copy)]
pub enum RenderingMode {
    Raster,
    Poly,
    Line,
}

/// A GL-based renderer for SDL. Contrary to what the name implies, it still
/// renders using rasterization into a 320x200 texture that is scaled. Howver,
/// it does it much more efficiently than the SDL raster renderer, using a
/// shader that takes the 320x200, 4bpp scene and corresponding palette and
/// infers the actual color of each pixel on the GPU.
///
/// It also operated without using the SDL Canvas API, meaning it can safely be
/// used along with other GL libraries, like ImGUI.
///
/// In the future it should also be able to render a DrawList into polygons at
/// any resolution - ideally we would be able to switch modes on the fly...
pub struct Sdl2GlRenderer {
    rendering_mode: RenderingMode,
    window: Window,
    _opengl_context: GLContext,

    raster_renderer: raster::Sdl2GlRasterRenderer,
    poly_renderer: poly::Sdl2GlPolyRenderer,
}

struct State {
    raster_renderer: Box<dyn Any>,
    poly_renderer: Box<dyn Any>,
}

impl Sdl2GlRenderer {
    pub fn new(sdl_context: &Sdl, rendering_mode: RenderingMode) -> Result<Box<Self>> {
        let sdl_video = sdl_context.video().map_err(|s| anyhow!(s))?;

        let gl_attr = sdl_video.gl_attr();
        // TODO use GLES?
        gl_attr.set_context_profile(GLProfile::Core);
        gl_attr.set_context_version(3, 3);

        let window = sdl_video
            .window("Another World", WINDOW_RESOLUTION[0], WINDOW_RESOLUTION[1])
            .opengl()
            .resizable()
            .allow_highdpi()
            .build()?;

        let opengl_context = window.gl_create_context().map_err(|s| anyhow!(s))?;
        gl::load_with(|s| sdl_video.gl_get_proc_address(s) as _);

        unsafe {
            gl::LineWidth(5.0);

            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::STENCIL_TEST);
        }

        let window_size = window.size();
        Ok(Box::new(Sdl2GlRenderer {
            rendering_mode,
            window,
            _opengl_context: opengl_context,

            raster_renderer: raster::Sdl2GlRasterRenderer::new()?,
            poly_renderer: {
                let rendering_mode = match rendering_mode {
                    RenderingMode::Raster | RenderingMode::Poly => poly::RenderingMode::Poly,
                    RenderingMode::Line => poly::RenderingMode::Line,
                };

                poly::Sdl2GlPolyRenderer::new(
                    rendering_mode,
                    window_size.0 as usize,
                    window_size.1 as usize,
                )?
            },
        }))
    }
}

impl Sdl2Renderer for Sdl2GlRenderer {
    fn blit_game(&mut self, dst: &Rect) {
        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        match self.rendering_mode {
            RenderingMode::Raster => self.raster_renderer.blit(&dst),
            RenderingMode::Poly => self.poly_renderer.blit(&dst),
            RenderingMode::Line => self.poly_renderer.blit(&dst),
        };
    }

    fn present(&mut self) {
        self.window.gl_swap_window();
    }

    fn as_gfx(&self) -> &dyn crate::gfx::Backend {
        self
    }

    fn as_gfx_mut(&mut self) -> &mut dyn crate::gfx::Backend {
        self
    }

    fn window(&self) -> &Window {
        &self.window
    }

    fn handle_events(&mut self, events: &[Event]) {
        for event in events {
            match event {
                Event::Window {
                    win_event: WindowEvent::Resized(w, h),
                    ..
                } => self
                    .poly_renderer
                    .resize_render_textures(*w as usize, *h as usize),
                Event::KeyDown {
                    keycode: Some(key),
                    repeat: false,
                    ..
                } => match key {
                    Keycode::F1 => self.rendering_mode = RenderingMode::Raster,
                    Keycode::F2 => {
                        self.rendering_mode = RenderingMode::Poly;
                        self.poly_renderer
                            .set_rendering_mode(poly::RenderingMode::Poly);
                        self.poly_renderer.redraw();
                    }
                    Keycode::F3 => {
                        self.rendering_mode = RenderingMode::Line;
                        self.poly_renderer
                            .set_rendering_mode(poly::RenderingMode::Line);
                        self.poly_renderer.redraw();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

impl gfx::Backend for Sdl2GlRenderer {
    fn set_palette(&mut self, palette: &[u8; 32]) {
        self.raster_renderer.set_palette(palette);
        self.poly_renderer.set_palette(palette);
    }

    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster_renderer.fillvideopage(page_id, color_idx);
        self.poly_renderer.fillvideopage(page_id, color_idx);
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster_renderer
            .copyvideopage(src_page_id, dst_page_id, vscroll);
        self.poly_renderer
            .copyvideopage(src_page_id, dst_page_id, vscroll);
    }

    fn fillpolygon(
        &mut self,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        color_idx: u8,
        zoom: u16,
        polygon: &Polygon,
    ) {
        self.raster_renderer
            .fillpolygon(dst_page_id, pos, offset, color_idx, zoom, polygon);
        self.poly_renderer
            .fillpolygon(dst_page_id, pos, offset, color_idx, zoom, polygon);
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.raster_renderer
            .draw_char(dst_page_id, pos, color_idx, c);
        self.poly_renderer.draw_char(dst_page_id, pos, color_idx, c);
    }

    fn blitframebuffer(&mut self, page_id: usize) {
        self.raster_renderer.blitframebuffer(page_id);
        self.poly_renderer.blitframebuffer(page_id);
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster_renderer.blit_buffer(dst_page_id, buffer);
        self.poly_renderer.blit_buffer(dst_page_id, buffer);
    }

    fn get_snapshot(&self) -> Box<dyn Any> {
        Box::new(State {
            raster_renderer: self.raster_renderer.get_snapshot(),
            poly_renderer: self.poly_renderer.get_snapshot(),
        })
    }

    fn set_snapshot(&mut self, snapshot: Box<dyn Any>) {
        if let Ok(state) = snapshot.downcast::<State>() {
            self.raster_renderer.set_snapshot(state.raster_renderer);
            self.poly_renderer.set_snapshot(state.poly_renderer);
        } else {
            eprintln!("Attempting to restore invalid gfx snapshot, ignoring");
        }
    }
}
