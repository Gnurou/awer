use super::{
    bitmap_renderer::BitmapRenderer, font_renderer::FontRenderer, poly_renderer::PolyRenderer,
    IndexedTexture,
};

pub trait Renderer {
    fn activate(&self, _target_texture: &IndexedTexture, _buffer0: &IndexedTexture) {}
    fn deactivate(&self) {}
}

pub enum CurrentRenderer<'a> {
    None,
    Poly(&'a PolyRenderer),
    Bitmap(&'a BitmapRenderer),
    Font(&'a FontRenderer),
}

impl<'a> Drop for CurrentRenderer<'a> {
    fn drop(&mut self) {
        self.deactivate();
    }
}

impl<'a> CurrentRenderer<'a> {
    pub fn new() -> Self {
        Self::None
    }

    fn deactivate(&mut self) {
        match self {
            CurrentRenderer::None => (),
            CurrentRenderer::Poly(renderer) => renderer.deactivate(),
            CurrentRenderer::Bitmap(renderer) => renderer.deactivate(),
            CurrentRenderer::Font(renderer) => renderer.deactivate(),
        }
    }

    pub fn use_poly(
        &mut self,
        renderer: &'a PolyRenderer,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) {
        match self {
            CurrentRenderer::Poly(_) => (),
            _ => {
                self.deactivate();
                *self = CurrentRenderer::Poly(renderer);
                renderer.activate(target_texture, buffer0);
            }
        }
    }

    pub fn use_bitmap(
        &mut self,
        renderer: &'a BitmapRenderer,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) {
        match self {
            CurrentRenderer::Bitmap(_) => (),
            _ => {
                self.deactivate();
                *self = CurrentRenderer::Bitmap(renderer);
                renderer.activate(target_texture, buffer0);
            }
        }
    }

    pub fn use_font(
        &mut self,
        renderer: &'a FontRenderer,
        target_texture: &IndexedTexture,
        buffer0: &IndexedTexture,
    ) {
        match self {
            CurrentRenderer::Font(_) => (),
            _ => {
                self.deactivate();
                *self = CurrentRenderer::Font(renderer);
                renderer.activate(target_texture, buffer0);
            }
        }
    }
}
