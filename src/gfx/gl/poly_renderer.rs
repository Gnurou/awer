mod programs;

use gl::types::GLint;
use gl::types::GLuint;

// TODO not elegant, but needed for now.
pub use programs::PolyRenderingMode;

use crate::gfx::gl::IndexedTexture;
use crate::gfx::polygon::Polygon;
use crate::gfx::raster::IndexedImage;
use crate::gfx::PolygonData;
use crate::gfx::SimplePolygonRenderer;
use crate::gfx::{self};
use crate::scenes::InitForScene;
use crate::sys::Snapshotable;
use anyhow::Result;

use self::programs::*;

use super::GlRenderer;

/// Command for filling the entire screen.
#[derive(Clone)]
struct FillScreenCommand {
    color: u8,
}

impl FillScreenCommand {
    pub fn new(color: u8) -> Self {
        Self { color }
    }
}

/// Draw command for a polygon, requesting it to be drawn at coordinates (`x`,
/// `y`) and with color `color`.
#[derive(Clone)]
struct PolyDrawCommand {
    poly: Polygon,
    pos: (i16, i16),
    offset: (i16, i16),
    zoom: u16,
    color: u8,
}

impl PolyDrawCommand {
    pub fn new(poly: Polygon, pos: (i16, i16), offset: (i16, i16), zoom: u16, color: u8) -> Self {
        Self {
            poly,
            pos,
            offset,
            zoom,
            color,
        }
    }
}

#[derive(Clone)]
struct BlitBufferCommand {
    image: Box<IndexedImage>,
}

impl From<IndexedImage> for BlitBufferCommand {
    fn from(image: IndexedImage) -> Self {
        Self {
            image: Box::new(image),
        }
    }
}

#[derive(Clone)]
struct CharDrawCommand {
    pos: (i16, i16),
    color: u8,
    c: u8,
}

impl CharDrawCommand {
    pub fn new(pos: (i16, i16), color: u8, c: u8) -> Self {
        Self { pos, color, c }
    }
}

#[derive(Clone)]
enum DrawCommand {
    Fill(FillScreenCommand),
    Poly(PolyDrawCommand),
    BlitBuffer(BlitBufferCommand),
    Char(CharDrawCommand),
}

#[derive(Default, Clone)]
struct DrawCommands([Vec<DrawCommand>; 4]);

impl gfx::PolygonFiller for DrawCommands {
    fn fill_polygon(
        &mut self,
        poly: &PolygonData,
        color_idx: u8,
        dst_page_id: usize,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
    ) {
        let command = &mut self.0[dst_page_id];
        command.push(DrawCommand::Poly(PolyDrawCommand::new(
            Polygon::new((poly.bb[0], poly.bb[1]), poly.points.to_vec()),
            pos,
            offset,
            zoom,
            color_idx,
        )));
    }
}

/// A renderer that uses the GPU to render the game into a 16 colors indexed bufffer of any size.
pub struct GlPolyRenderer {
    renderer: SimplePolygonRenderer,

    rendering_mode: PolyRenderingMode,

    draw_commands: DrawCommands,
    framebuffer_index: usize,

    target_fbo: GLuint,

    render_texture_buffer0: IndexedTexture,
    render_texture_framebuffer: IndexedTexture,

    renderers: Programs,
}

impl Drop for GlPolyRenderer {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.target_fbo);
        }
    }
}

impl InitForScene for GlPolyRenderer {
    #[tracing::instrument(skip(self, resman))]
    fn init_from_scene(
        &mut self,
        resman: &crate::res::ResourceManager,
        scene: &crate::scenes::Scene,
    ) {
        self.renderer.init_from_scene(resman, scene)
    }
}

impl GlPolyRenderer {
    pub fn new(
        rendering_mode: PolyRenderingMode,
        width: usize,
        height: usize,
    ) -> Result<GlPolyRenderer> {
        let mut target_fbo = 0;

        unsafe {
            gl::GenFramebuffers(1, &mut target_fbo);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, target_fbo);
            gl::DrawBuffers(1, [gl::COLOR_ATTACHMENT0].as_ptr());
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }

        Ok(GlPolyRenderer {
            renderer: Default::default(),
            rendering_mode,
            draw_commands: Default::default(),
            framebuffer_index: 0,
            target_fbo,
            render_texture_buffer0: IndexedTexture::new(width, height),
            render_texture_framebuffer: IndexedTexture::new(width, height),
            renderers: Programs::new(
                FillRenderer::new(),
                PolyRenderer::new()?,
                BitmapRenderer::new()?,
                FontRenderer::new()?,
            ),
        })
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn set_rendering_mode(&mut self, rendering_mode: PolyRenderingMode) {
        self.rendering_mode = rendering_mode;
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn resize_render_textures(&mut self, width: usize, height: usize) {
        self.render_texture_buffer0 = IndexedTexture::new(width, height);
        self.render_texture_framebuffer = IndexedTexture::new(width, height);
        self.redraw();
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn run_command_list(&mut self, commands_index: usize, rendering_mode: PolyRenderingMode) {
        let draw_commands = &self.draw_commands.0[commands_index];
        let mut draw_runner = self.renderers.start_drawing(
            &self.render_texture_framebuffer,
            &self.render_texture_buffer0,
        );
        for command in draw_commands {
            match command {
                DrawCommand::Fill(fill) => {
                    draw_runner.fill(fill.color);
                }
                DrawCommand::Poly(poly) => {
                    draw_runner.draw_poly(
                        &poly.poly,
                        poly.pos,
                        poly.offset,
                        poly.zoom,
                        poly.color,
                        rendering_mode,
                    );
                }
                DrawCommand::BlitBuffer(buffer) => {
                    draw_runner.draw_bitmap(&buffer.image);
                }
                DrawCommand::Char(c) => {
                    draw_runner.draw_char(c.pos, c.color, c.c);
                }
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn set_render_target(&self, target_texture: &IndexedTexture) {
        unsafe {
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, self.target_fbo);
            gl::FramebufferTexture(
                gl::DRAW_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                target_texture.as_tex_id(),
                0,
            );
            if gl::CheckFramebufferStatus(gl::DRAW_FRAMEBUFFER) != gl::FRAMEBUFFER_COMPLETE {
                panic!("Error while setting framebuffer up");
            }
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    pub fn redraw(&mut self) {
        unsafe {
            let dimensions = self.render_texture_framebuffer.dimensions();
            gl::Viewport(0, 0, dimensions.0 as GLint, dimensions.1 as GLint);
        }

        // First render buffer 0, since it may be needed to render the final
        // buffer.
        self.set_render_target(&self.render_texture_buffer0);
        self.run_command_list(0, self.rendering_mode);

        // Then render the framebuffer, which can now use buffer0 as a source
        // texture.
        self.set_render_target(&self.render_texture_framebuffer);
        self.run_command_list(self.framebuffer_index, self.rendering_mode);

        // TODO move into proper method?
        unsafe {
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, 0);
        }
    }
}

impl gfx::IndexedRenderer for GlPolyRenderer {
    fn fillvideopage(&mut self, page_id: usize, color_idx: u8) {
        let commands = &mut self.draw_commands.0[page_id];
        commands.clear();

        commands.push(DrawCommand::Fill(FillScreenCommand::new(color_idx)));
    }

    fn copyvideopage(&mut self, src_page_id: usize, dst_page_id: usize, _vscroll: i16) {
        let src_polys = self.draw_commands.0[src_page_id].clone();
        self.draw_commands.0[dst_page_id] = src_polys;
    }

    fn draw_char(&mut self, dst_page_id: usize, pos: (i16, i16), color: u8, c: u8) {
        let command_queue = &mut self.draw_commands.0[dst_page_id];
        command_queue.push(DrawCommand::Char(CharDrawCommand::new(pos, color, c)));
    }

    fn blit_buffer(&mut self, dst_page_id: usize, buffer: &[u8]) {
        let mut image: IndexedImage = Default::default();
        image
            .set_content(buffer)
            .unwrap_or_else(|e| tracing::error!("blit_buffer failed: {}", e));

        self.draw_commands.0[dst_page_id].clear();
        self.draw_commands.0[dst_page_id].push(DrawCommand::BlitBuffer(image.into()));
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
        self.renderer.draw_polygons(
            segment,
            start_offset,
            dst_page_id,
            pos,
            offset,
            zoom,
            &mut self.draw_commands,
        )
    }
}

#[derive(Clone)]
pub struct GlPolyRendererSnapshot {
    draw_commands: DrawCommands,
    framebuffer_index: usize,
}

impl Snapshotable for GlPolyRenderer {
    type State = GlPolyRendererSnapshot;

    #[tracing::instrument(level = "debug", skip(self))]
    fn take_snapshot(&self) -> Self::State {
        GlPolyRendererSnapshot {
            draw_commands: self.draw_commands.clone(),
            framebuffer_index: self.framebuffer_index,
        }
    }

    #[tracing::instrument(level = "debug", skip(self, snapshot))]
    fn restore_snapshot(&mut self, snapshot: &Self::State) -> bool {
        self.draw_commands = snapshot.draw_commands.clone();
        self.framebuffer_index = snapshot.framebuffer_index;
        true
    }
}

impl AsRef<IndexedTexture> for GlPolyRenderer {
    fn as_ref(&self) -> &IndexedTexture {
        &self.render_texture_framebuffer
    }
}

impl GlRenderer for GlPolyRenderer {
    #[tracing::instrument(level = "debug", skip(self))]
    fn update_texture(&mut self, page_id: usize) {
        self.framebuffer_index = page_id;
        self.redraw();
    }
}
