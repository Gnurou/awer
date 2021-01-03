pub mod raster;

use crate::gfx;
use sdl2::render::Texture;

pub trait SDL2Renderer {
    fn render_game(&mut self);
    fn get_rendered_texture(&self) -> &Texture;
    fn as_gfx(&self) -> &dyn gfx::Backend;
    fn as_gfx_mut(&mut self) -> &mut dyn gfx::Backend;
}
