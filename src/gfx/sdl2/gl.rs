use std::any::Any;

use sdl2::{
    event::{Event, WindowEvent},
    keyboard::Keycode,
    rect::Rect,
    video::{GLContext, GLProfile, Window},
    Sdl,
};

use anyhow::{anyhow, Result};

use crate::{
    gfx::{
        self,
        gl::{
            indexed_frame_renderer::IndexedFrameRenderer,
            poly_renderer::{GlPolyRenderer, PolyRenderingMode},
            raster_renderer::GlRasterRenderer,
            GlGameTexture, Viewport,
        },
        raster::RasterRenderer,
        sdl2::{Sdl2Display, WINDOW_RESOLUTION},
        Palette, Point,
    },
    sys::Snapshotable,
};

#[derive(Clone, Copy)]
pub enum RenderingMode {
    Raster,
    Poly,
    Line,
}

/// A GL-based display for SDL.
///
/// It operates two renderers behind the scene: one that renders the game using the CPU at original
/// resolution, the other that renders it using OpenGL at the current resolution of the window. Both
/// render into a 16-color indexed texture that is then converted into a true-color texture.
///
/// This display can safely be used along with other GL libraries, like ImGUI.
pub struct Sdl2GlGfx {
    rendering_mode: RenderingMode,
    window: Window,
    _opengl_context: GLContext,

    raster_renderer: GlRasterRenderer,
    poly_renderer: GlPolyRenderer,

    framebuffer_renderer: IndexedFrameRenderer,
    palette: Palette,
}

struct State {
    raster_renderer: Box<dyn Any>,
    poly_renderer: Box<dyn Any>,
}

impl Sdl2GlGfx {
    pub fn new(sdl_context: &Sdl, rendering_mode: RenderingMode) -> Result<Self> {
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
            gl::Enable(gl::PRIMITIVE_RESTART);
            gl::Enable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
        }

        let window_size = window.size();
        Ok(Sdl2GlGfx {
            rendering_mode,
            window,
            _opengl_context: opengl_context,

            raster_renderer: GlRasterRenderer::new()?,
            poly_renderer: {
                let rendering_mode = match rendering_mode {
                    RenderingMode::Raster | RenderingMode::Poly => PolyRenderingMode::Poly,
                    RenderingMode::Line => PolyRenderingMode::Line,
                };

                GlPolyRenderer::new(
                    rendering_mode,
                    window_size.0 as usize,
                    window_size.1 as usize,
                )?
            },
            framebuffer_renderer: IndexedFrameRenderer::new()?,
            palette: Default::default(),
        })
    }
}

impl Sdl2Display for Sdl2GlGfx {
    fn blit_game(&mut self, dst: &Rect) {
        unsafe {
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
        }

        let framebuffer_texture = match self.rendering_mode {
            RenderingMode::Raster => self.raster_renderer.as_ref(),
            RenderingMode::Poly | RenderingMode::Line => self.poly_renderer.as_ref(),
        };

        self.framebuffer_renderer.render(
            framebuffer_texture,
            &self.palette,
            0,
            &Viewport {
                x: dst.x(),
                y: dst.y(),
                width: dst.width() as i32,
                height: dst.height() as i32,
            },
        );

        self.window.gl_swap_window();
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
                            .set_rendering_mode(PolyRenderingMode::Poly);
                        self.poly_renderer.redraw();
                    }
                    Keycode::F3 => {
                        self.rendering_mode = RenderingMode::Line;
                        self.poly_renderer
                            .set_rendering_mode(PolyRenderingMode::Line);
                        self.poly_renderer.redraw();
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

impl gfx::IndexedRenderer for Sdl2GlGfx {
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        self.raster_renderer
            .as_mut()
            .fillvideopage(page_id, color_idx);
        self.poly_renderer.fillvideopage(page_id, color_idx);
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, vscroll: i16) {
        self.raster_renderer
            .as_mut()
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
        bb: (u8, u8),
        points: &[Point<u8>],
    ) {
        self.raster_renderer.as_mut().fillpolygon(
            dst_page_id,
            pos,
            offset,
            color_idx,
            zoom,
            bb,
            points,
        );
        self.poly_renderer
            .fillpolygon(dst_page_id, pos, offset, color_idx, zoom, bb, points);
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.raster_renderer
            .as_mut()
            .draw_char(dst_page_id, pos, color_idx, c);
        self.poly_renderer.draw_char(dst_page_id, pos, color_idx, c);
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster_renderer
            .as_mut()
            .blit_buffer(dst_page_id, buffer);
        self.poly_renderer.blit_buffer(dst_page_id, buffer);
    }
}

impl gfx::Display for Sdl2GlGfx {
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette) {
        self.palette = palette.clone();
        match self.rendering_mode {
            RenderingMode::Raster => self.raster_renderer.update_texture(page_id),
            RenderingMode::Poly | RenderingMode::Line => self.poly_renderer.update_texture(page_id),
        };
    }
}

impl Snapshotable for Sdl2GlGfx {
    type State = Box<dyn Any>;

    fn take_snapshot(&self) -> Self::State {
        Box::new(State {
            raster_renderer: AsRef::<RasterRenderer>::as_ref(&self.raster_renderer).take_snapshot(),
            poly_renderer: self.poly_renderer.take_snapshot(),
        })
    }

    fn restore_snapshot(&mut self, snapshot: Self::State) -> bool {
        if let Ok(state) = snapshot.downcast::<State>() {
            self.raster_renderer
                .as_mut()
                .restore_snapshot(state.raster_renderer);
            self.poly_renderer.restore_snapshot(state.poly_renderer);
            true
        } else {
            log::error!("Attempting to restore invalid gfx snapshot, ignoring");
            false
        }
    }
}

impl gfx::Gfx for Sdl2GlGfx {}

impl AsRef<dyn gfx::Gfx> for Sdl2GlGfx {
    fn as_ref(&self) -> &(dyn gfx::Gfx + 'static) {
        self
    }
}

impl AsMut<dyn gfx::Gfx> for Sdl2GlGfx {
    fn as_mut(&mut self) -> &mut (dyn gfx::Gfx + 'static) {
        self
    }
}
