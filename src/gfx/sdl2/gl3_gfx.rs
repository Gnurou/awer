use std::any::Any;

use sdl2::event::Event;
use sdl2::event::WindowEvent;
use sdl2::keyboard::Keycode;
use sdl2::rect::Rect;
use sdl2::video::GLContext;
use sdl2::video::GLProfile;
use sdl2::video::Window;
use sdl2::Sdl;

use anyhow::anyhow;
use anyhow::Result;

use crate::gfx;
use crate::gfx::gl3::game_renderer::GlGameRenderer;
use crate::gfx::gl3::game_renderer::PolyRenderingMode;
use crate::gfx::gl3::indexed_frame_renderer::IndexedFrameRenderer;
use crate::gfx::gl3::raster_renderer::GlRasterRenderer;
use crate::gfx::gl3::GlRenderer;
use crate::gfx::gl3::Viewport;
use crate::gfx::raster::RasterGameRenderer;
use crate::gfx::sdl2::Sdl2Gfx;
use crate::gfx::sdl2::WINDOW_RESOLUTION;
use crate::gfx::Display;
use crate::gfx::Palette;
use crate::scenes::InitForScene;
use crate::sys::Snapshotable;

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
    poly_renderer: GlGameRenderer,

    framebuffer_renderer: IndexedFrameRenderer,
    current_framebuffer: usize,
    palette: Palette,
}

impl Sdl2GlGfx {
    pub fn new(sdl_context: &Sdl, rendering_mode: RenderingMode) -> Result<Self> {
        let sdl_video = sdl_context.video().map_err(|s| anyhow!(s))?;

        let gl_attr = sdl_video.gl_attr();
        // TODO: use GLES?
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

                GlGameRenderer::new(
                    rendering_mode,
                    window_size.0 as usize,
                    window_size.1 as usize,
                )?
            },
            framebuffer_renderer: IndexedFrameRenderer::new()?,
            current_framebuffer: 0,
            palette: Default::default(),
        })
    }
}

impl gfx::GameRenderer for Sdl2GlGfx {
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

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color_idx: u8, c: u8) {
        self.raster_renderer
            .draw_char(dst_page_id, pos, color_idx, c);
        self.poly_renderer.draw_char(dst_page_id, pos, color_idx, c);
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        self.raster_renderer.blit_buffer(dst_page_id, buffer);
        self.poly_renderer.blit_buffer(dst_page_id, buffer);
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
        self.raster_renderer
            .draw_polygons(segment, start_offset, dst_page_id, pos, offset, zoom);
        self.poly_renderer
            .draw_polygons(segment, start_offset, dst_page_id, pos, offset, zoom);
    }
}

impl gfx::Display for Sdl2GlGfx {
    fn blitframebuffer(&mut self, page_id: usize, palette: &Palette) {
        self.current_framebuffer = page_id;
        self.palette = palette.clone();
        match self.rendering_mode {
            RenderingMode::Raster => self.raster_renderer.update_texture(page_id),
            RenderingMode::Poly | RenderingMode::Line => self.poly_renderer.update_texture(page_id),
        };
    }
}

struct Sdl2GfxSnapshot {
    raster_renderer: <RasterGameRenderer as Snapshotable>::State,
    poly_renderer: <GlGameRenderer as Snapshotable>::State,
    current_framebuffer: usize,
    palette: Palette,
}

impl Snapshotable for Sdl2GlGfx {
    type State = Box<dyn Any>;

    fn take_snapshot(&self) -> Self::State {
        Box::new(Sdl2GfxSnapshot {
            raster_renderer: self.raster_renderer.take_snapshot(),
            poly_renderer: self.poly_renderer.take_snapshot(),
            current_framebuffer: self.current_framebuffer,
            palette: self.palette.clone(),
        })
    }

    fn restore_snapshot(&mut self, snapshot: &Self::State) -> bool {
        if let Some(state) = snapshot.downcast_ref::<Sdl2GfxSnapshot>() {
            self.raster_renderer
                .restore_snapshot(&state.raster_renderer);
            self.poly_renderer.restore_snapshot(&state.poly_renderer);
            self.blitframebuffer(state.current_framebuffer, &state.palette);
            true
        } else {
            tracing::error!("Attempting to restore invalid gfx snapshot, ignoring");
            false
        }
    }
}

impl InitForScene for Sdl2GlGfx {
    fn init_from_scene(
        &mut self,
        resman: &crate::res::ResourceManager,
        scene: &crate::scenes::Scene,
    ) -> std::io::Result<()> {
        self.raster_renderer.init_from_scene(resman, scene)?;
        self.poly_renderer.init_from_scene(resman, scene)
    }
}

impl gfx::Gfx for Sdl2GlGfx {}

impl Sdl2Gfx for Sdl2GlGfx {
    #[tracing::instrument(skip(self))]
    fn show_game_framebuffer(&mut self, dst: &Rect) {
        // We do a full-screen rendering of the active buffer, but we may end up with rendering
        // artefacts if the buffer's ratio does not match the current screen resolution. Clearing
        // the screen prevents that from happening.
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
    }

    #[tracing::instrument(skip(self))]
    fn present(&mut self) {
        self.window.gl_swap_window();
    }

    fn window(&self) -> &Window {
        &self.window
    }

    #[tracing::instrument(skip(self))]
    fn handle_event(&mut self, event: &Event) {
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
            } => match *key {
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
