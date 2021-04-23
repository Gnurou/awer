pub mod gl;
pub mod raster;

use crate::gfx;
use sdl2::{rect::Rect, video::Window};

/// Initial size of the window when using this renderer.
pub const WINDOW_RESOLUTION: [u32; 2] = [1280, 960];

pub trait Sdl2Renderer {
    /// Blit the rendered framebuffer into the `dst` rectangle of the actual
    /// display.
    fn blit_game(&mut self, dst: &Rect);
    /// Page-flip the display.
    fn present(&mut self);

    /// Returns a reference to the graphics backend.
    fn as_gfx(&self) -> &dyn gfx::Backend;
    /// Returns a mutable reference to the graphics backend.
    fn as_gfx_mut(&mut self) -> &mut dyn gfx::Backend;

    /// Returns the window the renderer will render into.
    fn window(&self) -> &Window;
}
