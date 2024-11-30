mod bitmap_renderer;
mod fill_renderer;
mod font_renderer;
mod poly_renderer;

pub use bitmap_renderer::BitmapRenderer;
pub use fill_renderer::FillRenderer;
pub use font_renderer::FontRenderer;
pub use poly_renderer::PolyRenderer;
pub use poly_renderer::PolyRenderingMode;

use crate::gfx::gl::IndexedTexture;
use crate::gfx::polygon::Polygon;
use crate::gfx::raster::IndexedImage;

/// Trait for a GL program that can draw a certain class of object from the game (e.g. polygons or
/// font).
pub trait Program {
    /// Activate the program, i.e. make it ready to draw. `target_texture` is where incoming draw
    /// commands should be rendered, while `buffer0` is a texture with framebuffer 0 (which is used
    /// as a source for some commands).
    fn activate(&mut self, _target_texture: &IndexedTexture, _buffer0: &IndexedTexture) {}
    /// Deactivate the program, flushing any pending operations.
    fn deactivate(&mut self) {}
}

/// Keep track of which program is current.
enum CurrentProgram {
    None,
    Fill,
    Poly,
    Bitmap,
    Font,
}

/// Groups all the programs used with the GL backend, and allow to select a specific one and to use
/// it through a `DrawCommandRunner`.
pub struct Programs {
    current: CurrentProgram,
    fill: FillRenderer,
    poly: PolyRenderer,
    bitmap: BitmapRenderer,
    font: FontRenderer,
}

impl Drop for Programs {
    fn drop(&mut self) {
        self.deactivate();
    }
}

impl Programs {
    pub fn new(
        fill: FillRenderer,
        poly: PolyRenderer,
        bitmap: BitmapRenderer,
        font: FontRenderer,
    ) -> Self {
        Self {
            current: CurrentProgram::None,
            fill,
            poly,
            bitmap,
            font,
        }
    }

    pub fn start_drawing<'a>(
        &'a mut self,
        target: &'a IndexedTexture,
        buffer0: &'a IndexedTexture,
    ) -> DrawCommandRunner<'a> {
        DrawCommandRunner::new(self, target, buffer0)
    }

    fn deactivate(&mut self) {
        match self.current {
            CurrentProgram::None => (),
            CurrentProgram::Fill => self.fill.deactivate(),
            CurrentProgram::Poly => self.poly.deactivate(),
            CurrentProgram::Bitmap => self.bitmap.deactivate(),
            CurrentProgram::Font => self.font.deactivate(),
        }
        self.current = CurrentProgram::None;
    }

    fn use_fill(
        &mut self,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) -> &mut FillRenderer {
        match self.current {
            CurrentProgram::Fill => (),
            _ => {
                self.deactivate();
                self.fill.activate(target_texture, buffer0);
                self.current = CurrentProgram::Fill;
            }
        }
        &mut self.fill
    }

    fn use_poly(
        &mut self,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) -> &mut PolyRenderer {
        match self.current {
            CurrentProgram::Poly => (),
            _ => {
                self.deactivate();
                self.poly.activate(target_texture, buffer0);
                self.current = CurrentProgram::Poly;
            }
        }
        &mut self.poly
    }

    fn use_bitmap(
        &mut self,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) -> &mut BitmapRenderer {
        match self.current {
            CurrentProgram::Bitmap => (),
            _ => {
                self.deactivate();
                self.bitmap.activate(target_texture, buffer0);
                self.current = CurrentProgram::Bitmap;
            }
        }
        &mut self.bitmap
    }

    fn use_font(
        &mut self,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) -> &mut FontRenderer {
        match self.current {
            CurrentProgram::Font => (),
            _ => {
                self.deactivate();
                self.font.activate(target_texture, buffer0);
                self.current = CurrentProgram::Font;
            }
        }
        &mut self.font
    }
}

/// An interface to `Programs` that allows drawing to take place, making sure pending operations
/// are flushed when this object is dropped.
pub struct DrawCommandRunner<'a> {
    programs: &'a mut Programs,
    target: &'a IndexedTexture,
    buffer0: &'a IndexedTexture,
}

impl Drop for DrawCommandRunner<'_> {
    fn drop(&mut self) {
        self.programs.deactivate();
    }
}

impl<'a> DrawCommandRunner<'a> {
    fn new(
        programs: &'a mut Programs,
        target: &'a IndexedTexture,
        buffer0: &'a IndexedTexture,
    ) -> Self {
        Self {
            programs,
            target,
            buffer0,
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn fill(&mut self, color: u8) {
        self.programs
            .use_fill(self.target, self.buffer0)
            .fill(color);
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn draw_poly(
        &mut self,
        poly: &Polygon,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        color: u8,
        rendering_mode: PolyRenderingMode,
    ) {
        self.programs.use_poly(self.target, self.buffer0).draw_poly(
            poly,
            pos,
            offset,
            zoom,
            color,
            rendering_mode,
        )
    }

    #[tracing::instrument(level = "trace", skip(self, image))]
    pub fn draw_bitmap(&mut self, image: &IndexedImage) {
        self.programs
            .use_bitmap(self.target, self.buffer0)
            .draw_bitmap(image)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn draw_char(&mut self, pos: (i16, i16), color: u8, c: u8) {
        self.programs
            .use_font(self.target, self.buffer0)
            .draw_char(pos, color, c)
    }
}
