use crate::gfx::{polygon::Polygon, raster::IndexedImage};

use super::{
    bitmap_renderer::BitmapRenderer,
    font_renderer::FontRenderer,
    poly_renderer::{PolyRenderer, RenderingMode},
    IndexedTexture,
};

/// Trait for a GL renderer that can draw a certain class of object from the game (e.g. polygons or
/// font).
pub trait Renderer {
    /// Activate the renderer, i.e. make it ready to draw. `target_texture` is where incoming draw
    /// commands should be renderer, while `buffer0` is a texture with framebuffer 0 (which is used
    /// as a source for some commands).
    fn activate(&self, _target_texture: &IndexedTexture, _buffer0: &IndexedTexture) {}
    /// Deactivate the renderer, flushing any pending operations.
    fn deactivate(&self) {}
}

/// Keep track of which renderer is current.
enum CurrentRenderer {
    None,
    Poly,
    Bitmap,
    Font,
}

/// Groups all the renderers used with the GL backend, and allow to select a specific renderer and
/// to use it through a `DrawCommandRunner`.
pub struct Renderers {
    current: CurrentRenderer,
    poly: PolyRenderer,
    bitmap: BitmapRenderer,
    font: FontRenderer,
}

impl Drop for Renderers {
    fn drop(&mut self) {
        self.deactivate();
    }
}

impl Renderers {
    pub fn new(poly: PolyRenderer, bitmap: BitmapRenderer, font: FontRenderer) -> Self {
        Self {
            current: CurrentRenderer::None,
            poly,
            bitmap,
            font,
        }
    }

    fn deactivate(&mut self) {
        match self.current {
            CurrentRenderer::None => (),
            CurrentRenderer::Poly => self.poly.deactivate(),
            CurrentRenderer::Bitmap => self.bitmap.deactivate(),
            CurrentRenderer::Font => self.font.deactivate(),
        }
        self.current = CurrentRenderer::None;
    }

    fn use_poly(
        &mut self,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) -> &mut PolyRenderer {
        match self.current {
            CurrentRenderer::Poly => (),
            _ => {
                self.deactivate();
                self.poly.activate(target_texture, buffer0);
                self.current = CurrentRenderer::Poly;
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
            CurrentRenderer::Bitmap => (),
            _ => {
                self.deactivate();
                self.bitmap.activate(target_texture, buffer0);
                self.current = CurrentRenderer::Bitmap;
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
            CurrentRenderer::Font => (),
            _ => {
                self.deactivate();
                self.font.activate(target_texture, buffer0);
                self.current = CurrentRenderer::Font;
            }
        }
        &mut self.font
    }
}

/// An interface to `Renderers` that allows drawing to take place, making sure pending operations
/// are flushed when this object is dropped.
pub struct DrawCommandRunner<'a> {
    renderers: &'a mut Renderers,
    target: &'a IndexedTexture,
    buffer0: &'a IndexedTexture,
}

impl<'a> Drop for DrawCommandRunner<'a> {
    fn drop(&mut self) {
        self.renderers.deactivate();
    }
}

impl<'a> DrawCommandRunner<'a> {
    pub fn new(
        renderers: &'a mut Renderers,
        target: &'a IndexedTexture,
        buffer0: &'a IndexedTexture,
    ) -> Self {
        Self {
            renderers,
            target,
            buffer0,
        }
    }

    pub fn draw_poly(
        &mut self,
        poly: &Polygon,
        pos: (i16, i16),
        offset: (i16, i16),
        zoom: u16,
        color: u8,
        rendering_mode: RenderingMode,
    ) {
        self.renderers
            .use_poly(self.target, self.buffer0)
            .draw_poly(poly, pos, offset, zoom, color, rendering_mode)
    }

    pub fn draw_bitmap(&mut self, image: &IndexedImage) {
        self.renderers
            .use_bitmap(self.target, self.buffer0)
            .draw_bitmap(image)
    }

    pub fn draw_char(&mut self, pos: (i16, i16), color: u8, c: u8) {
        self.renderers
            .use_font(self.target, self.buffer0)
            .draw_char(pos, color, c)
    }
}
